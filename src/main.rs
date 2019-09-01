use diary_app_rust::config::Config;
use diary_app_rust::diary_app_interface::DiaryAppInterface;
use diary_app_rust::pgpool::PgPool;

fn main() {
    env_logger::init();

    let config = Config::init_config().unwrap();
    let pool = PgPool::new(&config.database_url);
    let dap = DiaryAppInterface::new(config, pool);

    // let result = dap.search_text(" FPG ").unwrap();
    // println!("result {}", result[0]);
    // let result = dap.sync_entries().unwrap();
    // println!("entries {}", result.len());
    // dap.local.export_year_to_local().unwrap();

    dap.local.cleanup_local().unwrap();
}
