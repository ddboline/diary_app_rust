#![allow(clippy::semicolon_if_nothing_returned)]

use diary_app_bot::telegram_bot::run_bot;

#[tokio::main]
async fn main() {
    env_logger::init();
    run_bot().await.unwrap();
}
