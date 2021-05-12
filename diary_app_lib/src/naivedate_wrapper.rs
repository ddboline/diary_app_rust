use chrono::{NaiveDate, NaiveTime};
use derive_more::{Deref, From, FromStr, Into, Display};
use rweb::openapi::{Entity, Schema, Type};
use serde::{Deserialize, Serialize};

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Display,
    FromStr,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Clone,
    Copy,
    Deref,
    Into,
    From,
)]
pub struct NaiveDateWrapper(NaiveDate);

impl Entity for NaiveDateWrapper {
    #[inline]
    fn describe() -> Schema {
        Schema {
            schema_type: Some(Type::String),
            format: "naivedate".into(),
            ..Schema::default()
        }
    }
}

#[derive(
    Serialize,
    Deserialize,
    Debug,
    FromStr,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Clone,
    Copy,
    Deref,
    Into,
    From,
)]
pub struct NaiveTimeWrapper(NaiveTime);

impl Entity for NaiveTimeWrapper {
    #[inline]
    fn describe() -> Schema {
        Schema {
            schema_type: Some(Type::String),
            format: "naivetime".into(),
            ..Schema::default()
        }
    }
}
