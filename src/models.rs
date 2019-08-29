use chrono::{DateTime, NaiveDate, Utc};

use crate::schema::{authorized_users, diary_cache, diary_entries};

#[derive(Queryable, Insertable, Clone, Debug)]
#[table_name = "diary_entries"]
pub struct DiaryEntries {
    pub diary_date: NaiveDate,
    pub diary_text: Option<String>,
}

#[derive(Queryable, Insertable, Clone, Debug)]
#[table_name = "diary_cache"]
pub struct DiaryCache {
    pub diary_datetime: DateTime<Utc>,
    pub diary_text: Option<String>,
}

#[derive(Queryable, Insertable, Clone, Debug)]
#[table_name = "authorized_users"]
pub struct AuthorizedUsers {
    pub email: String,
    pub telegram_userid: Option<i64>,
}
