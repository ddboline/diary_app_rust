use anyhow::Error;
use chrono::{DateTime, NaiveDate, Utc};
use diesel::{
    pg::PgConnection, result::Error as DieselError, ExpressionMethods, Insertable, QueryDsl,
    Queryable, RunQueryDsl, TextExpressionMethods,
};
use difference::{Changeset, Difference};
use log::debug;
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::collections::HashMap;
use tokio_diesel::{AsyncConnection, AsyncRunQueryDsl, OptionalExtension};

use crate::{
    pgpool::PgPool,
    schema::{authorized_users, diary_cache, diary_conflict, diary_entries},
};

#[derive(Queryable, Insertable, Clone, Debug)]
#[table_name = "diary_entries"]
pub struct DiaryEntries {
    pub diary_date: NaiveDate,
    pub diary_text: StackString,
    pub last_modified: DateTime<Utc>,
}

#[derive(Queryable, Insertable, Clone, Debug, Serialize, Deserialize)]
#[table_name = "diary_cache"]
pub struct DiaryCache {
    pub diary_datetime: DateTime<Utc>,
    pub diary_text: StackString,
}

impl PartialEq for DiaryCache {
    fn eq(&self, other: &Self) -> bool {
        (self.diary_text == other.diary_text)
            && ((self.diary_datetime - other.diary_datetime).num_milliseconds() == 0)
    }
}

#[derive(Queryable, Insertable, Clone, Debug)]
#[table_name = "authorized_users"]
pub struct AuthorizedUsers {
    pub email: StackString,
    pub telegram_userid: Option<i64>,
}

#[derive(Queryable, Clone, Debug, Serialize, Deserialize)]
pub struct DiaryConflict {
    pub id: i32,
    pub sync_datetime: DateTime<Utc>,
    pub diary_date: NaiveDate,
    pub diff_type: StackString,
    pub diff_text: StackString,
}

#[derive(Insertable, Clone, Debug, Serialize, Deserialize)]
#[table_name = "diary_conflict"]
pub struct DiaryConflictInsert {
    pub sync_datetime: DateTime<Utc>,
    pub diary_date: NaiveDate,
    pub diff_type: StackString,
    pub diff_text: StackString,
}

impl From<DiaryConflict> for DiaryConflictInsert {
    fn from(item: DiaryConflict) -> DiaryConflictInsert {
        Self {
            sync_datetime: item.sync_datetime,
            diary_date: item.diary_date,
            diff_type: item.diff_type,
            diff_text: item.diff_text,
        }
    }
}

impl AuthorizedUsers {
    pub async fn get_authorized_users(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::authorized_users::dsl::authorized_users;
        authorized_users.load_async(pool).await.map_err(Into::into)
    }
}

impl DiaryConflict {
    pub async fn get_all_dates(pool: &PgPool) -> Result<Vec<NaiveDate>, Error> {
        use crate::schema::diary_conflict::dsl::{diary_conflict, diary_date};
        diary_conflict
            .select(diary_date)
            .distinct()
            .order(diary_date)
            .load_async(pool)
            .await
            .map_err(Into::into)
    }

    pub async fn get_first_date(pool: &PgPool) -> Result<Option<NaiveDate>, Error> {
        use crate::schema::diary_conflict::dsl::{diary_conflict, diary_date};
        diary_conflict
            .select(diary_date)
            .distinct()
            .order(diary_date)
            .first_async(pool)
            .await
            .optional()
            .map_err(Into::into)
    }

    pub async fn get_by_date(date: NaiveDate, pool: &PgPool) -> Result<Vec<DateTime<Utc>>, Error> {
        use crate::schema::diary_conflict::dsl::{diary_conflict, diary_date, sync_datetime};
        diary_conflict
            .filter(diary_date.eq(date))
            .select(sync_datetime)
            .distinct()
            .order(sync_datetime)
            .load_async(pool)
            .await
            .map_err(Into::into)
    }

    pub async fn get_first_by_date(
        date: NaiveDate,
        pool: &PgPool,
    ) -> Result<Option<DateTime<Utc>>, Error> {
        use crate::schema::diary_conflict::dsl::{diary_conflict, diary_date, sync_datetime};
        diary_conflict
            .filter(diary_date.eq(date))
            .select(sync_datetime)
            .distinct()
            .order(sync_datetime)
            .first_async(pool)
            .await
            .optional()
            .map_err(Into::into)
    }

