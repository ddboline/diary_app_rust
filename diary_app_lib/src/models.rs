use chrono::{DateTime, NaiveDate, Utc};
use diesel::{
    Connection, ExpressionMethods, Insertable, QueryDsl, Queryable, RunQueryDsl,
    TextExpressionMethods,
};
use difference::Changeset;
use failure::{err_msg, Error};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;

use crate::pgpool::{PgPool, PgPoolConn};
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
    fn _insert_entry(&self, conn: &PgPoolConn) -> Result<(), Error> {
        use crate::schema::diary_entries::dsl::diary_entries;

        diesel::insert_into(diary_entries)
            .values(self)
            .execute(conn)
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn insert_entry(&self, pool: &PgPool) -> Result<(), Error> {
        let conn = pool.get()?;
        self._insert_entry(&conn)
    }

    fn _update_entry(&self, conn: &PgPoolConn) -> Result<(), Error> {
        println!("update_entry {}", self._get_difference(conn)?);
        use crate::schema::diary_entries::dsl::{
            diary_date, diary_entries, diary_text, last_modified,
        };

        diesel::update(diary_entries.filter(diary_date.eq(self.diary_date)))
            .set((
                diary_text.eq(&self.diary_text),
                last_modified.eq(Utc::now()),
            ))
            .execute(conn)
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn update_entry(&self, pool: &PgPool) -> Result<(), Error> {
        let conn = pool.get()?;
        self._update_entry(&conn)
    }

    pub fn upsert_entry(&self, pool: &PgPool) -> Result<(), Error> {
        let conn = pool.get()?;

        conn.transaction(|| match Self::_get_by_date(self.diary_date, &conn) {
            Ok(_) => self._update_entry(&conn),
            Err(_) => self._insert_entry(&conn),
        })
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

    fn _get_by_date(date: NaiveDate, conn: &PgPoolConn) -> Result<Self, Error> {
        use crate::schema::diary_entries::dsl::{diary_date, diary_entries};

        diary_entries
            .filter(diary_date.eq(date))
            .first(conn)
            .map_err(err_msg)
    }

    pub fn get_by_date(date: NaiveDate, pool: &PgPool) -> Result<Self, Error> {
        let conn = pool.get()?;
        Self::_get_by_date(date, &conn)
    }

    pub fn get_by_text(search_text: &str, pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::diary_entries::dsl::{diary_date, diary_entries, diary_text};
        let conn = pool.get()?;
        diary_entries
            .filter(diary_text.like(&format!("%{}%", search_text)))
            .order(diary_date)
            .load(&conn)
            .map_err(err_msg)
    }

    fn _get_difference(&self, conn: &PgPoolConn) -> Result<Changeset, Error> {
        Self::_get_by_date(self.diary_date, conn)
            .map(|original| Changeset::new(&original.diary_text, &self.diary_text, "\n"))
    }

    pub fn get_difference(&self, pool: &PgPool) -> Result<Changeset, Error> {
        let conn = pool.get()?;
        self._get_difference(&conn)
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
