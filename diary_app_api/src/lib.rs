#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::unused_async)]
#![allow(clippy::implicit_hasher)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::similar_names)]

pub mod app;
pub mod elements;
pub mod errors;
pub mod logged_user;
pub mod requests;
pub mod routes;

use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use utoipa::ToSchema;
use utoipa_helper::derive_utoipa_schema;

use diary_app_lib::date_time_wrapper::DateTimeWrapper;

#[derive(Serialize, Deserialize)]
pub struct ConflictData {
    pub date: Option<Date>,
    pub datetime: Option<DateTimeWrapper>,
}

derive_utoipa_schema!(ConflictData, _ConflictData);

#[allow(dead_code)]
#[derive(ToSchema)]
// ConflictData
struct _ConflictData {
    // Conflict Date
    pub date: Option<Date>,
    // Conflict DateTime
    pub datetime: Option<OffsetDateTime>,
}

#[derive(Serialize, Deserialize)]
pub struct CommitConflictData {
    pub datetime: DateTimeWrapper,
}

derive_utoipa_schema!(CommitConflictData, _CommitConflictData);

#[allow(dead_code)]
#[derive(ToSchema)]
struct _CommitConflictData {
    pub datetime: OffsetDateTime,
}

#[cfg(test)]
mod test {
    use utoipa_helper::derive_utoipa_test;

    use crate::{_CommitConflictData, _ConflictData, CommitConflictData, ConflictData};

    #[test]
    fn test_type() {
        derive_utoipa_test!(ConflictData, _ConflictData);
        derive_utoipa_test!(CommitConflictData, _CommitConflictData);
    }
}
