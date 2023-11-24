#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::unused_async)]
#![allow(clippy::implicit_hasher)]
#![allow(clippy::ignored_unit_patterns)]

pub mod app;
pub mod elements;
pub mod errors;
pub mod logged_user;
pub mod requests;
pub mod routes;

use rweb::Schema;
use serde::{Deserialize, Serialize};

use rweb_helper::{derive_rweb_schema, DateTimeType, DateType};

use diary_app_lib::date_time_wrapper::DateTimeWrapper;

#[derive(Serialize, Deserialize)]
pub struct ConflictData {
    pub date: Option<DateType>,
    pub datetime: Option<DateTimeWrapper>,
}

derive_rweb_schema!(ConflictData, _ConflictData);

#[allow(dead_code)]
#[derive(Schema)]
struct _ConflictData {
    #[schema(description = "Conflict Date")]
    pub date: Option<DateType>,
    #[schema(description = "Conflict DateTime")]
    pub datetime: Option<DateTimeType>,
}

#[derive(Serialize, Deserialize)]
pub struct CommitConflictData {
    pub datetime: DateTimeWrapper,
}

derive_rweb_schema!(CommitConflictData, _CommitConflictData);

#[allow(dead_code)]
#[derive(Schema)]
struct _CommitConflictData {
    pub datetime: DateTimeType,
}

#[cfg(test)]
mod test {
    use rweb_helper::derive_rweb_test;

    use crate::{CommitConflictData, ConflictData, _CommitConflictData, _ConflictData};

    #[test]
    fn test_type() {
        derive_rweb_test!(ConflictData, _ConflictData);
        derive_rweb_test!(CommitConflictData, _CommitConflictData);
    }
}
