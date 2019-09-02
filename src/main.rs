use diary_app_rust::diary_app_opts::DiaryAppOpts;

fn main() {
    env_logger::init();

    match DiaryAppOpts::process_args() {
        Ok(_) => {}
        Err(e) => {
            if !e.to_string().contains("Broken pipe") {
                panic!("{}", e)
            }
        }
    }
}
