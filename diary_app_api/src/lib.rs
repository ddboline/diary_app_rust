#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::unused_async)]

pub mod app;
pub mod errors;
pub mod logged_user;
pub mod requests;
pub mod routes;

use derive_more::{Deref, From, Into};
use diary_app_lib::models::DiaryCache;
use rweb::Schema;
use serde::{Deserialize, Serialize};
use stack_string::StackString;

use rweb_helper::{derive_rweb_schema, DateTimeType, DateType};

use diary_app_lib::date_time_wrapper::DateTimeWrapper;

#[derive(Deref, Clone, Debug, Serialize, Deserialize, Into, From)]
pub struct DiaryCacheWrapper(DiaryCache);

derive_rweb_schema!(DiaryCacheWrapper, _DiaryCacheWrapper);

#[allow(dead_code)]
#[derive(Schema)]
struct _DiaryCacheWrapper {
    #[schema(description = "Cache Datetime")]
    diary_datetime: DateTimeType,
    #[schema(description = "Cache Text")]
    diary_text: StackString,
}

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

    use crate::{
        CommitConflictData, ConflictData, DiaryCacheWrapper, _CommitConflictData, _ConflictData,
        _DiaryCacheWrapper,
    };

    #[test]
    fn test_type() {
        derive_rweb_test!(DiaryCacheWrapper, _DiaryCacheWrapper);
        derive_rweb_test!(ConflictData, _ConflictData);
        derive_rweb_test!(CommitConflictData, _CommitConflictData);
    }
}
