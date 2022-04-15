use anyhow::{format_err, Error};
use derive_more::Into;
use difference::{Changeset, Difference};
use log::debug;
use postgres_query::{client::GenericClient, query, query_dyn, FromSqlRow};
use serde::{Deserialize, Serialize};
use stack_string::{format_sstr, StackString};
use std::collections::HashMap;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::{
    date_time_wrapper::DateTimeWrapper,
    pgpool::{PgPool, PgTransaction},
};

#[derive(FromSqlRow, Clone, Debug)]
pub struct DiaryEntries {
    pub diary_date: Date,
    pub diary_text: StackString,
    pub last_modified: DateTimeWrapper,
}

#[derive(FromSqlRow, Clone, Debug, Serialize, Deserialize)]
pub struct DiaryCache {
    pub diary_datetime: DateTimeWrapper,
    pub diary_text: StackString,
}

impl PartialEq for DiaryCache {
    fn eq(&self, other: &Self) -> bool {
        let self_datetime: OffsetDateTime = self.diary_datetime.into();
        let other_datetime: OffsetDateTime = other.diary_datetime.into();
        (self.diary_text == other.diary_text)
            && ((self_datetime - other_datetime).whole_milliseconds() == 0)
    }
}

#[derive(FromSqlRow, Clone, Debug)]
pub struct AuthorizedUsers {
    pub email: StackString,
    pub telegram_userid: Option<i64>,
}

#[derive(FromSqlRow, Clone, Debug, Serialize, Deserialize)]
pub struct DiaryConflict {
    pub id: Uuid,
    pub sync_datetime: DateTimeWrapper,
    pub diary_date: Date,
    pub diff_type: StackString,
    pub diff_text: StackString,
    pub sequence: i32,
}

impl AuthorizedUsers {
    /// # Errors
    /// Return error if db query fails
    pub async fn get_authorized_users(pool: &PgPool) -> Result<Vec<Self>, Error> {
        let query = query!("SELECT * FROM authorized_users");
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }
}

