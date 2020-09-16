use diary_app_api::app::run_app;

#[actix_rt::main]
async fn main() {
    env_logger::init();
    run_app().await.unwrap();
}
