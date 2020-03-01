use anyhow::Error;
use diesel::{pg::PgConnection, r2d2::ConnectionManager};
use r2d2::{Pool, PooledConnection};
use std::{fmt, sync::Arc};

pub type PgPoolConn = PooledConnection<ConnectionManager<PgConnection>>;

#[derive(Clone)]
pub struct PgPool {
    pgurl: Arc<String>,
    pool: Pool<ConnectionManager<PgConnection>>,
}

impl fmt::Debug for PgPool {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PgPool {}", &self.pgurl)
    }
}

impl PgPool {
    pub fn new(pgurl: &str) -> Self {
        let manager = ConnectionManager::new(pgurl);
        Self {
            pgurl: Arc::new(pgurl.into()),
            pool: Pool::new(manager).expect("Failed to open DB connection"),
        }
    }

    pub fn get(&self) -> Result<PgPoolConn, Error> {
        self.pool.get().map_err(Into::into)
    }
}
