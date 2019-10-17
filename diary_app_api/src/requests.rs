use actix::{Handler, Message};
use chrono::{DateTime, NaiveDate, Utc};
use failure::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use diary_app_lib::diary_app_interface::DiaryAppInterface;
use diary_app_lib::models::{DiaryConflict, DiaryEntries};

#[derive(Serialize, Deserialize)]
pub struct SearchOptions {
    pub text: Option<String>,
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
    Insert(String),
    Sync,
    Replace { date: NaiveDate, text: String },
    List(ListOptions),
    Display(NaiveDate),
    ListConflicts(Option<NaiveDate>),
    ShowConflict(DateTime<Utc>),
    RemoveConflict(DateTime<Utc>),
}

impl Message for DiaryAppRequests {
    type Result = Result<Vec<String>, Error>;
}

impl Handler<DiaryAppRequests> for DiaryAppInterface {
    type Result = Result<Vec<String>, Error>;
    fn handle(&mut self, req: DiaryAppRequests, _: &mut Self::Context) -> Self::Result {
        match req {
            DiaryAppRequests::Search(opts) => {
                let body = if let Some(text) = opts.text {
                    let results: Vec<_> = self.search_text(&text)?;
                    results
                } else if let Some(date) = opts.date {
                    let entry = DiaryEntries::get_by_date(date, &self.pool)?;
                    vec![entry.diary_text.into()]
                } else {
                    vec!["".to_string()]
                };
                Ok(body)
            }
            DiaryAppRequests::Insert(text) => {
                let cache = self.cache_text(text.into())?;
                Ok(vec![format!("{}", cache.diary_datetime)])
            }
            DiaryAppRequests::Sync => {
                let output = self.sync_everything()?;
                Ok(output)
            }
            DiaryAppRequests::Replace { date, text } => {
                let entry = self.replace_text(date, text.into())?;
                let body = format!("{}\n{}", entry.diary_date, entry.diary_text);
                Ok(vec![body])
            }
            DiaryAppRequests::List(opts) => {
                let dates: Vec<_> = self
                    .get_list_of_dates(opts.min_date, opts.max_date, opts.start, opts.limit)?
                    .into_iter()
                    .map(|x| x.to_string())
                    .collect();
                Ok(dates)
            }
            DiaryAppRequests::Display(date) => {
                let entry = DiaryEntries::get_by_date(date, &self.pool)?;
                Ok(vec![entry.diary_text.into()])
            }
            DiaryAppRequests::ListConflicts(None) => {
                let conflicts: BTreeSet<_> = DiaryConflict::get_all_dates(&self.pool)?
                    .into_iter()
                    .map(|x| x.to_string())
                    .collect();
                Ok(conflicts.into_iter().collect())
            }
            DiaryAppRequests::ListConflicts(Some(date)) => {
                let conflicts: BTreeSet<_> = DiaryConflict::get_by_date(date, &self.pool)?
                    .into_iter()
                    .map(|entry| entry.format("%Y-%m-%dT%H:%M:%S%.fZ").to_string())
                    .collect();
                Ok(conflicts.into_iter().collect())
            }
            DiaryAppRequests::ShowConflict(datetime) => {
                let conflicts: Vec<_> = DiaryConflict::get_by_datetime(datetime, &self.pool)?
                    .into_iter()
                    .map(|entry| {
                        let nlines = entry.diff_text.split('\n').count() + 1;
                        match entry.diff_type.as_ref() {
                            "rem" => format!(
                                r#"<textarea style="color:Red;" cols=100 rows={}>{}</textarea><br>"#,
                                nlines,
                                entry.diff_text
                            ),
                            "add" => format!(
                                r#"<textarea style="color:Blue;" cols=100 rows={}>{}</textarea><br>"#,
                                nlines,
                                entry.diff_text
                            ),
                            _ => format!("<textarea cols=100 rows={}>{}</textarea><br>", nlines, entry.diff_text),
                        }
                    })
                    .collect();
                Ok(conflicts)
            }
            DiaryAppRequests::RemoveConflict(datetime) => {
                DiaryConflict::remove_by_datetime(datetime, &self.pool)?;
                Ok(vec![format!("remove {}", datetime)])
            }
        }
    }
}
