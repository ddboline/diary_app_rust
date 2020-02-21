use anyhow::Error;
pub use rust_auth_server::logged_user::{LoggedUser, AUTHORIZED_USERS, TRIGGER_DB_UPDATE};
use std::env::var;

use diary_app_lib::models::AuthorizedUsers;
use diary_app_lib::pgpool::PgPool;

pub async fn fill_from_db(pool: &PgPool) -> Result<(), Error> {
    if TRIGGER_DB_UPDATE.check() {
    let users: Vec<_> = AuthorizedUsers::get_authorized_users(&pool)
        .await?
        .into_iter()
        .map(|user| LoggedUser { email: user.email })
        .collect();

    if let Ok("true") = var("TESTENV").as_ref().map(String::as_str) {
        let user = LoggedUser {
            email: "user@test".to_string(),
        };
        AUTHORIZED_USERS.merge_users(&[user])?;
    }

    AUTHORIZED_USERS.merge_users(&users)
} else {Ok(())}
}
