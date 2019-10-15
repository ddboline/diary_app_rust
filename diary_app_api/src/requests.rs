use actix::{Handler, Message};
use chrono::NaiveDate;
use failure::Error;
use serde::{Deserialize, Serialize};

use diary_app_lib::diary_app_interface::DiaryAppInterface;
use diary_app_lib::models::DiaryEntries;

#[derive(Serialize, Deserialize)]
pub struct SearchOptions {
    pub text: Option<String>,
    pub date: Option<NaiveDate>,
}

#[derive(Serialize, Deserialize)]
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
        }
    }
}
