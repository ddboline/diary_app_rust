use failure::{format_err, Error};
use std::env::var;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use url::Url;

#[derive(Default, Debug)]
pub struct ConfigInner {
    pub database_url: String,
    pub diary_bucket: String,
    pub diary_path: String,
    pub aws_region_name: String,
    pub telegram_bot_token: String,
    pub ssh_url: Option<Url>,
}

#[derive(Default, Debug, Clone)]
pub struct Config(Arc<ConfigInner>);

impl Config {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn init_config() -> Result<Self, Error> {
        let fname = "config.env";

        let home_dir = var("HOME").map_err(|e| format_err!("No HOME directory {}", e))?;

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
                .map_err(|e| format_err!("DATABASE_URL must be set {}", e))?,
            diary_bucket: var("DIARY_BUCKET").unwrap_or_else(|_| "diary_bucket".to_string()),
            diary_path: var("DIARY_PATH")
                .unwrap_or_else(|_| format!("{}/Dropbox/epistle", home_dir)),
            aws_region_name: var("AWS_REGION_NAME").unwrap_or_else(|_| "us-east-1".to_string()),
            telegram_bot_token: var("TELEGRAM_BOT_TOKEN").unwrap_or_else(|_| "".to_string()),
            ssh_url: var("SSH_URL").ok().and_then(|s| s.parse().ok()),
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