    pub async fn get_by_datetime(
        datetime: DateTime<Utc>,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        use crate::schema::diary_conflict::dsl::{diary_conflict, id, sync_datetime};
        diary_conflict
            .filter(sync_datetime.eq(datetime))
            .order(id)
            .load_async(pool)
            .await
            .map_err(Into::into)
    }

    pub async fn get_first_conflict(pool: &PgPool) -> Result<Option<DateTime<Utc>>, Error> {
        if let Some(first_date) = Self::get_first_date(pool).await? {
            if let Some(first_conflict) = Self::get_first_by_date(first_date, pool).await? {
                return Ok(Some(first_conflict));
            }
        }
        Ok(None)
    }

    pub async fn update_by_id(id_: i32, new_diff_type: &str, pool: &PgPool) -> Result<(), Error> {
        use crate::schema::diary_conflict::dsl::{diary_conflict, diff_type, id};
        diesel::update(diary_conflict.filter(id.eq(id_)))
            .set(diff_type.eq(new_diff_type))
            .execute_async(pool)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn remove_by_datetime(datetime: DateTime<Utc>, pool: &PgPool) -> Result<(), Error> {
        use crate::schema::diary_conflict::dsl::{diary_conflict, sync_datetime};
        diesel::delete(diary_conflict.filter(sync_datetime.eq(datetime)))
            .execute_async(pool)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    fn insert_from_changeset(
        diary_date: NaiveDate,
        changeset: Changeset,
        conn: &PgConnection,
    ) -> Result<Option<DateTime<Utc>>, DieselError> {
        use crate::schema::diary_conflict::dsl::diary_conflict;

        let sync_datetime = Utc::now();
        let removed_lines: Vec<_> = changeset
            .diffs
            .into_iter()
            .map(|entry| match entry {
                Difference::Same(s) => DiaryConflictInsert {
                    sync_datetime,
                    diary_date,
                    diff_type: "same".into(),
                    diff_text: s.into(),
                },
                Difference::Rem(s) => DiaryConflictInsert {
                    sync_datetime,
                    diary_date,
                    diff_type: "rem".into(),
                    diff_text: s.into(),
                },
                Difference::Add(s) => DiaryConflictInsert {
                    sync_datetime,
                    diary_date,
                    diff_type: "add".into(),
                    diff_text: s.into(),
                },
            })
            .collect();

        let n_removed_lines: usize = removed_lines
            .iter()
            .filter(|x| &x.diff_type == "rem")
            .count();

        if n_removed_lines > 0 {
            debug!("update_entry {:?}", removed_lines);
            debug!("difference {}", n_removed_lines);
            diesel::insert_into(diary_conflict)
                .values(&removed_lines)
                .execute(conn)
                .map(|_| Some(sync_datetime))
                .map_err(Into::into)
        } else {
            Ok(None)
        }
    }
}

impl DiaryEntries {
    pub fn new(diary_date: NaiveDate, diary_text: &str) -> Self {
        Self {
            diary_date,
            diary_text: diary_text.into(),
            last_modified: Utc::now(),
        }
    }

    fn _insert_entry(&self, conn: &PgConnection) -> Result<Option<DateTime<Utc>>, DieselError> {
        use crate::schema::diary_entries::dsl::diary_entries;

        diesel::insert_into(diary_entries)
            .values(self)
            .execute(conn)
            .map(|_| None)
    }

    pub async fn insert_entry(self, pool: &PgPool) -> Result<(Self, Option<DateTime<Utc>>), Error> {
        pool.run(move |conn| self._insert_entry(conn).map(|x| (self, x)))
            .await
            .map_err(Into::into)
    }

    fn _update_entry(
        &self,
        conn: &PgConnection,
        insert_new: bool,
    ) -> Result<Option<DateTime<Utc>>, DieselError> {
        use crate::schema::diary_entries::dsl::{
            diary_date, diary_entries, diary_text, last_modified,
        };

        let changeset = self
            ._get_difference(conn, insert_new)?
            .ok_or_else(|| DieselError::NotFound)?;

        let conflict_opt = if changeset.distance > 0 {
            DiaryConflict::insert_from_changeset(self.diary_date, changeset, conn)?
        } else {
            None
        };

        if insert_new {
            diesel::update(diary_entries.filter(diary_date.eq(self.diary_date)))
                .set((
                    diary_text.eq(&self.diary_text),
                    last_modified.eq(Utc::now()),
                ))
                .execute(conn)
                .map(|_| conflict_opt)
                .map_err(Into::into)
        } else {
            Ok(None)
        }
    }

    pub async fn update_entry(
        self,
        pool: &PgPool,
        insert_new: bool,
    ) -> Result<(Self, Option<DateTime<Utc>>), Error> {
        pool.run(move |conn| self._update_entry(conn, insert_new).map(|x| (self, x)))
            .await
            .map_err(Into::into)
    }

    pub async fn upsert_entry(
        self,
        pool: &PgPool,
        insert_new: bool,
    ) -> Result<(Self, Option<DateTime<Utc>>), Error> {
        pool.transaction(move |conn| {
            match Self::_get_by_date(self.diary_date, conn)? {
                Some(_) => self._update_entry(conn, insert_new),
                None => self._insert_entry(conn),
            }
            .map(|x| (self, x))
        })
        .await
        .map_err(Into::into)
    }

    pub async fn get_modified_map(
        pool: &PgPool,
    ) -> Result<HashMap<NaiveDate, DateTime<Utc>>, Error> {
        use crate::schema::diary_entries::dsl::{diary_date, diary_entries, last_modified};
        diary_entries
            .select((diary_date, last_modified))
            .load_async(pool)
            .await
            .map(|v| v.into_iter().collect())
            .map_err(Into::into)
    }

    fn _get_by_date(date: NaiveDate, conn: &PgConnection) -> Result<Option<Self>, DieselError> {
        use crate::schema::diary_entries::dsl::{diary_date, diary_entries};
        use diesel::OptionalExtension;
        diary_entries
            .filter(diary_date.eq(date))
            .first(conn)
            .optional()
    }

    pub async fn get_by_date(date: NaiveDate, pool: &PgPool) -> Result<Option<Self>, Error> {
        pool.run(|conn| Self::_get_by_date(date, conn))
            .await
            .map_err(Into::into)
    }

    pub async fn get_by_text(search_text: &str, pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::diary_entries::dsl::{diary_date, diary_entries, diary_text};
        diary_entries
            .filter(diary_text.like(&format!("%{}%", search_text)))
            .order(diary_date)
            .load_async(pool)
            .await
            .map_err(Into::into)
    }

    fn _get_difference(
        &self,
        conn: &PgConnection,
        insert_new: bool,
    ) -> Result<Option<Changeset>, DieselError> {
        Self::_get_by_date(self.diary_date, conn).map(|opt| {
            opt.map(|original| {
                if insert_new {
                    Changeset::new(&original.diary_text, &self.diary_text, "\n")
                } else {
                    Changeset::new(&self.diary_text, &original.diary_text, "\n")
                }
            })
        })
    }

    pub async fn get_difference(self, pool: &PgPool) -> Result<(Self, Option<Changeset>), Error> {
        pool.run(move |conn| self._get_difference(conn, true).map(|x| (self, x)))
            .await
            .map_err(Into::into)
    }

    pub async fn delete_entry(self, pool: &PgPool) -> Result<Self, Error> {
        use crate::schema::diary_entries::dsl::{diary_date, diary_entries};
        diesel::delete(diary_entries)
            .filter(diary_date.eq(self.diary_date))
            .execute_async(pool)
            .await
            .map(|_| self)
            .map_err(Into::into)
    }
}

impl DiaryCache {
    pub async fn insert_entry(self, pool: &PgPool) -> Result<Self, Error> {
        use crate::schema::diary_cache::dsl::diary_cache;
        diesel::insert_into(diary_cache)
            .values(&self)
            .execute_async(pool)
            .await
            .map(|_| self)
            .map_err(Into::into)
    }

    pub async fn get_cache_entries(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::diary_cache::dsl::diary_cache;
        diary_cache.load_async(pool).await.map_err(Into::into)
    }

    pub async fn get_by_text(search_text: &str, pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::diary_cache::dsl::{diary_cache, diary_text};
        diary_cache
            .filter(diary_text.like(&format!("%{}%", search_text)))
            .load_async(pool)
            .await
            .map_err(Into::into)
    }

    pub async fn delete_entry(self, pool: &PgPool) -> Result<Self, Error> {
        use crate::schema::diary_cache::dsl::{diary_cache, diary_datetime};
        diesel::delete(diary_cache)
            .filter(diary_datetime.eq(self.diary_datetime))
            .execute_async(pool)
            .await
            .map(|_| self)
            .map_err(Into::into)
    }
}
