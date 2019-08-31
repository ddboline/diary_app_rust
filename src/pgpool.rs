use diesel::pg::PgConnection;
use diesel::r2d2::ConnectionManager;
use failure::{err_msg, Error};
use r2d2::{Pool, PooledConnection};
use std::fmt;

#[derive(Clone)]
pub struct PgPool {
    pgurl: String,
    pool: Pool<ConnectionManager<PgConnection>>,
}

impl fmt::Debug for PgPool {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PgPool {}", self.pgurl)
    }
}

impl PgPool {
    pub fn new(pgurl: &str) -> PgPool {
        let manager = ConnectionManager::new(pgurl);
        PgPool {
            pgurl: pgurl.to_string(),
            pool: Pool::new(manager).expect("Failed to open DB connection"),
        }
    }

    pub fn get(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, Error> {
        self.pool.get().map_err(err_msg)
    }
}
