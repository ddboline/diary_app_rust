pub use authorized_users::{
    AUTHORIZED_USERS, AuthorizedUser as ExternalUser, JWT_SECRET, KEY_LENGTH, LOGIN_HTML,
    SECRET_KEY, get_random_key, get_secrets, token::Token,
};
use axum::{extract::FromRequestParts, http::request::Parts};
use axum_extra::extract::CookieJar;
use futures::TryStreamExt;
use log::debug;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    env::var,
    str::FromStr,
};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use diary_app_lib::{models::AuthorizedUsers, pgpool::PgPool};

use crate::errors::ServiceError as Error;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, ToSchema)]
// LoggedUser
pub struct LoggedUser {
    // Email Address
    pub email: StackString,
    // Session Id
    pub session: Uuid,
    // User Created At
    pub created_at: OffsetDateTime,
}

impl LoggedUser {
    /// # Errors
    /// Returns error if `session_id` doesn't match `self.session`
    pub fn verify_session_id(self, session_id: Uuid) -> Result<Self, Error> {
        if self.session == session_id {
            Ok(self)
        } else {
            Err(Error::Unauthorized)
        }
    }

    fn extract_user_from_cookies(cookie_jar: &CookieJar) -> Option<LoggedUser> {
        let session_id: Uuid = StackString::from_display(cookie_jar.get("session-id")?.encoded())
            .strip_prefix("session-id=")?
            .parse()
            .ok()?;
        debug!("session_id {session_id:?}");
        let user: LoggedUser = StackString::from_display(cookie_jar.get("jwt")?.encoded())
            .strip_prefix("jwt=")?
            .parse()
            .ok()?;
        debug!("user {user:?}");
        user.verify_session_id(session_id).ok()
    }
}

impl From<ExternalUser> for LoggedUser {
    fn from(user: ExternalUser) -> Self {
        Self {
            email: user.email,
            session: user.session,
            created_at: user.created_at,
        }
    }
}

impl TryFrom<Token> for LoggedUser {
    type Error = Error;
    fn try_from(token: Token) -> Result<Self, Self::Error> {
        if let Ok(user) = token.try_into() {
            if AUTHORIZED_USERS.is_authorized(&user) {
                return Ok(user.into());
            }
            debug!("NOT AUTHORIZED {user:?}",);
        }
        Err(Error::Unauthorized)
    }
}

impl FromStr for LoggedUser {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut buf = StackString::new();
        buf.push_str(s);
        let token: Token = buf.into();
        token.try_into()
    }
}

impl<S> FromRequestParts<S> for LoggedUser
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let cookie_jar = CookieJar::from_request_parts(parts, state)
            .await
            .expect("extract failed");
        debug!("cookie_jar {cookie_jar:?}");
        let user = LoggedUser::extract_user_from_cookies(&cookie_jar)
            .ok_or_else(|| Error::Unauthorized)?;
        Ok(user)
    }
}

/// # Errors
/// Returns error if `get_authorized_users` fails
pub async fn fill_from_db(pool: &PgPool) -> Result<(), Error> {
    if let Ok("true") = var("TESTENV").as_ref().map(String::as_str) {
        AUTHORIZED_USERS.update_users(hashmap! {
            "user@test".into() => ExternalUser {
                email: "user@test".into(),
                session: Uuid::new_v4(),
                secret_key: StackString::default(),
                created_at: OffsetDateTime::now_utc()
            }
        });
        return Ok(());
    }
    let (created_at, deleted_at) = AuthorizedUsers::get_most_recent(pool).await?;
    let most_recent_user_db = created_at.max(deleted_at);
    let existing_users = AUTHORIZED_USERS.get_users();
    let most_recent_user = existing_users.values().map(|i| i.created_at).max();
    debug!("most_recent_user_db {most_recent_user_db:?} most_recent_user {most_recent_user:?}");
    if most_recent_user_db.is_some()
        && most_recent_user.is_some()
        && most_recent_user_db <= most_recent_user
    {
        return Ok(());
    }

    let result: Result<HashMap<StackString, _>, _> = AuthorizedUsers::get_authorized_users(pool)
        .await?
        .map_ok(|u| {
            (
                u.email.clone(),
                ExternalUser {
                    email: u.email,
                    session: Uuid::new_v4(),
                    secret_key: StackString::default(),
                    created_at: u.created_at,
                },
            )
        })
        .try_collect()
        .await;
    let users = result?;
    AUTHORIZED_USERS.update_users(users);
    debug!("AUTHORIZED_USERS {:?}", *AUTHORIZED_USERS);
    Ok(())
}
