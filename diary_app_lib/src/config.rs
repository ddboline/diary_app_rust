use failure::{err_msg, format_err, Error};
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
    pub port: u32,
    pub domain: String,
    pub n_db_workers: usize,
    pub secret_key: String,
}

#[derive(Default, Debug, Clone)]
pub struct Config(Arc<ConfigInner>);

impl Config {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get_inner(self) -> Result<ConfigInner, Error> {
        Arc::try_unwrap(self.0).map_err(|_| err_msg("Failed unwrapping"))
    }

    pub fn from_inner(inner: ConfigInner) -> Self {
        Self(Arc::new(inner))
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

        let database_url =
            var("DATABASE_URL").map_err(|e| format_err!("DATABASE_URL must be set {}", e))?;
        let diary_bucket = var("DIARY_BUCKET").unwrap_or_else(|_| "diary_bucket".to_string());
        let diary_path =
            var("DIARY_PATH").unwrap_or_else(|_| format!("{}/Dropbox/epistle", home_dir));
        let aws_region_name = var("AWS_REGION_NAME").unwrap_or_else(|_| "us-east-1".to_string());
        let telegram_bot_token = var("TELEGRAM_BOT_TOKEN").unwrap_or_else(|_| "".to_string());
        let ssh_url = var("SSH_URL").ok().and_then(|s| s.parse().ok());
        let port = var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3042);
        let domain = var("DOMAIN").unwrap_or_else(|_| "localhost".to_string());
        let n_db_workers = var("N_DB_WORKERS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(2);
        let secret_key = var("SECRET_KEY").unwrap_or_else(|_| "0123".repeat(8));

        let conf = ConfigInner {
            database_url,
            diary_bucket,
            diary_path,
            aws_region_name,
            telegram_bot_token,
            ssh_url,
            port,
            domain,
            n_db_workers,
            secret_key,
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
