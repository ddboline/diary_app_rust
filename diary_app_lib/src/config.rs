use anyhow::{format_err, Error};
use std::{env::var, ops::Deref, path::Path, sync::Arc};
use url::Url;

use crate::stack_string::StackString;

#[derive(Default, Debug)]
pub struct ConfigInner {
    pub database_url: StackString,
    pub diary_bucket: StackString,
    pub diary_path: StackString,
    pub aws_region_name: StackString,
    pub telegram_bot_token: StackString,
    pub ssh_url: Option<Url>,
    pub port: u32,
    pub domain: StackString,
    pub n_db_workers: usize,
    pub secret_key: StackString,
}

#[derive(Default, Debug, Clone)]
pub struct Config(Arc<ConfigInner>);

macro_rules! set_config_parse {
    ($s:ident, $id:ident) => {
        $s.$id = var(&stringify!($id).to_uppercase())
            .ok()
            .and_then(|x| x.parse().ok());
    };
}

macro_rules! set_config_parse_default {
    ($s:ident, $id:ident, $d:expr) => {
        $s.$id = var(&stringify!($id).to_uppercase())
            .ok()
            .and_then(|x| x.parse().ok())
            .unwrap_or_else(|| $d);
    };
}

macro_rules! set_config_must {
    ($s:ident, $id:ident) => {
        $s.$id = var(&stringify!($id).to_uppercase())
            .map(Into::into)
            .map_err(|e| format_err!("{} must be set: {}", stringify!($id).to_uppercase(), e))?;
    };
}

macro_rules! set_config_default {
    ($s:ident, $id:ident, $d:expr) => {
        $s.$id = var(&stringify!($id).to_uppercase()).map_or_else(|_| $d, Into::into);
    };
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_inner(self) -> Result<ConfigInner, Error> {
        Arc::try_unwrap(self.0).map_err(|_| format_err!("Failed unwrapping"))
    }

    pub fn from_inner(inner: ConfigInner) -> Self {
        Self(Arc::new(inner))
    }

    pub fn init_config() -> Result<Self, Error> {
        let fname = Path::new("config.env");
        let home_dir =
            dirs::home_dir().ok_or_else(|| format_err!("Cannot determine home directory"))?;
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

        let mut conf = ConfigInner::default();

        set_config_must!(conf, database_url);
        set_config_default!(conf, diary_bucket, "diary_bucket".into());
        set_config_default!(
            conf,
            diary_path,
            home_dir
                .join("Dropbox")
                .join("epistle")
                .to_string_lossy()
                .to_string()
                .into()
        );
        set_config_default!(conf, aws_region_name, "us-east-1".into());
        set_config_default!(conf, telegram_bot_token, "".into());
        set_config_parse!(conf, ssh_url);
        set_config_parse_default!(conf, port, 3042);
        set_config_default!(conf, domain, "localhost".into());
        set_config_parse_default!(conf, n_db_workers, 2);
        set_config_default!(conf, secret_key, "0123".repeat(8).into());

        Ok(Self(Arc::new(conf)))
    }
}

impl Deref for Config {
    type Target = ConfigInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
