use chrono::{DateTime, NaiveDate, Utc};
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl, TextExpressionMethods};
use diesel::{Insertable, Queryable};
use failure::{err_msg, Error};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;

use crate::pgpool::PgPool;
use crate::schema::{authorized_users, diary_cache, diary_entries};

#[derive(Queryable, Insertable, Clone, Debug)]
#[table_name = "diary_entries"]
pub struct DiaryEntries<'a> {
    pub diary_date: NaiveDate,
    pub diary_text: Cow<'a, str>,
    pub last_modified: DateTime<Utc>,
}

#[derive(Queryable, Insertable, Clone, Debug, Serialize, Deserialize)]
#[table_name = "diary_cache"]
pub struct DiaryCache<'a> {
    pub diary_datetime: DateTime<Utc>,
    pub diary_text: Cow<'a, str>,
}

#[derive(Queryable, Insertable, Clone, Debug)]
#[table_name = "authorized_users"]
pub struct AuthorizedUsers {
    pub email: String,
    pub telegram_userid: Option<i64>,
}

impl AuthorizedUsers {
    pub fn get_authorized_users(pool: &PgPool) -> Result<Vec<AuthorizedUsers>, Error> {
        use crate::schema::authorized_users::dsl::authorized_users;
        let conn = pool.get()?;
        authorized_users.load(&conn).map_err(err_msg)
    }
}

impl DiaryEntries<'_> {
    pub fn insert_entry(&self, pool: &PgPool) -> Result<(), Error> {
        use crate::schema::diary_entries::dsl::diary_entries;

        let conn = pool.get()?;
        diesel::insert_into(diary_entries)
            .values(self)
            .execute(&conn)
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn update_entry(&self, pool: &PgPool) -> Result<(), Error> {
        use crate::schema::diary_entries::dsl::{
            diary_date, diary_entries, diary_text, last_modified,
        };
        let conn = pool.get()?;

        diesel::update(diary_entries.filter(diary_date.eq(self.diary_date)))
            .set((
                diary_text.eq(&self.diary_text),
                last_modified.eq(Utc::now()),
            ))
            .execute(&conn)
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn get_modified_map(pool: &PgPool) -> Result<HashMap<NaiveDate, DateTime<Utc>>, Error> {
        use crate::schema::diary_entries::dsl::{diary_date, diary_entries, last_modified};
        let conn = pool.get()?;

        diary_entries
            .select((diary_date, last_modified))
            .load(&conn)
            .map_err(err_msg)
            .map(|v| v.into_iter().collect())
    }

    pub fn get_by_date(date: NaiveDate, pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::diary_entries::dsl::{diary_date, diary_entries};

        let conn = pool.get()?;
        diary_entries
            .filter(diary_date.eq(date))
            .load(&conn)
            .map_err(err_msg)
    }

    pub fn get_by_text(search_text: &str, pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::diary_entries::dsl::{diary_entries, diary_text};
        let conn = pool.get()?;
        diary_entries
            .filter(diary_text.like(&format!("%{}%", search_text)))
            .load(&conn)
            .map_err(err_msg)
    }
}

impl DiaryCache<'_> {
    pub fn insert_entry(&self, pool: &PgPool) -> Result<(), Error> {
        use crate::schema::diary_cache::dsl::diary_cache;
        let conn = pool.get()?;
        diesel::insert_into(diary_cache)
            .values(self)
            .execute(&conn)
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn get_cache_entries(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::diary_cache::dsl::diary_cache;
        let conn = pool.get()?;
        diary_cache.load(&conn).map_err(err_msg)
    }

    pub fn get_by_text(search_text: &str, pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::diary_cache::dsl::{diary_cache, diary_text};
        let conn = pool.get()?;
        diary_cache
            .filter(diary_text.like(&format!("%{}%", search_text)))
            .load(&conn)
            .map_err(err_msg)
    }

    pub fn delete_entry(&self, pool: &PgPool) -> Result<(), Error> {
        use crate::schema::diary_cache::dsl::{diary_cache, diary_datetime};
        let conn = pool.get()?;
        diesel::delete(diary_cache)
            .filter(diary_datetime.eq(self.diary_datetime))
            .execute(&conn)
            .map_err(err_msg)
            .map(|_| ())
    }
}
