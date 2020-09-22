use actix_identity::{CookieIdentityPolicy, IdentityService};
use actix_web::{web, App, HttpServer};
use anyhow::Error;
use lazy_static::lazy_static;
use std::{ops::Deref, time::Duration};
use tokio::time::interval;

use diary_app_lib::{config::Config, diary_app_interface::DiaryAppInterface, pgpool::PgPool};

use super::{
    logged_user::{fill_from_db, get_secrets, SECRET_KEY, TRIGGER_DB_UPDATE},
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

pub async fn run_app() -> Result<(), Error> {
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

    TRIGGER_DB_UPDATE.set();
    get_secrets(&CONFIG.secret_path, &CONFIG.jwt_secret_path).await?;

    let pool = PgPool::new(&CONFIG.database_url);

    let dapp = DiaryAppActor(DiaryAppInterface::new(CONFIG.clone(), pool));

    actix_rt::spawn(update_db(dapp.pool.clone()));
    actix_rt::spawn(hourly_sync(dapp.clone()));

    let port = dapp.config.port;

    HttpServer::new(move || {
        App::new()
            .data(AppState { db: dapp.clone() })
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(&SECRET_KEY.get())
                    .name("auth")
                    .path("/")
                    .domain(dapp.config.domain.as_str())
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
    .bind(&format!("127.0.0.1:{}", port))
    .unwrap_or_else(|_| panic!("Failed to bind to port {}", port))
    .run()
    .await
    .map_err(Into::into)
}
