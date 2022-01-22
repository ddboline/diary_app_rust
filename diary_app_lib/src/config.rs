use anyhow::{format_err, Error};
use serde::Deserialize;
use std::{
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

use stack_string::StackString;

#[derive(Default, Debug, Deserialize)]
pub struct ConfigInner {
    pub database_url: StackString,
    #[serde(default = "default_diary_bucket")]
    pub diary_bucket: StackString,
    #[serde(default = "default_diary_path")]
    pub diary_path: PathBuf,
    #[serde(default = "default_aws_region_name")]
    pub aws_region_name: StackString,
    #[serde(default)]
    pub telegram_bot_token: StackString,
    pub ssh_url: Option<StackString>,
    #[serde(default = "default_host")]
    pub host: StackString,
    #[serde(default = "default_port")]
    pub port: u32,
    #[serde(default = "default_domain")]
    pub domain: StackString,
    #[serde(default = "default_n_db_workers")]
    pub n_db_workers: usize,
    #[serde(default = "default_home_dir")]
    pub home_dir: PathBuf,
    #[serde(default = "default_secret_path")]
    pub secret_path: PathBuf,
    #[serde(default = "default_secret_path")]
    pub jwt_secret_path: PathBuf,
}

#[derive(Default, Debug, Clone)]
pub struct Config(Arc<ConfigInner>);

fn default_home_dir() -> PathBuf {
    dirs::home_dir().expect("Cannot determine home directory")
}
fn default_diary_bucket() -> StackString {
    "diary_bucket".into()
}
fn default_diary_path() -> PathBuf {
    let home_dir = default_home_dir();
    home_dir.join("Dropbox").join("epistle")
}
fn default_host() -> StackString {
    "0.0.0.0".into()
}
fn default_port() -> u32 {
    3042
}
fn default_domain() -> StackString {
    "localhost".into()
}
fn default_n_db_workers() -> usize {
    2
}
fn default_aws_region_name() -> StackString {
    "us-east-1".into()
}
fn default_secret_path() -> PathBuf {
    dirs::config_dir()
        .unwrap()
        .join("aws_app_rust")
        .join("secret.bin")
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_inner(self) -> Result<ConfigInner, Error> {
        Arc::try_unwrap(self.0).map_err(|e| format_err!("Failed unwrapping {e:?}"))
    }

    pub fn from_inner(inner: ConfigInner) -> Self {
        Self(Arc::new(inner))
    }

    pub fn init_config() -> Result<Self, Error> {
        let fname = Path::new("config.env");
        let config_dir = dirs::config_dir().ok_or_else(|| format_err!("No CONFIG directory"))?;
        let default_fname = config_dir.join("diary_app_rust").join("config.env");

        let env_file = if fname.exists() {
            fname
        } else {
            &default_fname
        };

        dotenv::dotenv().ok();

        if env_file.exists() {
            dotenv::from_path(env_file).ok();
        }

        let conf: ConfigInner = envy::from_env()?;

        Ok(Self(Arc::new(conf)))
    }
}

impl Deref for Config {
    type Target = ConfigInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
