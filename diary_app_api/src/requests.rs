use anyhow::{Error, format_err};
use futures::TryStreamExt;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use stack_string::{StackString, format_sstr};
use std::collections::BTreeSet;
use time::Date;
use utoipa::ToSchema;
use uuid::Uuid;

use diary_app_lib::{
    date_time_wrapper::DateTimeWrapper,
    models::{DiaryConflict, DiaryEntries},
};

use super::app::DiaryAppActor;

#[derive(Serialize, Deserialize, ToSchema)]
pub struct SearchOptions {
    // Search Text")]
    pub text: Option<StackString>,
    // Search Date")]
    pub date: Option<Date>,
}

#[derive(Serialize, Deserialize, Default, Copy, Clone, ToSchema)]
pub struct ListOptions {
    // Minimum Date")]
    pub min_date: Option<Date>,
    // Maximum Date")]
    pub max_date: Option<Date>,
    // Start Index")]
    pub start: Option<usize>,
    // Limit")]
    pub limit: Option<usize>,
}

pub enum DiaryAppRequests {
    Search(SearchOptions),
    Insert(StackString),
    Sync,
    Replace { date: Date, text: StackString },
    List(ListOptions),
    Display(Date),
    ListConflicts(Option<Date>),
    ShowConflict(DateTimeWrapper),
    RemoveConflict(DateTimeWrapper),
    CleanConflicts(Date),
    UpdateConflict { id: Uuid, diff_text: StackString },
    CommitConflict(DateTimeWrapper),
}

pub enum DiaryAppOutput {
    Lines(Vec<StackString>),
    Timestamps(Vec<DateTimeWrapper>),
    Dates(Vec<Date>),
    Conflicts(Vec<DiaryConflict>),
}

impl From<Vec<StackString>> for DiaryAppOutput {
    fn from(item: Vec<StackString>) -> Self {
        Self::Lines(item)
    }
}

impl From<Vec<DateTimeWrapper>> for DiaryAppOutput {
    fn from(item: Vec<DateTimeWrapper>) -> Self {
        Self::Timestamps(item)
    }
}

impl From<Vec<Date>> for DiaryAppOutput {
    fn from(item: Vec<Date>) -> Self {
        Self::Dates(item)
    }
}

impl From<Vec<DiaryConflict>> for DiaryAppOutput {
    fn from(value: Vec<DiaryConflict>) -> Self {
        Self::Conflicts(value)
    }
}

impl DiaryAppRequests {
    /// # Errors
    /// Return error if any operation fails
    pub async fn process(self, dapp: &DiaryAppActor) -> Result<DiaryAppOutput, Error> {
        match self {
            DiaryAppRequests::Search(opts) => {
                let body = if let Some(text) = opts.text {
                    let results: Vec<_> = dapp.search_text(&text).await?;
                    results
                } else if let Some(date) = opts.date {
                    let entry = DiaryEntries::get_by_date(date, &dapp.pool)
                        .await?
                        .ok_or_else(|| format_err!("Date should exist {}", date))?;
                    vec![entry.diary_text]
                } else {
                    vec!["".into()]
                };
                Ok(body.into())
            }
            DiaryAppRequests::Insert(text) => {
                let cache = dapp.cache_text(&text).await?;
                Ok(vec![cache.diary_datetime].into())
            }
            DiaryAppRequests::Sync => {
                let output = dapp.sync_everything().await?;
                Ok(output.into())
            }
            DiaryAppRequests::Replace { date, text } => {
                let (entry, _) = dapp.replace_text(date, &text).await?;
                let body: StackString = format_sstr!("{}\n{}", entry.diary_date, entry.diary_text);
                Ok(vec![body].into())
            }
            DiaryAppRequests::List(opts) => {
                let dates = dapp
                    .get_list_of_dates(opts.min_date, opts.max_date, opts.start, opts.limit)
                    .await?;
                Ok(dates.into())
            }
            DiaryAppRequests::Display(date) => {
                let entry = DiaryEntries::get_by_date(date, &dapp.pool)
                    .await?
                    .ok_or_else(|| format_err!("Date should exist {}", date))?;
                Ok(vec![entry.diary_text].into())
            }
            DiaryAppRequests::ListConflicts(None) => {
                let mut conflicts: Vec<_> = DiaryConflict::get_all_dates(&dapp.pool)
                    .await?
                    .try_collect()
                    .await?;
                conflicts.sort();
                conflicts.dedup();
                Ok(conflicts.into())
            }
            DiaryAppRequests::ListConflicts(Some(date)) => {
                let mut conflicts: Vec<_> = DiaryConflict::get_by_date(date, &dapp.pool)
                    .await?
                    .try_collect()
                    .await?;
                conflicts.sort();
                conflicts.dedup();
                Ok(conflicts.into())
            }
            DiaryAppRequests::ShowConflict(datetime) => {
                let conflicts: Vec<_> = DiaryConflict::get_by_datetime(datetime, &dapp.pool)
                    .await?
                    .try_collect()
                    .await?;
                Ok(conflicts.into())
            }
            DiaryAppRequests::RemoveConflict(datetime) => {
                DiaryConflict::remove_by_datetime(datetime, &dapp.pool).await?;
                let body: StackString = format_sstr!("remove {datetime}");
                Ok(vec![body].into())
            }
            DiaryAppRequests::CleanConflicts(date) => {
                let results: Result<Vec<StackString>, Error> =
                    DiaryConflict::get_by_date(date, &dapp.pool)
                        .await?
                        .map_err(Into::into)
                        .and_then(|datetime| {
                            let pool = dapp.pool.clone();
                            async move {
                                DiaryConflict::remove_by_datetime(datetime, &pool).await?;
                                Ok(format_sstr!("remove {datetime}"))
                            }
                        })
                        .try_collect()
                        .await;
                results.map(Into::into)
            }
            DiaryAppRequests::UpdateConflict { id, diff_text } => {
                let new_diff_type = match diff_text.as_str() {
                    "rem" => "rem",
                    "add" => "add",
                    _ => return Err(format_err!("Bad diff type {}", diff_text)),
                };
                DiaryConflict::update_by_id(id, new_diff_type, &dapp.pool).await?;
                let body: StackString = "updated".into();
                Ok(vec![body].into())
            }
            DiaryAppRequests::CommitConflict(datetime) => {
                let conflicts: Vec<_> = DiaryConflict::get_by_datetime(datetime, &dapp.pool)
                    .await?
                    .try_collect()
                    .await?;
                let diary_dates: BTreeSet<Date> =
                    conflicts.iter().map(|entry| entry.diary_date).collect();
                if diary_dates.len() > 1 {
                    return Err(format_err!(
                        "Something has gone horribly wrong {:?}",
                        conflicts
                    ));
                }
                let date = diary_dates.into_iter().next().ok_or_else(|| {
                    format_err!("Something has gone horribly wrong {:?}", conflicts)
                })?;

                let additions = conflicts
                    .into_iter()
                    .filter_map(|entry| {
                        if &entry.diff_type == "add" || &entry.diff_type == "same" {
                            Some(entry.diff_text)
                        } else {
                            None
                        }
                    })
                    .join("\n");
                let (entry, _) = dapp.replace_text(date, &additions).await?;
                let body = format_sstr!("{}\n{}", entry.diary_date, entry.diary_text);
                Ok(vec![body].into())
            }
        }
    }
}
