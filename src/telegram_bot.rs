use diary_app_bot::telegram_bot::run_bot;

#[actix_rt::main]
async fn main() {
    env_logger::init();
    run_bot().await.unwrap();
}
