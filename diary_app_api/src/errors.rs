use anyhow::Error as AnyhowError;
use axum::{
    extract::Json,
    http::{
        StatusCode,
        header::{CONTENT_TYPE, InvalidHeaderName},
    },
    response::{IntoResponse, Response},
};
use handlebars::RenderError;
use log::error;
use notify::Error as NotifyError;
use postgres_query::Error as PqError;
use serde::Serialize;
use serde_json::Error as SerdeJsonError;
use serde_yml::Error as SerdeYamlError;
use stack_string::{StackString, format_sstr};
use std::{
    fmt::{Debug, Error as FmtError},
    net::AddrParseError,
};
use thiserror::Error;
use utoipa::{
    IntoResponses, PartialSchema, ToSchema,
    openapi::{
        content::ContentBuilder,
        response::{ResponseBuilder, ResponsesBuilder},
    },
};

use authorized_users::errors::AuthUsersError;

use crate::logged_user::LOGIN_HTML;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("io Error {0}")]
    IoError(#[from] std::io::Error),
    #[error("InvalidHeaderName {0}")]
    InvalidHeaderName(#[from] InvalidHeaderName),
    #[error("AuthUsersError {0}")]
    AuthUsersError(#[from] AuthUsersError),
    #[error("NotifyError {0}")]
    NotifyError(Box<NotifyError>),
    #[error("AddrParseError {0}")]
    AddrParseError(#[from] AddrParseError),
    #[error("SerdeYamlError {0}")]
    SerdeYamlError(#[from] SerdeYamlError),
    #[error("SerdeJsonError {0}")]
    SerdeJsonError(#[from] SerdeJsonError),
    #[error("Internal Server Error")]
    InternalServerError,
    #[error("BadRequest: {0}")]
    BadRequest(StackString),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Anyhow error {0}")]
    AnyhowError(#[from] AnyhowError),
    #[error("Handlebars RenderError {0}")]
    RenderError(Box<RenderError>),
    #[error("PqError {0}")]
    PqError(Box<PqError>),
    #[error("FmtError {0}")]
    FmtError(#[from] FmtError),
}

impl From<PqError> for ServiceError {
    fn from(value: PqError) -> Self {
        Self::PqError(value.into())
    }
}

impl From<NotifyError> for ServiceError {
    fn from(value: NotifyError) -> Self {
        Self::NotifyError(value.into())
    }
}

impl From<RenderError> for ServiceError {
    fn from(value: RenderError) -> Self {
        Self::RenderError(value.into())
    }
}
#[derive(Serialize, ToSchema)]
struct ErrorMessage {
    #[schema(inline)]
    message: StackString,
}

impl IntoResponse for ErrorMessage {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthorized => (
                StatusCode::OK,
                [(CONTENT_TYPE, mime::TEXT_HTML.essence_str())],
                LOGIN_HTML,
            )
                .into_response(),
            Self::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
                [(CONTENT_TYPE, mime::APPLICATION_JSON.essence_str())],
                ErrorMessage { message },
            )
                .into_response(),
            e => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorMessage {
                    message: format_sstr!("Internal Server Error: {e}"),
                },
            )
                .into_response(),
        }
    }
}

impl IntoResponses for ServiceError {
    fn responses() -> std::collections::BTreeMap<
        String,
        utoipa::openapi::RefOr<utoipa::openapi::response::Response>,
    > {
        let error_message_content = ContentBuilder::new()
            .schema(Some(ErrorMessage::schema()))
            .build();
        ResponsesBuilder::new()
            .response(
                StatusCode::UNAUTHORIZED.as_str(),
                ResponseBuilder::new()
                    .description("Not Authorized")
                    .content(
                        mime::TEXT_HTML.essence_str(),
                        ContentBuilder::new().schema(Some(String::schema())).build(),
                    ),
            )
            .response(
                StatusCode::BAD_REQUEST.as_str(),
                ResponseBuilder::new().description("Bad Request").content(
                    mime::APPLICATION_JSON.essence_str(),
                    error_message_content.clone(),
                ),
            )
            .response(
                StatusCode::INTERNAL_SERVER_ERROR.as_str(),
                ResponseBuilder::new()
                    .description("Internal Server Error")
                    .content(
                        mime::APPLICATION_JSON.essence_str(),
                        error_message_content.clone(),
                    ),
            )
            .build()
            .into()
    }
}

#[cfg(test)]
mod test {
    use anyhow::Error as AnyhowError;
    use axum::http::header::InvalidHeaderName;
    use handlebars::RenderError;
    use notify::Error as NotifyError;
    use postgres_query::Error as PqError;
    use serde_json::Error as SerdeJsonError;
    use serde_yml::Error as SerdeYamlError;
    use stack_string::StackString;
    use std::{fmt::Error as FmtError, net::AddrParseError};
    use time_tz::system::Error as TzError;
    use tokio::task::JoinError;

    use authorized_users::errors::AuthUsersError;

    use crate::errors::ServiceError as Error;

    #[test]
    fn test_error_size() {
        println!("JoinError {}", std::mem::size_of::<JoinError>());
        println!("BadRequest: {}", std::mem::size_of::<StackString>());
        println!("Anyhow error {}", std::mem::size_of::<AnyhowError>());
        println!("io Error {}", std::mem::size_of::<std::io::Error>());
        println!("tokio join error {}", std::mem::size_of::<JoinError>());
        println!("TzError {}", std::mem::size_of::<TzError>());
        println!("PqError {}", std::mem::size_of::<PqError>());
        println!("FmtError {}", std::mem::size_of::<FmtError>());
        println!("io Error {}", std::mem::size_of::<std::io::Error>());
        println!(
            "InvalidHeaderName {}",
            std::mem::size_of::<InvalidHeaderName>()
        );
        println!("AuthUsersError {}", std::mem::size_of::<AuthUsersError>());
        println!("NotifyError {}", std::mem::size_of::<NotifyError>());
        println!("AddrParseError {}", std::mem::size_of::<AddrParseError>());
        println!("SerdeYamlError {}", std::mem::size_of::<SerdeYamlError>());
        println!("SerdeJsonError {}", std::mem::size_of::<SerdeJsonError>());
        println!(
            "Handlebars RenderError {}",
            std::mem::size_of::<RenderError>()
        );
        println!("PqError {}", std::mem::size_of::<PqError>());
        println!("FmtError  {}", std::mem::size_of::<FmtError>());

        assert_eq!(std::mem::size_of::<Error>(), 24);
    }
}
