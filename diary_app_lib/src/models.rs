use anyhow::Error;
use chrono::{DateTime, NaiveDate, Utc};
use diesel::{
    Connection, ExpressionMethods, Insertable, QueryDsl, Queryable, RunQueryDsl,
    TextExpressionMethods,
};
use difference::{Changeset, Difference};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{stdout, Write};

use crate::pgpool::{PgPool, PgPoolConn};
use crate::schema::{authorized_users, diary_cache, diary_conflict, diary_entries};

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

impl PartialEq for DiaryCache<'_> {
    fn eq(&self, other: &Self) -> bool {
        (self.diary_text == other.diary_text)
            && ((self.diary_datetime - other.diary_datetime).num_milliseconds() == 0)
    }
}

#[derive(Queryable, Insertable, Clone, Debug)]
#[table_name = "authorized_users"]
pub struct AuthorizedUsers {
    pub email: String,
    pub telegram_userid: Option<i64>,
}

#[derive(Queryable, Clone, Debug, Serialize, Deserialize)]
pub struct DiaryConflict<'a> {
    pub id: i32,
    pub sync_datetime: DateTime<Utc>,
    pub diary_date: NaiveDate,
    pub diff_type: Cow<'a, str>,
    pub diff_text: Cow<'a, str>,
}

#[derive(Insertable, Clone, Debug, Serialize, Deserialize)]
#[table_name = "diary_conflict"]
pub struct DiaryConflictInsert<'a> {
    pub sync_datetime: DateTime<Utc>,
    pub diary_date: NaiveDate,
    pub diff_type: Cow<'a, str>,
    pub diff_text: Cow<'a, str>,
}

impl<'a> From<DiaryConflict<'a>> for DiaryConflictInsert<'a> {
    fn from(item: DiaryConflict<'a>) -> DiaryConflictInsert<'a> {
        Self {
            sync_datetime: item.sync_datetime,
            diary_date: item.diary_date,
            diff_type: item.diff_type,
            diff_text: item.diff_text,
        }
    }
}

impl AuthorizedUsers {
    pub fn get_authorized_users(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::authorized_users::dsl::authorized_users;
        let conn = pool.get()?;
        authorized_users.load(&conn).map_err(Into::into)
    }
}

