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

use chrono::{DateTime, Utc};
use derive_more::{Deref, From, Into};
use diary_app_lib::models::DiaryCache;
use rweb::Schema;
use serde::{Deserialize, Serialize};
use stack_string::StackString;

use rweb_helper::derive_rweb_schema;

#[derive(Deref, Clone, Debug, Serialize, Deserialize, Into, From)]
pub struct DiaryCacheWrapper(DiaryCache);

derive_rweb_schema!(DiaryCacheWrapper, _DiaryCacheWrapper);

#[allow(dead_code)]
#[derive(Schema)]
struct _DiaryCacheWrapper {
    #[schema(description = "Cache Datetime")]
    diary_datetime: DateTime<Utc>,
    #[schema(description = "Cache Text")]
    diary_text: StackString,
}

#[cfg(test)]
mod test {
    use rweb_helper::derive_rweb_test;

    use crate::{DiaryCacheWrapper, _DiaryCacheWrapper};

    #[test]
    fn test_type() {
        derive_rweb_test!(DiaryCacheWrapper, _DiaryCacheWrapper);
    }
}
