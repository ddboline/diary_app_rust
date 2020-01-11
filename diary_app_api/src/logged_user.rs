use actix_identity::Identity;
use actix_web::{dev::Payload, FromRequest, HttpRequest};
use anyhow::Error;
use chrono::{DateTime, Utc};
use futures::executor::block_on;
use futures::future::{ready, Ready};
use lazy_static::lazy_static;
use parking_lot::RwLock;
use rust_auth_server::utils::{decode_token, Claim};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::From;
use std::env;

use diary_app_lib::models::AuthorizedUsers as AuthorizedUsersDB;
use diary_app_lib::pgpool::PgPool;

use super::errors::ServiceError;

lazy_static! {
    pub static ref AUTHORIZED_USERS: AuthorizedUsers = AuthorizedUsers::new();
}

impl<'a> From<Claim> for LoggedUser {
    fn from(claims: Claim) -> Self {
        Self {
            email: claims.get_email(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub struct LoggedUser {
    pub email: String,
}

impl LoggedUser {
    pub fn is_authorized(&self, pool: &PgPool) -> Result<bool, Error> {
        AuthorizedUsersDB::get_authorized_users(&pool)
            .map(|auth_list| auth_list.into_iter().any(|user| user.email == self.email))
    }
}

fn _from_request(req: &HttpRequest, pl: &mut Payload) -> Result<LoggedUser, actix_web::Error> {
    if let Ok(s) = env::var("TESTENV") {
        if &s == "true" {
            return Ok(LoggedUser {
                email: "user@test".to_string(),
            });
        }
    }
    if let Some(identity) = block_on(Identity::from_request(req, pl))?.identity() {
        let user: LoggedUser = decode_token(&identity)?.into();
        if AUTHORIZED_USERS.is_authorized(&user) {
            return Ok(user);
        }
    }
    Err(ServiceError::Unauthorized.into())
}

impl FromRequest for LoggedUser {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, actix_web::Error>>;
    type Config = ();

    fn from_request(req: &HttpRequest, pl: &mut Payload) -> Self::Future {
        ready(_from_request(req, pl))
    }
}

#[derive(Clone, Debug, Copy)]
enum AuthStatus {
    Authorized(DateTime<Utc>),
    NotAuthorized,
}

#[derive(Debug, Default)]
pub struct AuthorizedUsers(RwLock<HashMap<LoggedUser, AuthStatus>>);

impl AuthorizedUsers {
    pub fn new() -> Self {
        Self(RwLock::new(HashMap::new()))
    }

    pub fn fill_from_db(&self, pool: &PgPool) -> Result<(), Error> {
        let users: Vec<_> = AuthorizedUsersDB::get_authorized_users(&pool)?
            .into_iter()
            .map(|user| LoggedUser { email: user.email })
            .collect();

        for user in self.0.read().keys() {
            if !users.contains(&user) {
                self.store_auth(user.clone(), false)?;
            }
        }

        for user in users {
            self.store_auth(user, true)?;
        }

        Ok(())
    }

    pub fn is_authorized(&self, user: &LoggedUser) -> bool {
        if let Some(AuthStatus::Authorized(last_time)) = self.0.read().get(user) {
            let current_time = Utc::now();
            if (current_time - *last_time).num_minutes() < 15 {
                return true;
            }
        }
        false
    }

    pub fn store_auth(&self, user: LoggedUser, is_auth: bool) -> Result<(), Error> {
        let current_time = Utc::now();
        let status = if is_auth {
            AuthStatus::Authorized(current_time)
        } else {
            AuthStatus::NotAuthorized
        };
        self.0.write().insert(user, status);
        Ok(())
    }
}
