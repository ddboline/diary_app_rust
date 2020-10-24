use diary_app_api::app::start_app;

#[actix_rt::main]
async fn main() {
    env_logger::init();
    start_app().await.unwrap();
}