impl DiaryConflict {
    pub fn new(
        sync_datetime: OffsetDateTime,
        diary_date: Date,
        diff_type: impl Into<StackString>,
        diff_text: impl Into<StackString>,
        sequence: i32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            sync_datetime: sync_datetime.into(),
            diary_date,
            diff_type: diff_type.into(),
            diff_text: diff_text.into(),
            sequence,
        }
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_all_dates(pool: &PgPool) -> Result<Vec<Date>, Error> {
        #[derive(FromSqlRow, Into)]
        struct Wrap(Date);

        let query = query!("SELECT distinct diary_date FROM diary_conflict ORDER BY diary_date");
        let conn = pool.get().await?;
        let result: Vec<Wrap> = query.fetch(&conn).await?;
        Ok(result.into_iter().map(Into::into).collect())
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_first_date(pool: &PgPool) -> Result<Option<Date>, Error> {
        #[derive(FromSqlRow, Into)]
        struct Wrap(Date);

        let query =
            query!("SELECT distinct diary_date FROM diary_conflict ORDER BY diary_date LIMIT 1");
        let conn = pool.get().await?;
        let result: Option<Wrap> = query.fetch_opt(&conn).await?;
        Ok(result.map(Into::into))
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_by_date(date: Date, pool: &PgPool) -> Result<Vec<DateTimeWrapper>, Error> {
        #[derive(FromSqlRow, Into)]
        struct Wrap(DateTimeWrapper);

        let query = query!(
            r#"
                SELECT distinct sync_datetime
                FROM diary_conflict
                WHERE diary_date = $date
                ORDER BY sequence
            "#,
            date = date,
        );
        let conn = pool.get().await?;
        let result: Vec<Wrap> = query.fetch(&conn).await?;
        Ok(result.into_iter().map(Into::into).collect())
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_first_by_date(
        date: Date,
        pool: &PgPool,
    ) -> Result<Option<OffsetDateTime>, Error> {
        #[derive(FromSqlRow, Into)]
        struct Wrap(OffsetDateTime);

        let query = query!(
            r#"
                SELECT distinct sync_datetime
                FROM diary_conflict
                WHERE diary_date = $date
                ORDER BY sequence
                LIMIT 1
            "#,
            date = date,
        );
        let conn = pool.get().await?;
        let result: Option<Wrap> = query.fetch_opt(&conn).await?;
        Ok(result.map(Into::into))
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_by_datetime(
        datetime: DateTimeWrapper,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let query = query!(
            r#"
                SELECT * FROM diary_conflict
                WHERE sync_datetime BETWEEN $datetime AND ($datetime + interval '1 second')
                ORDER BY sync_datetime, sequence
            "#,
            datetime = datetime,
        );
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_first_conflict(pool: &PgPool) -> Result<Option<OffsetDateTime>, Error> {
        if let Some(first_date) = Self::get_first_date(pool).await? {
            if let Some(first_conflict) = Self::get_first_by_date(first_date, pool).await? {
                return Ok(Some(first_conflict));
            }
        }
        Ok(None)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn update_by_id(
        id: i32,
        new_diff_type: impl AsRef<str>,
        pool: &PgPool,
    ) -> Result<(), Error> {
        let conn = pool.get().await?;
        Self::update_by_id_conn(id, new_diff_type.as_ref(), &conn).await?;
        Ok(())
    }

    async fn update_by_id_conn<C>(id: i32, new_diff_type: &str, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                UPDATE diary_conflict
                SET diff_type = $new_diff_type
                WHERE id = $id
            "#,
            id = id,
            new_diff_type = new_diff_type,
        );
        query.execute(conn).await?;
        Ok(())
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn remove_by_datetime(datetime: DateTimeWrapper, pool: &PgPool) -> Result<(), Error> {
        let conn = pool.get().await?;
        Self::remove_by_datetime_conn(datetime, &conn).await?;
        Ok(())
    }

    async fn remove_by_datetime_conn<C>(datetime: DateTimeWrapper, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            "DELETE FROM diary_conflict WHERE sync_datetime = $datetime",
            datetime = datetime,
        );
        query.execute(conn).await?;
        Ok(())
    }

    async fn insert_conflict_conn<C>(&self, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                INSERT INTO diary_conflict (
                    id, sync_datetime, diary_date, diff_type, diff_text
                ) VALUES (
                    $id, $sync_datetime, $diary_date, $diff_type, $diff_text
                )
            "#,
            id = self.id,
            sync_datetime = self.sync_datetime,
            diary_date = self.diary_date,
            diff_type = self.diff_type,
            diff_text = self.diff_text,
        );
        query.execute(conn).await?;
        Ok(())
    }

    async fn insert_from_changeset<C>(
        diary_date: Date,
        changeset: Changeset,
        conn: &C,
    ) -> Result<Option<OffsetDateTime>, Error>
    where
        C: GenericClient + Sync,
    {
        let sync_datetime = OffsetDateTime::now_utc();
        let removed_lines: Vec<_> = changeset
            .diffs
            .into_iter()
            .enumerate()
            .map(|(sequence, entry)| match entry {
                Difference::Same(s) => {
                    DiaryConflict::new(sync_datetime, diary_date, "same", s, sequence as i32)
                }
                Difference::Rem(s) => {
                    DiaryConflict::new(sync_datetime, diary_date, "rem", s, sequence as i32)
                }
                Difference::Add(s) => {
                    DiaryConflict::new(sync_datetime, diary_date, "add", s, sequence as i32)
                }
            })
            .collect();

        let n_removed_lines: usize = removed_lines
            .iter()
            .filter(|x| &x.diff_type == "rem")
            .count();

        if n_removed_lines > 0 {
            debug!("update_entry {:?}", removed_lines);
            debug!("difference {}", n_removed_lines);
            for conflict in &removed_lines {
                conflict.insert_conflict_conn(conn).await?;
            }
            Ok(Some(sync_datetime))
        } else {
            Ok(None)
        }
    }
}

impl DiaryEntries {
    pub fn new(diary_date: Date, diary_text: impl Into<StackString>) -> Self {
        Self {
            diary_date,
            diary_text: diary_text.into(),
            last_modified: DateTimeWrapper::now(),
        }
    }

    async fn _insert_entry<C>(&self, conn: &C) -> Result<(), Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                INSERT INTO diary_entries (diary_date, diary_text, last_modified)
                VALUES ($diary_date, $diary_text, now())
            "#,
            diary_date = self.diary_date,
            diary_text = self.diary_text,
        );
        query.execute(conn).await?;
        Ok(())
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn insert_entry(&self, pool: &PgPool) -> Result<(), Error> {
        let conn = pool.get().await?;
        self._insert_entry(&conn).await?;
        Ok(())
    }

    async fn _update_entry<C>(
        &self,
        conn: &C,
        insert_new: bool,
    ) -> Result<Option<OffsetDateTime>, Error>
    where
        C: GenericClient + Sync,
    {
        let changeset = self
            ._get_difference(conn, insert_new)
            .await?
            .ok_or_else(|| format_err!("Not found"))?;

        let conflict_opt = if changeset.distance > 0 {
            DiaryConflict::insert_from_changeset(self.diary_date, changeset, conn).await?
        } else {
            None
        };

        if insert_new {
            let query = query!(
                r#"
                    UPDATE diary_entries
                    SET diary_text=$diary_text,last_modified=now()
                    WHERE diary_date = $diary_date
                "#,
                diary_date = self.diary_date,
                diary_text = self.diary_text,
            );
            query.execute(conn).await?;
            Ok(conflict_opt)
        } else {
            Ok(None)
        }
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn update_entry(
        &self,
        pool: &PgPool,
        insert_new: bool,
    ) -> Result<Option<OffsetDateTime>, Error> {
        let conn = pool.get().await?;
        self._update_entry(&conn, insert_new)
            .await
            .map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn upsert_entry(
        &self,
        pool: &PgPool,
        insert_new: bool,
    ) -> Result<Option<OffsetDateTime>, Error> {
        let mut conn = pool.get().await?;
        let tran = conn.transaction().await?;
        let conn: &PgTransaction = &tran;
        let existing = Self::_get_by_date(self.diary_date, conn).await?;
        let output = if existing.is_some() {
            self._update_entry(conn, insert_new).await?
        } else {
            self._insert_entry(conn).await?;
            None
        };
        tran.commit().await?;
        Ok(output)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_modified_map(pool: &PgPool) -> Result<HashMap<Date, OffsetDateTime>, Error> {
        #[derive(FromSqlRow)]
        struct Wrap {
            diary_date: Date,
            last_modified: OffsetDateTime,
        }

        let query = query!("SELECT diary_date, last_modified FROM diary_entries");
        let conn = pool.get().await?;
        let output: Vec<Wrap> = query.fetch(&conn).await?;
        Ok(output
            .into_iter()
            .map(|x| (x.diary_date, x.last_modified))
            .collect())
    }

    async fn _get_by_date<C>(date: Date, conn: &C) -> Result<Option<Self>, Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            "SELECT * FROM diary_entries WHERE diary_date = $date",
            date = date
        );
        query.fetch_opt(conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_by_date(date: Date, pool: &PgPool) -> Result<Option<Self>, Error> {
        let conn = pool.get().await?;
        Self::_get_by_date(date, &conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_by_text(
        search_text: impl AsRef<str>,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let search_text: StackString = search_text
            .as_ref()
            .chars()
            .filter(|c| char::is_alphanumeric(*c) || *c == '-' || *c == '_')
            .collect();
        let query = format_sstr!(
            r#"
                SELECT * FROM diary_entries
                WHERE diary_text like '%{search_text}%'
                ORDER BY diary_date
            "#
        );
        let query = query_dyn!(&query)?;
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }

    async fn _get_difference<C>(
        &self,
        conn: &C,
        insert_new: bool,
    ) -> Result<Option<Changeset>, Error>
    where
        C: GenericClient + Sync,
    {
        Self::_get_by_date(self.diary_date, conn).await.map(|opt| {
            opt.map(|original| {
                if insert_new {
                    Changeset::new(&original.diary_text, &self.diary_text, "\n")
                } else {
                    Changeset::new(&self.diary_text, &original.diary_text, "\n")
                }
            })
        })
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_difference(&self, pool: &PgPool) -> Result<Option<Changeset>, Error> {
        let conn = pool.get().await?;
        self._get_difference(&conn, true).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn delete_entry(&self, pool: &PgPool) -> Result<(), Error> {
        let query = query!(
            "DELETE FROM diary_entries WHERE diary_date = $diary_date",
            diary_date = self.diary_date
        );
        let conn = pool.get().await?;
        query.execute(&conn).await?;
        Ok(())
    }
}

impl DiaryCache {
    /// # Errors
    /// Return error if db query fails
    pub async fn insert_entry(&self, pool: &PgPool) -> Result<(), Error> {
        let query = query!(
            r#"
                INSERT INTO diary_cache (diary_datetime, diary_text)
                VALUES ($diary_datetime, $diary_text)
            "#,
            diary_datetime = self.diary_datetime,
            diary_text = self.diary_text,
        );
        let conn = pool.get().await?;
        query.execute(&conn).await?;
        Ok(())
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_cache_entries(pool: &PgPool) -> Result<Vec<Self>, Error> {
        let query = query!("SELECT * FROM diary_cache");
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_by_text(
        search_text: impl AsRef<str>,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let search_text: StackString = search_text
            .as_ref()
            .chars()
            .filter(|c| char::is_alphanumeric(*c) || *c == '-' || *c == '_')
            .collect();
        let query = format_sstr!(
            r#"
                SELECT * FROM diary_cache
                WHERE diary_text like '%{search_text}%'
            "#
        );
        let query = query_dyn!(&query)?;
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn delete_entry(&self, pool: &PgPool) -> Result<(), Error> {
        let query = query!(
            "DELETE FROM diary_cache WHERE diary_datetime = $diary_datetime",
            diary_datetime = self.diary_datetime
        );
        let conn = pool.get().await?;
        query.execute(&conn).await?;
        Ok(())
    }
}
