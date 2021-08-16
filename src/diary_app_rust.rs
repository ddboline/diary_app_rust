use diary_app_lib::diary_app_opts::DiaryAppOpts;

#[tokio::main]
async fn main() {
    env_logger::init();

    match DiaryAppOpts::process_args().await {
        Ok(_) => {}
        Err(e) => {
            if !e.to_string().contains("Broken pipe") {
                panic!("{}", e);
            }
        }
    }
}
