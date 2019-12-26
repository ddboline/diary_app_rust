use actix_identity::Identity;
use actix_web::{dev::Payload, FromRequest, HttpRequest};
use chrono::{DateTime, Utc};
use failure::{format_err, Error};
use futures::executor::block_on;
use futures::future::{ready, Ready};
use jsonwebtoken::{decode, Validation};
use lazy_static::lazy_static;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::From;
use std::env;

use diary_app_lib::models::AuthorizedUsers as AuthorizedUsersDB;
use diary_app_lib::pgpool::PgPool;

use super::errors::ServiceError;

lazy_static! {
    pub static ref AUTHORIZED_USERS: AuthorizedUsers = AuthorizedUsers::new();
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims<'a> {
    // issuer
    iss: Cow<'a, str>,
    // subject
    sub: Cow<'a, str>,
    //issued at
    iat: i64,
    // expiry
    exp: i64,
    // user email
    email: Cow<'a, str>,
}

impl<'a> From<Claims<'a>> for LoggedUser {
    fn from(claims: Claims) -> Self {
        LoggedUser {
            email: claims.email.into(),
        }
    }
}

fn get_secret() -> String {
    env::var("JWT_SECRET").unwrap_or_else(|_| "my secret".into())
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

fn _from_request(req: &HttpRequest, pl: &mut Payload) -> Result<LoggedUser, Error> {
    if let Some(identity) = block_on(Identity::from_request(req, pl))
        .map_err(|e| format_err!("{:?}", e))?
        .identity()
    {
        let user: LoggedUser = decode_token(&identity)?;
        if AUTHORIZED_USERS.is_authorized(&user) {
            return Ok(user);
        }
    }
    Err(ServiceError::Unauthorized.into())
}

impl FromRequest for LoggedUser {
    type Error = Error;
    type Future = Ready<Result<LoggedUser, Error>>;
    type Config = ();

    fn from_request(req: &HttpRequest, pl: &mut Payload) -> Self::Future {
        ready(_from_request(req, pl))
    }
}

pub fn decode_token(token: &str) -> Result<LoggedUser, ServiceError> {
    decode::<Claims>(token, get_secret().as_ref(), &Validation::default())
        .map(|data| Ok(data.claims.into()))
        .map_err(|_err| ServiceError::Unauthorized)?
}

#[derive(Clone, Debug, Copy)]
enum AuthStatus {
    Authorized(DateTime<Utc>),
    NotAuthorized,
}

#[derive(Debug, Default)]
pub struct AuthorizedUsers(RwLock<HashMap<LoggedUser, AuthStatus>>);

impl AuthorizedUsers {
    pub fn new() -> AuthorizedUsers {
        AuthorizedUsers(RwLock::new(HashMap::new()))
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
        if let Ok(s) = env::var("TESTENV") {
            if &s == "true" {
                return true;
            }
        }
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
