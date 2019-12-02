use crossbeam_utils::thread::{self, Scope};

use diary_app_bot::telegram_bot::run_bot;
use diary_app_lib::pgpool::PgPool;

fn main() {
    env_logger::init();
    let config = Config::init_config().unwrap();
    let pool = PgPool::new(&config.database_url);
    thread::scope(|scope| run_bot(&config.telegram_bot_token, pool, scope))
        .map_err(|x| format_err!("{:?}", x))
        .and_then(|r| r)
        .unwrap();
}
