use actix::{Handler, Message};
use chrono::{Datelike, NaiveDate};
use failure::Error;
use serde::{Deserialize, Serialize};

use diary_app_lib::diary_app_interface::DiaryAppInterface;
use diary_app_lib::pgpool::PgPool;

#[derive(Serialize, Deserialize)]
pub struct SearchOptions {
    pub text: Option<String>,
    pub date: Option<NaiveDate>,
}

pub enum DiaryAppRequests {
    Search(SearchOptions),
    Insert(String),
    Sync,
    Replace { date: NaiveDate, text: String },
}

impl Message for DiaryAppRequests {
    type Result = Result<String, Error>;
}

impl Handler<DiaryAppRequests> for DiaryAppInterface {
    type Result = Result<String, Error>;
    fn handle(&mut self, req: DiaryAppRequests, _: &mut Self::Context) -> Self::Result {
        match req {
            DiaryAppRequests::Search(opts) => {
                let body = if let Some(text) = opts.text {
                    let results: Vec<_> = self.search_text(&text)?;
                    results.join("\n")
                } else if let Some(date) = opts.date {
                    let text = format!("{}", date);
                    let results: Vec<_> = self.search_text(&text)?;
                    results.join("\n")
                } else {
                    "".to_string()
                };
                Ok(body)
            }
            DiaryAppRequests::Insert(text) => Ok("".to_string()),
            DiaryAppRequests::Sync => Ok("".to_string()),
            DiaryAppRequests::Replace { date, text } => Ok("".to_string()),
        }
    }
}
