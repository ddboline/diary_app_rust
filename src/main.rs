use diary_app_rust::diary_app_opts::DiaryAppOpts;

fn main() {
    env_logger::init();

    DiaryAppOpts::process_args().unwrap();
}
