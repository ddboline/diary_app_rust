use anyhow::Error;
pub use rust_auth_server::logged_user::{LoggedUser, AUTHORIZED_USERS};
use std::env::var;

use diary_app_lib::models::AuthorizedUsers;
use diary_app_lib::pgpool::PgPool;

pub fn fill_from_db(pool: &PgPool) -> Result<(), Error> {
    let users: Vec<_> = AuthorizedUsers::get_authorized_users(&pool)?
        .into_iter()
        .map(|user| LoggedUser { email: user.email })
        .collect();

    if let Ok("true") = var("TESTENV").as_ref().map(|x| x.as_str()) {
        let user = LoggedUser {
            email: "user@test".to_string(),
        };
        AUTHORIZED_USERS.merge_users(&[user])?;
    }

    AUTHORIZED_USERS.merge_users(&users)
}
