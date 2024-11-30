use anyhow::Error;
use handlebars::Handlebars;
use log::{error, info};
use notify::{
    recommended_watcher, Event, EventHandler, EventKind, INotifyWatcher, RecursiveMode,
    Result as NotifyResult, Watcher,
};
use rweb::{
    filters::BoxedFilter,
    http::header::CONTENT_TYPE,
    openapi::{self, Info},
    Filter, Reply,
};
use stack_string::format_sstr;
use std::{
    collections::HashSet,
    net::SocketAddr,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::watch::{channel, Receiver, Sender},
    time::{interval, sleep},
};
use time::{
    macros::format_description,
    Date,
};

use diary_app_lib::{config::Config, diary_app_interface::DiaryAppInterface, pgpool::PgPool};

use super::{
    errors::error_response,
    logged_user::{fill_from_db, get_secrets},
    routes::{
        commit_conflict, diary_frontpage, display, edit, insert, list, list_conflicts,
        remove_conflict, replace, search, show_conflict, sync, update_conflict, user,
    },
};

#[derive(Clone)]
pub struct DiaryAppActor(pub DiaryAppInterface);

impl Deref for DiaryAppActor {
    type Target = DiaryAppInterface;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: DiaryAppActor,
    pub hb: Arc<Handlebars<'static>>,
}

#[derive(Clone)]
struct Notifier {
    send: Sender<HashSet<PathBuf>>,
    recv: Receiver<HashSet<PathBuf>>,
    watcher: Option<Arc<INotifyWatcher>>,
}

impl Notifier {
    fn new() -> Self {
        let (send, recv) = channel::<HashSet<PathBuf>>(HashSet::new());
        Self {
            send,
            recv,
            watcher: None,
        }
    }

    fn set_watcher(mut self, directory: &Path) -> Result<Self, Error> {
        let watcher = recommended_watcher(self.clone())
            .and_then(|mut w| w.watch(directory, RecursiveMode::Recursive).map(|()| w))?;
        self.watcher = Some(Arc::new(watcher));
        Ok(self)
    }
}

impl EventHandler for Notifier {
    fn handle_event(&mut self, event: NotifyResult<Event>) {
        match event {
            Ok(event) => match event.kind {
                EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) => {
                    info!("expected event {event:?}");
                    let new_paths: HashSet<_> = event.paths.iter().filter_map(|p| {
                        if p.file_name().map(|f| f.to_string_lossy()).and_then(|filename| Date::parse(&filename, format_description!("[year]-[month]-[day].txt")).ok()).is_some() {
                            Some(p.clone())
                        } else {
                            None
                        }
                    }).collect();
                    if !new_paths.is_empty() {
                        info!("got event kind {:?} paths {:?}", event.kind, event.paths);
                        self.send.send_replace(new_paths);
                    }
                }
                _ => (),
            },
            Err(e) => error!("Error {e}"),
        }
    }
}

/// # Errors
/// Returns error if starting app fails
pub async fn start_app() -> Result<(), Error> {
    async fn update_db(pool: PgPool) {
        let mut i = interval(Duration::from_secs(60));
        loop {
            fill_from_db(&pool).await.unwrap_or(());
            i.tick().await;
        }
    }
    async fn run_sync(diary_app_interface: &DiaryAppInterface) {
        match diary_app_interface.local.import_from_local().await {
            Ok(entries) => info!("entries: {entries:?}"),
            Err(e) => error!("got error {e}"),
        }
    }
    async fn check_files(dapp_interface: DiaryAppInterface, mut notifier: Notifier) {
        run_sync(&dapp_interface).await;
        while notifier.recv.changed().await.is_ok() {
            sleep(Duration::from_secs(10)).await;
            run_sync(&dapp_interface).await;
        }
    }

    let config = Config::init_config()?;
    get_secrets(&config.secret_path, &config.jwt_secret_path).await?;
    let pool = PgPool::new(&config.database_url)?;
    let sdk_config = aws_config::load_from_env().await;
    let dapp = DiaryAppActor(DiaryAppInterface::new(config.clone(), &sdk_config, pool));
    let notifier = Notifier::new().set_watcher(&config.diary_path)?;

    tokio::task::spawn(update_db(dapp.pool.clone()));
    tokio::task::spawn({
        let diary_app_interface = dapp.0.clone();
        async move {
            check_files(diary_app_interface, notifier).await;
        }
    });
    run_app(dapp, config.port).await
}

