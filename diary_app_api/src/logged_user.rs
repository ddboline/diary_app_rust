use anyhow::Error;
pub use auth_server_rust::logged_user::{
    LoggedUser, AUTHORIZED_USERS, JWT_SECRET, SECRET_KEY, TRIGGER_DB_UPDATE,
};
use log::debug;
use std::env::var;

use diary_app_lib::{models::AuthorizedUsers, pgpool::PgPool};

pub async fn fill_from_db(pool: &PgPool) -> Result<(), Error> {
    debug!("{:?}", *TRIGGER_DB_UPDATE);
    let users: Vec<_> = if TRIGGER_DB_UPDATE.check() {
        AuthorizedUsers::get_authorized_users(&pool)
            .await?
            .into_iter()
            .map(|user| LoggedUser { email: user.email })
            .collect()
    } else {
        AUTHORIZED_USERS.get_users()
    };
    if let Ok("true") = var("TESTENV").as_ref().map(String::as_str) {
        let user = LoggedUser {
            email: "user@test".into(),
        };
        AUTHORIZED_USERS.merge_users(&[user])?;
    }
    AUTHORIZED_USERS.merge_users(&users)?;

    debug!("{:?}", *AUTHORIZED_USERS);
    Ok(())
}
