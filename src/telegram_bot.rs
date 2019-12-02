use diary_app_bot::telegram_bot::run_bot;

fn main() {
    env_logger::init();
    run_bot().unwrap();
}