fn get_api_path(app: &AppState) -> BoxedFilter<(impl Reply,)> {
    let search_path = search(app.clone()).boxed();
    let insert_path = insert(app.clone()).boxed();
    let sync_path = sync(app.clone()).boxed();
    let replace_path = replace(app.clone()).boxed();
    let list_path = list(app.clone()).boxed();
    let edit_path = edit(app.clone()).boxed();
    let display_path = display(app.clone()).boxed();
    let frontpage_path = diary_frontpage().boxed();
    let list_conflicts_path = list_conflicts(app.clone()).boxed();
    let show_conflict_path = show_conflict(app.clone()).boxed();
    let remove_conflict_path = remove_conflict(app.clone()).boxed();
    let update_conflict_path = update_conflict(app.clone()).boxed();
    let commit_conflict_path = commit_conflict(app.clone()).boxed();
    let user_path = user().boxed();

    search_path
        .or(insert_path)
        .or(sync_path)
        .or(replace_path)
        .or(list_path)
        .or(edit_path)
        .or(display_path)
        .or(frontpage_path)
        .or(list_conflicts_path)
        .or(show_conflict_path)
        .or(remove_conflict_path)
        .or(update_conflict_path)
        .or(commit_conflict_path)
        .or(user_path)
        .boxed()
}

async fn run_app(db: DiaryAppActor, port: u32) -> Result<(), Error> {
    let mut hb = Handlebars::new();
    hb.register_template_string("id", include_str!("../../templates/index.html.hbr"))
        .expect("Failed to parse template");
    let hb = Arc::new(hb);

    let app = AppState { db, hb };

    let (spec, api_path) = openapi::spec()
        .info(Info {
            title: "Frontend for Diary".into(),
            description: "Web Frontend for Diary Service".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            ..Info::default()
        })
        .build(|| get_api_path(&app));
    let spec = Arc::new(spec);
    let spec_json_path = rweb::path!("api" / "openapi" / "json")
        .and(rweb::path::end())
        .map({
            let spec = spec.clone();
            move || rweb::reply::json(spec.as_ref())
        });

    let spec_yaml = serde_yaml::to_string(spec.as_ref())?;
    let spec_yaml_path = rweb::path!("api" / "openapi" / "yaml")
        .and(rweb::path::end())
        .map(move || {
            let reply = rweb::reply::html(spec_yaml.clone());
            rweb::reply::with_header(reply, CONTENT_TYPE, "text/yaml")
        });

    let routes = api_path
        .or(spec_json_path)
        .or(spec_yaml_path)
        .recover(error_response);
    let addr: SocketAddr = format_sstr!("127.0.0.1:{port}").parse()?;
    rweb::serve(routes).bind(addr).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use maplit::hashmap;
    use stack_string::format_sstr;
    use std::env::{remove_var, set_var};

    use auth_server_http::app::run_test_app;
    use auth_server_lib::get_random_string;

    use diary_app_lib::{config::Config, diary_app_interface::DiaryAppInterface, pgpool::PgPool};

    use crate::{
        app::{run_app, DiaryAppActor},
        logged_user::{get_random_key, JWT_SECRET, KEY_LENGTH, SECRET_KEY},
    };

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_app() -> Result<(), Error> {
        set_var("TESTENV", "true");

        let email = format_sstr!("{}@localhost", get_random_string(32));
        let password = get_random_string(32);

        let mut secret_key = [0u8; KEY_LENGTH];
        secret_key.copy_from_slice(&get_random_key());

        JWT_SECRET.set(secret_key);
        SECRET_KEY.set(secret_key);

        let test_port: u32 = 12345;
        set_var("PORT", test_port.to_string());
        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url)?;
        let sdk_config = aws_config::load_from_env().await;
        let dapp = DiaryAppActor(DiaryAppInterface::new(config.clone(), &sdk_config, pool));

        tokio::task::spawn(async move {
            env_logger::init();
            run_app(dapp, test_port).await.unwrap()
        });

        let auth_port: u32 = 54321;
        set_var("PORT", auth_port.to_string());
        set_var("DOMAIN", "localhost");

        let config = auth_server_lib::config::Config::init_config()?;
        tokio::task::spawn(async move { run_test_app(config).await.unwrap() });

        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        let client = reqwest::Client::builder().cookie_store(true).build()?;
        let url = format_sstr!("http://localhost:{auth_port}/api/auth");
        let data = hashmap! {
            "email" => &email,
            "password" => &password,
        };
        let result = client
            .post(url.as_str())
            .json(&data)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        println!("{}", result);

        let url = format_sstr!("http://localhost:{test_port}/api/index.html");
        let result = client
            .get(url.as_str())
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        assert!(result.contains("javascript:searchDiary"));
        remove_var("TESTENV");
        Ok(())
    }
}
