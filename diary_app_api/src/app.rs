use anyhow::Error;
use std::{net::SocketAddr, ops::Deref, time::Duration};
use tokio::time::interval;
use warp::Filter;

use diary_app_lib::{config::Config, diary_app_interface::DiaryAppInterface, pgpool::PgPool};

use super::{
    errors::error_response,
    logged_user::{fill_from_db, get_secrets, TRIGGER_DB_UPDATE},
    routes::{
        commit_conflict, diary_frontpage, display, edit, insert, list, list_api, list_conflicts,
        remove_conflict, replace, search, search_api, show_conflict, sync, sync_api,
        update_conflict, user,
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
}

pub async fn start_app() -> Result<(), Error> {
    async fn update_db(pool: PgPool) {
        let mut i = interval(Duration::from_secs(60));
        loop {
            fill_from_db(&pool).await.unwrap_or(());
            i.tick().await;
        }
    }

    async fn hourly_sync(dapp: DiaryAppActor) {
        let mut i = interval(Duration::from_secs(3600));
        loop {
            i.tick().await;
            dapp.sync_everything().await.map(|_| ()).unwrap_or(());
        }
    }

    let config = Config::init_config()?;
    get_secrets(&config.secret_path, &config.jwt_secret_path).await?;
    let pool = PgPool::new(&config.database_url);
    let dapp = DiaryAppActor(DiaryAppInterface::new(config.clone(), pool));

    tokio::task::spawn(update_db(dapp.pool.clone()));
    tokio::task::spawn(hourly_sync(dapp.clone()));

    run_app(dapp, config.port).await
}

async fn run_app(dapp: DiaryAppActor, port: u32) -> Result<(), Error> {
    TRIGGER_DB_UPDATE.set();

    let dapp = AppState { db: dapp.clone() };

    let data = warp::any().map(move || dapp.clone());

    let search_path = warp::path("search")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(search)
        .boxed();
    let search_api_path = warp::path("search_api")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(search_api)
        .boxed();
    let insert_path = warp::path("insert")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(insert)
        .boxed();
    let sync_path = warp::path("sync")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(sync)
        .boxed();
    let sync_api_path = warp::path("sync_api")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(sync_api)
        .boxed();
    let replace_path = warp::path("replace")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(replace)
        .boxed();
    let list_path = warp::path("list")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(list)
        .boxed();
    let list_api_path = warp::path("list_api")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(list_api)
        .boxed();
    let edit_path = warp::path("edit")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(edit)
        .boxed();
    let display_path = warp::path("display")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(display)
        .boxed();
    let frontpage_path = warp::path("index.html")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(diary_frontpage)
        .boxed();
    let list_conflicts_path = warp::path("list_conflicts")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(list_conflicts)
        .boxed();
    let show_conflict_path = warp::path("show_conflict")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(show_conflict)
        .boxed();
    let remove_conflict_path = warp::path("remove_conflict")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(remove_conflict)
        .boxed();
    let update_conflict_path = warp::path("update_conflict")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(update_conflict)
        .boxed();
    let commit_conflict_path = warp::path("commit_conflict")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(commit_conflict)
        .boxed();
    let user_path = warp::path("user")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::cookie("jwt"))
        .and_then(user)
        .boxed();

    let api_path = warp::path("api")
        .and(
            search_path
                .or(search_api_path)
                .or(insert_path)
                .or(sync_path)
                .or(sync_api_path)
                .or(replace_path)
                .or(list_path)
                .or(list_api_path)
                .or(edit_path)
                .or(display_path)
                .or(frontpage_path)
                .or(list_conflicts_path)
                .or(show_conflict_path)
                .or(remove_conflict_path)
                .or(update_conflict_path)
                .or(commit_conflict_path)
                .or(user_path),
        )
        .boxed();

    let routes = api_path.recover(error_response);
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
    warp::serve(routes).bind(addr).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use maplit::hashmap;
    use std::env::{remove_var, set_var};

    use auth_server_http::app::run_test_app;
    use auth_server_lib::get_random_string;

    use diary_app_lib::{config::Config, diary_app_interface::DiaryAppInterface, pgpool::PgPool};

    use crate::{
        app::{run_app, DiaryAppActor},
        logged_user::{get_random_key, JWT_SECRET, KEY_LENGTH, SECRET_KEY},
    };

    #[tokio::test]
    async fn test_run_app() -> Result<(), Error> {
        set_var("TESTENV", "true");

        let email = format!("{}@localhost", get_random_string(32));
        let password = get_random_string(32);

        let auth_port: u32 = 54321;
        set_var("PORT", auth_port.to_string());
        set_var("DOMAIN", "localhost");

        let config = auth_server_lib::config::Config::init_config()?;

        let mut secret_key = [0u8; KEY_LENGTH];
        secret_key.copy_from_slice(&get_random_key());

        JWT_SECRET.set(secret_key);
        SECRET_KEY.set(secret_key);

        tokio::task::spawn(async move { run_test_app(config).await.unwrap() });

        let test_port: u32 = 12345;
        set_var("PORT", test_port.to_string());
        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url);
        let dapp = DiaryAppActor(DiaryAppInterface::new(config.clone(), pool));

        tokio::task::spawn(async move {
            env_logger::init();
            run_app(dapp, test_port).await.unwrap()
        });
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        let client = reqwest::Client::builder().cookie_store(true).build()?;
        let url = format!("http://localhost:{}/api/auth", auth_port);
        let data = hashmap! {
            "email" => &email,
            "password" => &password,
        };
        let result = client
            .post(&url)
            .json(&data)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        println!("{}", result);

        let url = format!("http://localhost:{}/api/index.html", test_port);
        let result = client
            .get(&url)
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
