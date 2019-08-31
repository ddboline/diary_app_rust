use diary_app_rust::s3_interface::S3Interface;
use diary_app_rust::local_interface::LocalInterface;

fn main() {
    env_logger::init();

    let s3 = S3Interface::new();
    let entries = s3.import_from_s3().unwrap();
    println!("{}", entries.len());
    s3.export_to_s3().unwrap();

    let loc = LocalInterface::new();
    loc.import_from_local().unwrap();
}
