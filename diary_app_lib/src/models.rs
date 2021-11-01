use anyhow::{format_err, Error};
use chrono::{DateTime, NaiveDate, Utc};
use derive_more::Into;
use difference::{Changeset, Difference};
use log::debug;
use postgres_query::{client::GenericClient, query, query_dyn, FromSqlRow};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::collections::HashMap;
use uuid::Uuid;

use crate::pgpool::{PgPool, PgTransaction};

#[derive(FromSqlRow, Clone, Debug)]
pub struct DiaryEntries {
    pub diary_date: NaiveDate,
    pub diary_text: StackString,
    pub last_modified: DateTime<Utc>,
}

#[derive(FromSqlRow, Clone, Debug, Serialize, Deserialize)]
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

#[derive(FromSqlRow, Clone, Debug)]
pub struct AuthorizedUsers {
    pub email: StackString,
    pub telegram_userid: Option<i64>,
}

#[derive(FromSqlRow, Clone, Debug, Serialize, Deserialize)]
pub struct DiaryConflict {
    pub id: Uuid,
    pub sync_datetime: DateTime<Utc>,
    pub diary_date: NaiveDate,
    pub diff_type: StackString,
    pub diff_text: StackString,
}

impl AuthorizedUsers {
    pub async fn get_authorized_users(pool: &PgPool) -> Result<Vec<Self>, Error> {
        let query = query!("SELECT * FROM authorized_users");
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }
}

