use failure::{err_msg, Error};
use std::env::var;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

#[derive(Default, Debug)]
pub struct ConfigInner {
    pub database_url: String,
    pub diary_bucket: String,
    pub diary_path: String,
    pub aws_region_name: String,
}

#[derive(Default, Debug, Clone)]
pub struct Config(Arc<ConfigInner>);

impl Config {
    pub fn new() -> Config {
        Default::default()
    }

    pub fn init_config() -> Result<Config, Error> {
        let fname = "config.env";

        let home_dir = var("HOME").map_err(|e| err_msg(format!("No HOME directory {}", e)))?;

        let default_fname = format!("{}/.config/diary_app_rust/config.env", home_dir);

        let env_file = if Path::new(fname).exists() {
            fname.to_string()
        } else {
            default_fname
        };

        dotenv::dotenv().ok();

        if Path::new(&env_file).exists() {
            dotenv::from_path(&env_file).ok();
        } else if Path::new("config.env").exists() {
            dotenv::from_filename("config.env").ok();
        }

        let conf = ConfigInner {
            database_url: var("DATABASE_URL")
                .map_err(|e| err_msg(format!("DATABASE_URL must be set {}", e)))?,
            diary_bucket: var("DIARY_BUCKET").unwrap_or_else(|_| "diary_bucket".to_string()),
            diary_path: var("DIARY_PATH")
                .unwrap_or_else(|_| format!("{}/Dropbox/epistle", home_dir)),
            aws_region_name: var("AWS_REGION_NAME").unwrap_or_else(|_| "us-east-1".to_string()),
        };

        Ok(Config(Arc::new(conf)))
    }
}

impl Deref for Config {
    type Target = ConfigInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
