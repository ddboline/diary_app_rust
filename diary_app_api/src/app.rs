use actix::sync::SyncArbiter;
use actix::Addr;
use actix::{Actor, SyncContext};
use actix_identity::{CookieIdentityPolicy, IdentityService};
use actix_web::{web, App, HttpServer};
use chrono::Duration;
use futures::future::Future;
use futures::stream::Stream;
use std::ops::Deref;
use std::time;
use tokio_timer::Interval;

use diary_app_lib::config::Config;
use diary_app_lib::diary_app_interface::DiaryAppInterface;
use diary_app_lib::pgpool::PgPool;

use super::logged_user::AUTHORIZED_USERS;
use super::routes::{
    commit_conflict, diary_frontpage, display, edit, insert, list, list_api, list_conflicts,
    remove_conflict, replace, search, search_api, show_conflict, sync, sync_api, update_conflict,
};

#[derive(Clone)]
pub struct DiaryAppActor(pub DiaryAppInterface);

impl Actor for DiaryAppActor {
    type Context = SyncContext<Self>;
}

impl Deref for DiaryAppActor {
    type Target = DiaryAppInterface;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct AppState {
    pub db: Addr<DiaryAppActor>,
}

pub fn start_app() {
    let config = Config::init_config().expect("Failed to load config");
    let pool = PgPool::new(&config.database_url);
    let dapp = DiaryAppActor(DiaryAppInterface::new(config, pool));

    let _p = dapp.pool.clone();

    actix_rt::spawn(
        Interval::new(time::Instant::now(), time::Duration::from_secs(60))
            .for_each(move |_| {
                AUTHORIZED_USERS.fill_from_db(&_p).unwrap_or(());
                Ok(())
            })
            .map_err(|e| panic!("error {:?}", e)),
    );

    let _d = dapp.clone();
    let addr: Addr<DiaryAppActor> =
        SyncArbiter::start(dapp.config.n_db_workers, move || _d.clone());

    let port = dapp.config.port;

    HttpServer::new(move || {
        App::new()
            .data(AppState { db: addr.clone() })
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(dapp.config.secret_key.as_bytes())
                    .name("auth")
                    .path("/")
                    .domain(dapp.config.domain.as_str())
                    .max_age_time(Duration::days(1))
                    .secure(false), // this can only be true if you have https
            ))
            .service(web::resource("/api/search").route(web::get().to_async(search)))
            .service(web::resource("/api/search_api").route(web::get().to_async(search_api)))
            .service(web::resource("/api/insert").route(web::post().to_async(insert)))
            .service(web::resource("/api/sync").route(web::get().to_async(sync)))
            .service(web::resource("/api/sync_api").route(web::get().to_async(sync_api)))
            .service(web::resource("/api/replace").route(web::post().to_async(replace)))
            .service(web::resource("/api/list").route(web::get().to_async(list)))
            .service(web::resource("/api/list_api").route(web::get().to_async(list_api)))
            .service(web::resource("/api/edit").route(web::get().to_async(edit)))
            .service(web::resource("/api/display").route(web::get().to_async(display)))
            .service(web::resource("/api/index.html").route(web::get().to_async(diary_frontpage)))
            .service(
                web::resource("/api/list_conflicts").route(web::get().to_async(list_conflicts)),
            )
            .service(web::resource("/api/show_conflict").route(web::get().to_async(show_conflict)))
            .service(
                web::resource("/api/remove_conflict").route(web::get().to_async(remove_conflict)),
            )
            .service(
                web::resource("/api/update_conflict").route(web::get().to_async(update_conflict)),
            )
            .service(
                web::resource("/api/commit_conflict").route(web::get().to_async(commit_conflict)),
            )
    })
    .bind(&format!("127.0.0.1:{}", port))
    .unwrap_or_else(|_| panic!("Failed to bind to port {}", port))
    .start();
}

pub fn run_app() {
    let sys = actix_rt::System::new("diary_app_api");

    start_app();

    let _ = sys.run();
}