impl DiaryConflict {
    pub fn new(
        sync_datetime: DateTime<Utc>,
        diary_date: NaiveDate,
        diff_type: StackString,
        diff_text: StackString,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            sync_datetime,
            diary_date,
            diff_type,
            diff_text,
        }
    }

    pub async fn get_all_dates(pool: &PgPool) -> Result<Vec<NaiveDate>, Error> {
        #[derive(FromSqlRow, Into)]
        struct Wrap(NaiveDate);

        let query = query!("SELECT distinct diary_date FROM diary_conflict ORDER BY dieary_date");
        let conn = pool.get().await?;
        let result: Vec<Wrap> = query.fetch(&conn).await?;
        Ok(result.into_iter().map(Into::into).collect())
    }

    pub async fn get_first_date(pool: &PgPool) -> Result<Option<NaiveDate>, Error> {
        #[derive(FromSqlRow, Into)]
        struct Wrap(NaiveDate);

        let query =
            query!("SELECT distinct diary_date FROM diary_conflict ORDER BY diary_date LIMIT 1");
        let conn = pool.get().await?;
        let result: Option<Wrap> = query.fetch_opt(&conn).await?;
        Ok(result.map(Into::into))
    }

    pub async fn get_by_date(date: NaiveDate, pool: &PgPool) -> Result<Vec<DateTime<Utc>>, Error> {
        #[derive(FromSqlRow, Into)]
        struct Wrap(DateTime<Utc>);

        let query = query!(
            r#"
                SELECT distinct sync_datetime
                FROM diary_conflict
                WHERE date_date = $date
                ORDER BY sync_datetime
            "#,
            date = date,
        );
        let conn = pool.get().await?;
        let result: Vec<Wrap> = query.fetch(&conn).await?;
        Ok(result.into_iter().map(Into::into).collect())
    }

    pub async fn get_first_by_date(
        date: NaiveDate,
        pool: &PgPool,
    ) -> Result<Option<DateTime<Utc>>, Error> {
        #[derive(FromSqlRow, Into)]
        struct Wrap(DateTime<Utc>);

        let query = query!(
            r#"
                SELECT distinct sync_datetime
                FROM diary_conflict
                WHERE date_date = $date
                ORDER BY sync_datetime
                LIMIT 1
            "#,
            date = date,
        );
        let conn = pool.get().await?;
        let result: Option<Wrap> = query.fetch_opt(&conn).await?;
        Ok(result.map(Into::into))
    }

    pub async fn get_by_datetime(
        datetime: DateTime<Utc>,
        pool: &PgPool,
    ) -> Result<Vec<Self>, Error> {
        let query = query!(
            "SELECT * FROM diary_conflict WHERE sync_datetime = $datetime ORDER BY id",
            datetime = datetime,
        );
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }

    pub async fn get_first_conflict(pool: &PgPool) -> Result<Option<DateTime<Utc>>, Error> {
        if let Some(first_date) = Self::get_first_date(pool).await? {
            if let Some(first_conflict) = Self::get_first_by_date(first_date, pool).await? {
                return Ok(Some(first_conflict));
            }
        }
        Ok(None)
    }

    pub async fn update_by_id(id: i32, new_diff_type: &str, pool: &PgPool) -> Result<(), Error> {
        let conn = pool.get().await?;
        Self::update_by_id_conn(id, new_diff_type, &conn).await?;
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

    pub async fn remove_by_datetime(datetime: DateTime<Utc>, pool: &PgPool) -> Result<(), Error> {
        let conn = pool.get().await?;
        Self::remove_by_datetime_conn(datetime, &conn).await?;
        Ok(())
    }

    async fn remove_by_datetime_conn<C>(datetime: DateTime<Utc>, conn: &C) -> Result<(), Error>
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
        diary_date: NaiveDate,
        changeset: Changeset,
        conn: &C,
    ) -> Result<Option<DateTime<Utc>>, Error>
    where
        C: GenericClient + Sync,
    {
        let sync_datetime = Utc::now();
        let removed_lines: Vec<_> = changeset
            .diffs
            .into_iter()
            .map(|entry| match entry {
                Difference::Same(s) => {
                    DiaryConflict::new(sync_datetime, diary_date, "same".into(), s.into())
                }
                Difference::Rem(s) => {
                    DiaryConflict::new(sync_datetime, diary_date, "rem".into(), s.into())
                }
                Difference::Add(s) => {
                    DiaryConflict::new(sync_datetime, diary_date, "add".into(), s.into())
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
    pub fn new(diary_date: NaiveDate, diary_text: &str) -> Self {
        Self {
            diary_date,
            diary_text: diary_text.into(),
            last_modified: Utc::now(),
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

    pub async fn insert_entry(&self, pool: &PgPool) -> Result<(), Error> {
        let conn = pool.get().await?;
        self._insert_entry(&conn).await?;
        Ok(())
    }

    async fn _update_entry<C>(
        &self,
        conn: &C,
        insert_new: bool,
    ) -> Result<Option<DateTime<Utc>>, Error>
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

    pub async fn update_entry(
        &self,
        pool: &PgPool,
        insert_new: bool,
    ) -> Result<Option<DateTime<Utc>>, Error> {
        let conn = pool.get().await?;
        self._update_entry(&conn, insert_new)
            .await
            .map_err(Into::into)
    }

    pub async fn upsert_entry(
        &self,
        pool: &PgPool,
        insert_new: bool,
    ) -> Result<Option<DateTime<Utc>>, Error> {
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

    pub async fn get_modified_map(
        pool: &PgPool,
    ) -> Result<HashMap<NaiveDate, DateTime<Utc>>, Error> {
        #[derive(FromSqlRow)]
        struct Wrap {
            diary_date: NaiveDate,
            last_modified: DateTime<Utc>,
        }

        let query = query!("SELECT diary_date, last_modified FROM diary_entries");
        let conn = pool.get().await?;
        let output: Vec<Wrap> = query.fetch(&conn).await?;
        Ok(output
            .into_iter()
            .map(|x| (x.diary_date, x.last_modified))
            .collect())
    }

    async fn _get_by_date<C>(date: NaiveDate, conn: &C) -> Result<Option<Self>, Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            "SELECT * FROM diary_entries WHERE diary_date = $date",
            date = date
        );
        query.fetch_opt(conn).await.map_err(Into::into)
    }

    pub async fn get_by_date(date: NaiveDate, pool: &PgPool) -> Result<Option<Self>, Error> {
        let conn = pool.get().await?;
        Self::_get_by_date(date, &conn).await.map_err(Into::into)
    }

    pub async fn get_by_text(search_text: &str, pool: &PgPool) -> Result<Vec<Self>, Error> {
        let search_text: StackString = search_text
            .chars()
            .filter(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' => true,
                _ => false,
            })
            .collect();
        let query = format!(
            r#"
                SELECT * FROM diary_entries
                WHERE diary_text like '%{}%'
                ORDER BY diary_date
            "#,
            search_text
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

    pub async fn get_difference(&self, pool: &PgPool) -> Result<Option<Changeset>, Error> {
        let conn = pool.get().await?;
        self._get_difference(&conn, true).await.map_err(Into::into)
    }

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

    pub async fn get_cache_entries(pool: &PgPool) -> Result<Vec<Self>, Error> {
        let query = query!("SELECT * FROM diary_cache");
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }

    pub async fn get_by_text(search_text: &str, pool: &PgPool) -> Result<Vec<Self>, Error> {
        let search_text: StackString = search_text
            .chars()
            .filter(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' => true,
                _ => false,
            })
            .collect();
        let query = format!(
            r#"
                SELECT * FROM diary_cache
                WHERE diary_text like '%{}%'
            "#,
            search_text,
        );
        let query = query_dyn!(&query)?;
        let conn = pool.get().await?;
        query.fetch(&conn).await.map_err(Into::into)
    }

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
