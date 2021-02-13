use anyhow::{format_err, Error};
use chrono::{DateTime, NaiveDate, Utc};
use futures::future::try_join_all;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::collections::BTreeSet;

use diary_app_lib::models::{DiaryConflict, DiaryEntries};

use super::app::DiaryAppActor;

#[derive(Serialize, Deserialize)]
pub struct SearchOptions {
    pub text: Option<StackString>,
    pub date: Option<NaiveDate>,
}

#[derive(Serialize, Deserialize, Default, Copy, Clone)]
pub struct ListOptions {
    pub min_date: Option<NaiveDate>,
    pub max_date: Option<NaiveDate>,
    pub start: Option<usize>,
    pub limit: Option<usize>,
}

pub enum DiaryAppRequests {
    Search(SearchOptions),
    Insert(StackString),
    Sync,
    Replace { date: NaiveDate, text: StackString },
    List(ListOptions),
    Display(NaiveDate),
    ListConflicts(Option<NaiveDate>),
    ShowConflict(DateTime<Utc>),
    RemoveConflict(DateTime<Utc>),
    CleanConflicts(NaiveDate),
    UpdateConflict { id: i32, diff_text: StackString },
    CommitConflict(DateTime<Utc>),
}

impl DiaryAppRequests {
    pub async fn handle(self, dapp: &DiaryAppActor) -> Result<Vec<StackString>, Error> {
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
                Ok(body)
            }
            DiaryAppRequests::Insert(text) => {
                let cache = dapp.cache_text(&text).await?;
                Ok(vec![format!("{}", cache.diary_datetime).into()])
            }
            DiaryAppRequests::Sync => {
                let output = dapp.sync_everything().await?;
                Ok(output)
            }
            DiaryAppRequests::Replace { date, text } => {
                let (entry, _) = dapp.replace_text(date, &text).await?;
                let body = format!("{}\n{}", entry.diary_date, entry.diary_text).into();
                Ok(vec![body])
            }
            DiaryAppRequests::List(opts) => {
                let dates = dapp
                    .get_list_of_dates(opts.min_date, opts.max_date, opts.start, opts.limit)
                    .await?
                    .into_iter()
                    .map(|x| x.to_string().into())
                    .collect();
                Ok(dates)
            }
            DiaryAppRequests::Display(date) => {
                let entry = DiaryEntries::get_by_date(date, &dapp.pool)
                    .await?
                    .ok_or_else(|| format_err!("Date should exist {}", date))?;
                Ok(vec![entry.diary_text])
            }
            DiaryAppRequests::ListConflicts(None) => {
                let conflicts: BTreeSet<_> = DiaryConflict::get_all_dates(&dapp.pool)
                    .await?
                    .into_iter()
                    .map(|x| x.to_string().into())
                    .collect();
                Ok(conflicts.into_iter().collect())
            }
            DiaryAppRequests::ListConflicts(Some(date)) => {
                let conflicts: BTreeSet<_> = DiaryConflict::get_by_date(date, &dapp.pool)
                    .await?
                    .into_iter()
                    .map(|entry| entry.format("%Y-%m-%dT%H:%M:%S%.fZ").to_string().into())
                    .collect();
                Ok(conflicts.into_iter().collect())
            }
            DiaryAppRequests::ShowConflict(datetime) => {
                let conflicts = DiaryConflict::get_by_datetime(datetime, &dapp.pool).await?;
                let diary_dates: BTreeSet<NaiveDate> =
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

                let conflicts = conflicts
                    .into_iter()
                    .map(|entry| {
                        let nlines = entry.diff_text.split('\n').count() + 1;
                        match entry.diff_type.as_ref() {
                            "rem" => format!(
                                r#"<textarea style="color:Red;" cols=100 rows={}
                                   >{}</textarea>
                                   <input type="button" name="add" value="Add" onclick="updateConflictAdd({}, '{}', '{}');">
                                   <br>"#,
                                nlines,
                                entry.diff_text,
                                entry.id,
                                date,
                                datetime.format("%Y-%m-%dT%H:%M:%S%.fZ"),
                            ).into(),
                            "add" => format!(
                                r#"<textarea style="color:Blue;" cols=100 rows={}
                                   >{}</textarea>
                                   <input type="button" name="rm" value="Rm" onclick="updateConflictRem({}, '{}', '{}');">
                                   <br>"#,
                                nlines,
                                entry.diff_text,
                                entry.id,
                                date,
                                datetime.format("%Y-%m-%dT%H:%M:%S%.fZ"),
                            ).into(),
                            _ => format!("<textarea cols=100 rows={}>{}</textarea><br>", nlines, entry.diff_text).into(),
                        }
                    })
                    .collect();
                Ok(conflicts)
            }
            DiaryAppRequests::RemoveConflict(datetime) => {
                DiaryConflict::remove_by_datetime(datetime, &dapp.pool).await?;
                Ok(vec![format!("remove {}", datetime).into()])
            }
            DiaryAppRequests::CleanConflicts(date) => {
                let futures = DiaryConflict::get_by_date(date, &dapp.pool)
                    .await?
                    .into_iter()
                    .map(|datetime| {
                        let pool = dapp.pool.clone();
                        async move {
                            DiaryConflict::remove_by_datetime(datetime, &pool).await?;
                            Ok(format!("remove {}", datetime).into())
                        }
                    });
                try_join_all(futures).await
            }
            DiaryAppRequests::UpdateConflict { id, diff_text } => {
                let new_diff_type = match diff_text.as_str() {
                    "rem" => "rem",
                    "add" => "add",
                    _ => return Err(format_err!("Bad diff type {}", diff_text)),
                };
                DiaryConflict::update_by_id(id, new_diff_type, &dapp.pool).await?;
                Ok(Vec::new())
            }
            DiaryAppRequests::CommitConflict(datetime) => {
                let conflicts = DiaryConflict::get_by_datetime(datetime, &dapp.pool).await?;
                let diary_dates: BTreeSet<NaiveDate> =
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
                let body = format!("{}\n{}", entry.diary_date, entry.diary_text).into();
                Ok(vec![body])
            }
        }
    }
}
