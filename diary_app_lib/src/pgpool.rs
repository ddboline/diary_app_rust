use anyhow::Error;
use deadpool_postgres::{Client, Config, Pool};
use derive_more::Deref;
use std::{fmt, sync::Arc};
use tokio_postgres::{Config as PgConfig, NoTls};

pub use tokio_postgres::Transaction as PgTransaction;

use stack_string::StackString;

#[derive(Clone, Deref)]
pub struct PgPool {
    pgurl: Arc<StackString>,
    #[deref]
    pool: Pool,
}

impl fmt::Debug for PgPool {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PgPool {}", &self.pgurl)
    }
}

impl PgPool {
    #[allow(clippy::missing_panics_doc)]
    #[must_use]
    pub fn new(pgurl: &str) -> Self {
        let pgconf: PgConfig = pgurl.parse().expect("Failed to parse Url");

        let mut config = Config::default();

        if let tokio_postgres::config::Host::Tcp(s) = &pgconf.get_hosts()[0] {
            config.host.replace(s.to_string());
        }
        if let Some(u) = pgconf.get_user() {
            config.user.replace(u.to_string());
        }
        if let Some(p) = pgconf.get_password() {
            config
                .password
                .replace(String::from_utf8_lossy(p).to_string());
        }
        if let Some(db) = pgconf.get_dbname() {
            config.dbname.replace(db.to_string());
        }

        let pool = config
            .builder(NoTls)
            .unwrap_or_else(|_| panic!("failed to create builder"))
            .max_size(4)
            .build()
            .unwrap_or_else(|_| panic!("Failed to create pool {}", pgurl));

        Self {
            pgurl: Arc::new(pgurl.into()),
            pool,
        }
    }

    /// # Errors
    /// Return error if getting client fail
    pub async fn get(&self) -> Result<Client, Error> {
        self.pool.get().await.map_err(Into::into)
    }
}