impl DiaryConflict<'_> {
    pub fn get_all_dates(pool: &PgPool) -> Result<Vec<NaiveDate>, Error> {
        use crate::schema::diary_conflict::dsl::{diary_conflict, diary_date};
        let conn = pool.get()?;
        diary_conflict
            .select(diary_date)
            .distinct()
            .order(diary_date)
            .load(&conn)
            .map_err(Into::into)
    }

    pub fn get_by_date(date: NaiveDate, pool: &PgPool) -> Result<Vec<DateTime<Utc>>, Error> {
        use crate::schema::diary_conflict::dsl::{diary_conflict, diary_date, sync_datetime};
        let conn = pool.get()?;

        diary_conflict
            .filter(diary_date.eq(date))
            .select(sync_datetime)
            .distinct()
            .order(sync_datetime)
            .load(&conn)
            .map_err(Into::into)
    }

    pub fn get_by_datetime(datetime: DateTime<Utc>, pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::diary_conflict::dsl::{diary_conflict, id, sync_datetime};
        let conn = pool.get()?;
        diary_conflict
            .filter(sync_datetime.eq(datetime))
            .order(id)
            .load(&conn)
            .map_err(Into::into)
    }

    pub fn get_first_conflict(pool: &PgPool) -> Result<Option<DateTime<Utc>>, Error> {
        let dates = Self::get_all_dates(pool)?;
        if !dates.is_empty() {
            let conflicts = Self::get_by_date(dates[0], pool)?;
            if !conflicts.is_empty() {
                return Ok(Some(conflicts[0]));
            }
        }
        Ok(None)
    }

    pub fn update_by_id(id_: i32, new_diff_type: &str, pool: &PgPool) -> Result<(), Error> {
        use crate::schema::diary_conflict::dsl::{diary_conflict, diff_type, id};
        let conn = pool.get()?;
        diesel::update(diary_conflict.filter(id.eq(id_)))
            .set(diff_type.eq(new_diff_type))
            .execute(&conn)
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn remove_by_datetime(datetime: DateTime<Utc>, pool: &PgPool) -> Result<(), Error> {
        use crate::schema::diary_conflict::dsl::{diary_conflict, sync_datetime};
        let conn = pool.get()?;
        diesel::delete(diary_conflict.filter(sync_datetime.eq(datetime)))
            .execute(&conn)
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn insert_from_changeset(
        diary_date: NaiveDate,
        changeset: Changeset,
        conn: &PgPoolConn,
    ) -> Result<Option<DateTime<Utc>>, Error> {
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
            .filter(|x| x.diff_type == "rem")
            .count();

        if n_removed_lines > 0 {
            writeln!(stdout(), "update_entry {:?}", removed_lines)?;
            writeln!(stdout(), "difference {}", n_removed_lines)?;
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

impl<'a> DiaryEntries<'a> {
    pub fn new(diary_date: NaiveDate, diary_text: Cow<'a, str>) -> Self {
        Self {
            diary_date,
            diary_text,
            last_modified: Utc::now(),
        }
    }

    fn _insert_entry(&self, conn: &PgPoolConn) -> Result<Option<DateTime<Utc>>, Error> {
        use crate::schema::diary_entries::dsl::diary_entries;

        diesel::insert_into(diary_entries)
            .values(self)
            .execute(conn)
            .map(|_| None)
            .map_err(Into::into)
    }

    pub fn insert_entry(&self, pool: &PgPool) -> Result<Option<DateTime<Utc>>, Error> {
        let conn = pool.get()?;
        self._insert_entry(&conn)
    }

    fn _update_entry(&self, conn: &PgPoolConn) -> Result<Option<DateTime<Utc>>, Error> {
        let changeset = self._get_difference(conn)?;

        let conflict_opt = if changeset.distance > 0 {
            DiaryConflict::insert_from_changeset(self.diary_date, changeset, conn)?
        } else {
            None
        };

        use crate::schema::diary_entries::dsl::{
            diary_date, diary_entries, diary_text, last_modified,
        };

        diesel::update(diary_entries.filter(diary_date.eq(self.diary_date)))
            .set((
                diary_text.eq(&self.diary_text),
                last_modified.eq(Utc::now()),
            ))
            .execute(conn)
            .map(|_| conflict_opt)
            .map_err(Into::into)
    }

    pub fn update_entry(&self, pool: &PgPool) -> Result<Option<DateTime<Utc>>, Error> {
        let conn = pool.get()?;
        self._update_entry(&conn)
    }

    pub fn upsert_entry(&self, pool: &PgPool) -> Result<Option<DateTime<Utc>>, Error> {
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
            .map(|v| v.into_iter().collect())
            .map_err(Into::into)
    }

    fn _get_by_date(date: NaiveDate, conn: &PgPoolConn) -> Result<Self, Error> {
        use crate::schema::diary_entries::dsl::{diary_date, diary_entries};

        diary_entries
            .filter(diary_date.eq(date))
            .first(conn)
            .map_err(Into::into)
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
            .map_err(Into::into)
    }

    fn _get_difference(&self, conn: &PgPoolConn) -> Result<Changeset, Error> {
        Self::_get_by_date(self.diary_date, conn)
            .map(|original| Changeset::new(&original.diary_text, &self.diary_text, "\n"))
    }

    pub fn get_difference(&self, pool: &PgPool) -> Result<Changeset, Error> {
        let conn = pool.get()?;
        self._get_difference(&conn)
    }

    pub fn delete_entry(&self, pool: &PgPool) -> Result<(), Error> {
        use crate::schema::diary_entries::dsl::{diary_date, diary_entries};
        let conn = pool.get()?;
        diesel::delete(diary_entries)
            .filter(diary_date.eq(self.diary_date))
            .execute(&conn)
            .map(|_| ())
            .map_err(Into::into)
    }
}

impl DiaryCache<'_> {
    pub fn insert_entry(&self, pool: &PgPool) -> Result<(), Error> {
        use crate::schema::diary_cache::dsl::diary_cache;
        let conn = pool.get()?;
        diesel::insert_into(diary_cache)
            .values(self)
            .execute(&conn)
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn get_cache_entries(pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::diary_cache::dsl::diary_cache;
        let conn = pool.get()?;
        diary_cache.load(&conn).map_err(Into::into)
    }

    pub fn get_by_text(search_text: &str, pool: &PgPool) -> Result<Vec<Self>, Error> {
        use crate::schema::diary_cache::dsl::{diary_cache, diary_text};
        let conn = pool.get()?;
        diary_cache
            .filter(diary_text.like(&format!("%{}%", search_text)))
            .load(&conn)
            .map_err(Into::into)
    }

    pub fn delete_entry(&self, pool: &PgPool) -> Result<(), Error> {
        use crate::schema::diary_cache::dsl::{diary_cache, diary_datetime};
        let conn = pool.get()?;
        diesel::delete(diary_cache)
            .filter(diary_datetime.eq(self.diary_datetime))
            .execute(&conn)
            .map(|_| ())
            .map_err(Into::into)
    }
}
