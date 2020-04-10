use anyhow::Error;
use derive_more::{Display, From, Into};
use diesel::{
    backend::Backend,
    deserialize::{FromSql, Result as DeResult},
    serialize::{Output, Result as SerResult, ToSql},
    sql_types::Text,
};
use inlinable_string::InlinableString;
use serde::{Deserialize, Serialize};
use std::{borrow::Borrow, io::Write, str::FromStr};

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Into,
    From,
    Display,
    PartialEq,
    Eq,
    Hash,
    FromSqlRow,
    AsExpression,
    Default,
    PartialOrd,
    Ord,
)]
#[sql_type = "Text"]
#[serde(into = "String", from = "&str")]
pub struct StackString(InlinableString);

impl StackString {
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<StackString> for String {
    fn from(item: StackString) -> Self {
        match item.0 {
            InlinableString::Heap(s) => s,
            InlinableString::Inline(s) => s.to_string(),
        }
    }
}

impl From<String> for StackString {
    fn from(item: String) -> Self {
        Self(item.into())
    }
}

impl From<&String> for StackString {
    fn from(item: &String) -> Self {
        Self(item.as_str().into())
    }
}

impl From<&str> for StackString {
    fn from(item: &str) -> Self {
        Self(item.into())
    }
}

impl Borrow<str> for StackString {
    fn borrow(&self) -> &str {
        self.0.borrow()
    }
}

impl<DB> ToSql<Text, DB> for StackString
where
    DB: Backend,
    str: ToSql<Text, DB>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> SerResult {
        self.as_str().to_sql(out)
    }
}

impl<ST, DB> FromSql<ST, DB> for StackString
where
    DB: Backend,
    *const str: FromSql<ST, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> DeResult<Self> {
        let str_ptr = <*const str as FromSql<ST, DB>>::from_sql(bytes)?;
        let string = unsafe { &*str_ptr };
        Ok(string.into())
    }
}

impl AsRef<str> for StackString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl FromStr for StackString {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(s.into())
    }
}
