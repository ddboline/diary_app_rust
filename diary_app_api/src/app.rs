use actix_identity::{CookieIdentityPolicy, IdentityService};
use actix_web::{web, App, HttpServer};
use chrono::Duration;
use std::ops::Deref;
use std::time;
use tokio::time::interval;

use diary_app_lib::config::Config;
use diary_app_lib::diary_app_interface::DiaryAppInterface;
use diary_app_lib::pgpool::PgPool;

use super::logged_user::fill_from_db;
use super::routes::{
    commit_conflict, diary_frontpage, display, edit, insert, list, list_api, list_conflicts,
    remove_conflict, replace, search, search_api, show_conflict, sync, sync_api, update_conflict,
};

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

pub async fn run_app() {
    let config = Config::init_config().expect("Failed to load config");
    let pool = PgPool::new(&config.database_url);

    let dapp = DiaryAppActor(DiaryAppInterface::new(config, pool));

    async fn _update_db(pool: PgPool) {
        let mut i = interval(time::Duration::from_secs(60));
        loop {
            i.tick().await;
            fill_from_db(&pool).unwrap_or(());
        }
    }

    actix_rt::spawn(_update_db(dapp.pool.clone()));

    let port = dapp.config.port;

    HttpServer::new(move || {
        App::new()
            .data(AppState { db: dapp.clone() })
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(dapp.config.secret_key.as_bytes())
                    .name("auth")
                    .path("/")
                    .domain(dapp.config.domain.as_str())
                    .max_age_time(Duration::days(1))
                    .secure(false), // this can only be true if you have https
            ))
            .service(web::resource("/api/search").route(web::get().to(search)))
            .service(web::resource("/api/search_api").route(web::get().to(search_api)))
            .service(web::resource("/api/insert").route(web::post().to(insert)))
            .service(web::resource("/api/sync").route(web::get().to(sync)))
            .service(web::resource("/api/sync_api").route(web::get().to(sync_api)))
            .service(web::resource("/api/replace").route(web::post().to(replace)))
            .service(web::resource("/api/list").route(web::get().to(list)))
            .service(web::resource("/api/list_api").route(web::get().to(list_api)))
            .service(web::resource("/api/edit").route(web::get().to(edit)))
            .service(web::resource("/api/display").route(web::get().to(display)))
            .service(web::resource("/api/index.html").route(web::get().to(diary_frontpage)))
            .service(web::resource("/api/list_conflicts").route(web::get().to(list_conflicts)))
            .service(web::resource("/api/show_conflict").route(web::get().to(show_conflict)))
            .service(web::resource("/api/remove_conflict").route(web::get().to(remove_conflict)))
            .service(web::resource("/api/update_conflict").route(web::get().to(update_conflict)))
            .service(web::resource("/api/commit_conflict").route(web::get().to(commit_conflict)))
    })
    .bind(&format!("127.0.0.1:{}", port))
    .unwrap_or_else(|_| panic!("Failed to bind to port {}", port))
    .run()
    .await
    .expect("Failed to run app");
}
