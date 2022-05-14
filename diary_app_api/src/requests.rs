use anyhow::{format_err, Error};
use futures::future::try_join_all;
use itertools::Itertools;
use rweb::Schema;
use rweb_helper::DateType;
use serde::{Deserialize, Serialize};
use stack_string::{format_sstr, StackString};
use std::collections::BTreeSet;
use time::{macros::format_description, Date};
use uuid::Uuid;

use diary_app_lib::{
    date_time_wrapper::DateTimeWrapper,
    models::{DiaryConflict, DiaryEntries},
};

use super::app::DiaryAppActor;

#[derive(Serialize, Deserialize, Schema)]
pub struct SearchOptions {
    #[schema(description = "Search Text")]
    pub text: Option<StackString>,
    #[schema(description = "Search Date")]
    pub date: Option<DateType>,
}

#[derive(Serialize, Deserialize, Default, Copy, Clone, Schema)]
pub struct ListOptions {
    #[schema(description = "Minimum Date")]
    pub min_date: Option<DateType>,
    #[schema(description = "Maximum Date")]
    pub max_date: Option<DateType>,
    #[schema(description = "Start Index")]
    pub start: Option<usize>,
    #[schema(description = "Limit")]
    pub limit: Option<usize>,
}

pub enum DiaryAppRequests {
    Search(SearchOptions),
    Insert(StackString),
    Sync,
    Replace { date: Date, text: StackString },
    List(ListOptions),
    Display(Date),
    ListConflicts(Option<DateType>),
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

impl DiaryAppRequests {
    /// # Errors
    /// Return error if any operation fails
    pub async fn handle(self, dapp: &DiaryAppActor) -> Result<DiaryAppOutput, Error> {
        match self {
            DiaryAppRequests::Search(opts) => {
                let body = if let Some(text) = opts.text {
                    let results: Vec<_> = dapp.search_text(&text).await?;
                    results
                } else if let Some(date) = opts.date.map(Into::into) {
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
                    .get_list_of_dates(
                        opts.min_date.map(Into::into),
                        opts.max_date.map(Into::into),
                        opts.start,
                        opts.limit,
                    )
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
                let mut conflicts = DiaryConflict::get_all_dates(&dapp.pool).await?;
                conflicts.sort();
                conflicts.dedup();
                Ok(conflicts.into())
            }
            DiaryAppRequests::ListConflicts(Some(date)) => {
                let mut conflicts = DiaryConflict::get_by_date(date.into(), &dapp.pool).await?;
                conflicts.sort();
                conflicts.dedup();
                Ok(conflicts.into())
            }
            DiaryAppRequests::ShowConflict(datetime) => {
                let conflicts = DiaryConflict::get_by_datetime(datetime, &dapp.pool).await?;
                let diary_dates: BTreeSet<Date> =
                    conflicts.iter().map(|entry| entry.diary_date).collect();
                if diary_dates.len() > 1 {
                    return Err(format_err!(
                        "Something has gone horribly wrong {datetime}, {conflicts:?}"
                    ));
                }
                let date = diary_dates.into_iter().next().ok_or_else(|| {
                    format_err!("Something has gone horribly wrong {datetime} {conflicts:?}")
                })?;

                let conflicts: Vec<StackString> = conflicts
                    .into_iter()
                    .map(|entry| {
                        let nlines = entry.diff_text.split('\n').count() + 1;
                        let id = entry.id;
                        let diff = &entry.diff_text;
                        let dt = datetime.format(format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]Z")).unwrap_or_else(|_| "".into());
                        match entry.diff_type.as_ref() {
                            "rem" => format_sstr!(
                                r#"<textarea style="color:Red;" cols=100 rows={nlines}
                                   >{diff}</textarea>
                                   <input type="button" name="add" value="Add" onclick="updateConflictAdd('{id}', '{date}', '{dt}');">
                                   <br>"#
                            ),
                            "add" => format_sstr!(
                                r#"<textarea style="color:Blue;" cols=100 rows={nlines}
                                   >{diff}</textarea>
                                   <input type="button" name="rm" value="Rm" onclick="updateConflictRem('{id}', '{date}', '{dt}');">
                                   <br>"#
                            ),
                            _ => format_sstr!("<textarea cols=100 rows={nlines}>{diff}</textarea><br>"),
                        }
                    })
                    .collect();
                Ok(conflicts.into())
            }
            DiaryAppRequests::RemoveConflict(datetime) => {
                DiaryConflict::remove_by_datetime(datetime, &dapp.pool).await?;
                let body: StackString = format_sstr!("remove {datetime}");
                Ok(vec![body].into())
            }
            DiaryAppRequests::CleanConflicts(date) => {
                let futures = DiaryConflict::get_by_date(date, &dapp.pool)
                    .await?
                    .into_iter()
                    .map(|datetime| {
                        let pool = dapp.pool.clone();
                        async move {
                            DiaryConflict::remove_by_datetime(datetime, &pool).await?;
                            Ok(format_sstr!("remove {datetime}"))
                        }
                    });
                let results: Result<Vec<StackString>, Error> = try_join_all(futures).await;
                results.map(Into::into).map_err(Into::into)
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
                let conflicts = DiaryConflict::get_by_datetime(datetime, &dapp.pool).await?;
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
