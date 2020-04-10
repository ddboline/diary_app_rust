use anyhow::Error;
use log::debug;
pub use rust_auth_server::logged_user::{LoggedUser, AUTHORIZED_USERS, TRIGGER_DB_UPDATE};
use std::env::var;

use diary_app_lib::{models::AuthorizedUsers, pgpool::PgPool};

pub async fn fill_from_db(pool: &PgPool) -> Result<(), Error> {
    debug!("{:?}", *TRIGGER_DB_UPDATE);
    let users: Vec<_> = if TRIGGER_DB_UPDATE.check() {
        AuthorizedUsers::get_authorized_users(&pool)
            .await?
            .into_iter()
            .map(|user| LoggedUser {
                email: user.email.into(),
            })
            .collect()
    } else {
        AUTHORIZED_USERS.get_users()
    };
    if let Ok("true") = var("TESTENV").as_ref().map(String::as_str) {
        let user = LoggedUser {
            email: "user@test".to_string(),
        };
        AUTHORIZED_USERS.merge_users(&[user])?;
    }
    AUTHORIZED_USERS.merge_users(&users)?;

    debug!("{:?}", *AUTHORIZED_USERS);
    Ok(())
}
