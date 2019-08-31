use diary_app_rust::config::Config;
use diary_app_rust::diary_app_interface::DiaryAppInterface;

fn main() {
    env_logger::init();

    let config = Config::init_config().unwrap();
    let dap = DiaryAppInterface::new(config);
    let result = dap.search_text(" FPG ").unwrap();
    println!("result {}", result[0]);
}
