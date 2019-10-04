use diary_app_api::app::start_app;

fn main() {
    env_logger::init();
    let sys = actix_rt::System::new("diary_app_api");

    start_app();

    let _ = sys.run();
}
