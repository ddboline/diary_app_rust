use actix::sync::SyncArbiter;
use actix::Addr;
use actix_identity::{CookieIdentityPolicy, IdentityService};
use actix_web::{web, App, HttpServer};
use chrono::Duration;
use futures::future::Future;
use futures::stream::Stream;
use std::time;
use tokio_timer::Interval;

use diary_app_lib::config::Config;
use diary_app_lib::diary_app_interface::DiaryAppInterface;
use diary_app_lib::pgpool::PgPool;

use super::logged_user::AUTHORIZED_USERS;
use super::routes::{display, edit, insert, list, list_api, replace, search, search_api, sync};

pub struct AppState {
    pub db: Addr<DiaryAppInterface>,
}

pub fn start_app() {
    let config = Config::init_config().expect("Failed to load config");
    let pool = PgPool::new(&config.database_url);
    let dapp = DiaryAppInterface::new(config.clone(), pool.clone());

    let _u = AUTHORIZED_USERS.clone();
    let _p = pool.clone();

    actix_rt::spawn(
        Interval::new(time::Instant::now(), time::Duration::from_secs(60))
            .for_each(move |_| {
                _u.fill_from_db(&_p).unwrap_or(());
                Ok(())
            })
            .map_err(|e| panic!("error {:?}", e)),
    );

    let addr: Addr<DiaryAppInterface> =
        SyncArbiter::start(config.n_db_workers, move || dapp.clone());

    let port = config.port;

    HttpServer::new(move || {
        App::new()
            .data(AppState { db: addr.clone() })
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(config.secret_key.as_bytes())
                    .name("auth")
                    .path("/")
                    .domain(config.domain.as_str())
                    .max_age_time(Duration::days(1))
                    .secure(false), // this can only be true if you have https
            ))
            .service(web::resource("/api/search").route(web::get().to_async(search)))
            .service(web::resource("/api/search_api").route(web::get().to_async(search_api)))
            .service(web::resource("/api/insert").route(web::post().to_async(insert)))
            .service(web::resource("/api/sync").route(web::get().to_async(sync)))
            .service(web::resource("/api/replace").route(web::post().to_async(replace)))
            .service(web::resource("/api/list").route(web::get().to_async(list)))
            .service(web::resource("/api/list_api").route(web::get().to_async(list_api)))
            .service(web::resource("/api/edit").route(web::get().to_async(edit)))
            .service(web::resource("/api/display").route(web::get().to_async(display)))
    })
    .bind(&format!("127.0.0.1:{}", port))
    .unwrap_or_else(|_| panic!("Failed to bind to port {}", port))
    .start();
}
