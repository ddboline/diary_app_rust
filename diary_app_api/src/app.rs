use actix_identity::{CookieIdentityPolicy, IdentityService};
use actix_web::{middleware::Compress, web, App, HttpServer};
use anyhow::Error;
use lazy_static::lazy_static;
use stack_string::StackString;
use std::{ops::Deref, time::Duration};
use tokio::time::interval;

use diary_app_lib::{config::Config, diary_app_interface::DiaryAppInterface, pgpool::PgPool};

use super::{
    logged_user::{fill_from_db, get_secrets, KEY_LENGTH, SECRET_KEY, TRIGGER_DB_UPDATE},
    routes::{
        commit_conflict, diary_frontpage, display, edit, insert, list, list_api, list_conflicts,
        remove_conflict, replace, search, search_api, show_conflict, sync, sync_api,
        update_conflict, user,
    },
};

lazy_static! {
    pub static ref CONFIG: Config = Config::init_config().expect("Failed to init config");
}

#[derive(Clone)]
pub struct DiaryAppActor(pub DiaryAppInterface);

impl Deref for DiaryAppActor {
    type Target = DiaryAppInterface;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

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

    let config = CONFIG.clone();
    get_secrets(&config.secret_path, &config.jwt_secret_path).await?;
    let pool = PgPool::new(&config.database_url);
    let dapp = DiaryAppActor(DiaryAppInterface::new(config.clone(), pool));

    actix_rt::spawn(update_db(dapp.pool.clone()));
    actix_rt::spawn(hourly_sync(dapp.clone()));

    run_app(dapp, config.port, SECRET_KEY.get(), config.domain.clone()).await
}

async fn run_app(
    dapp: DiaryAppActor,
    port: u32,
    cookie_secret: [u8; KEY_LENGTH],
    domain: StackString,
) -> Result<(), Error> {
    TRIGGER_DB_UPDATE.set();

    HttpServer::new(move || {
        App::new()
            .data(AppState { db: dapp.clone() })
            .wrap(Compress::default())
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(&cookie_secret)
                    .name("auth")
                    .path("/")
                    .domain(domain.as_str())
                    .max_age(24 * 3600)
                    .secure(false), // this can only be true if you have https
            ))
            .service(
                web::scope("/api")
                    .service(web::resource("/search").route(web::get().to(search)))
                    .service(web::resource("/search_api").route(web::get().to(search_api)))
                    .service(web::resource("/insert").route(web::post().to(insert)))
                    .service(web::resource("/sync").route(web::get().to(sync)))
                    .service(web::resource("/sync_api").route(web::get().to(sync_api)))
                    .service(web::resource("/replace").route(web::post().to(replace)))
                    .service(web::resource("/list").route(web::get().to(list)))
                    .service(web::resource("/list_api").route(web::get().to(list_api)))
                    .service(web::resource("/edit").route(web::get().to(edit)))
                    .service(web::resource("/display").route(web::get().to(display)))
                    .service(web::resource("/index.html").route(web::get().to(diary_frontpage)))
                    .service(web::resource("/list_conflicts").route(web::get().to(list_conflicts)))
                    .service(web::resource("/show_conflict").route(web::get().to(show_conflict)))
                    .service(
                        web::resource("/remove_conflict").route(web::get().to(remove_conflict)),
                    )
                    .service(
                        web::resource("/update_conflict").route(web::get().to(update_conflict)),
                    )
                    .service(
                        web::resource("/commit_conflict").route(web::get().to(commit_conflict)),
                    )
                    .service(web::resource("/user").route(web::get().to(user))),
            )
    })
    .bind(&format!("127.0.0.1:{}", port))?
    .run()
    .await
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use maplit::hashmap;
    use std::env::{remove_var, set_var};

    use auth_server_rust::app::{get_random_string, run_test_app};

    use diary_app_lib::{config::Config, diary_app_interface::DiaryAppInterface, pgpool::PgPool};

    use crate::{
        app::{run_app, DiaryAppActor},
        logged_user::{get_random_key, JWT_SECRET, KEY_LENGTH, SECRET_KEY},
    };

    #[actix_rt::test]
    async fn test_run_app() -> Result<(), Error> {
        set_var("TESTENV", "true");

        let email = format!("{}@localhost", get_random_string(32));
        let password = get_random_string(32);

        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url);
        let dapp = DiaryAppActor(DiaryAppInterface::new(config.clone(), pool));

        let mut secret_key = [0u8; KEY_LENGTH];
        secret_key.copy_from_slice(&get_random_key());

        JWT_SECRET.set(secret_key);
        SECRET_KEY.set(secret_key);

        let auth_port: u32 = 54321;
        actix_rt::spawn(async move {
            run_test_app(auth_port, secret_key, "localhost".into())
                .await
                .unwrap()
        });

        let test_port: u32 = 12345;
        actix_rt::spawn(async move {
            run_app(dapp, test_port, secret_key, "localhost".into())
                .await
                .unwrap()
        });
        actix_rt::time::delay_for(std::time::Duration::from_secs(10)).await;

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
