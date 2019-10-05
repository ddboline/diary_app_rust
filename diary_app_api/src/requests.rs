use actix::{Handler, Message};
use chrono::NaiveDate;
use failure::Error;
use serde::{Deserialize, Serialize};

use diary_app_lib::diary_app_interface::DiaryAppInterface;

#[derive(Serialize, Deserialize)]
pub struct SearchOptions {
    pub text: Option<String>,
    pub date: Option<NaiveDate>,
    pub api: Option<bool>,
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
                let body = if opts.api.unwrap_or(false) {
                    body
                } else {
                    format!(
                        r#"<textarea autofocus readonly="readonly" rows=50 cols=100>{}</textarea>"#,
                        body
                    )
                };
                Ok(body)
            }
            DiaryAppRequests::Insert(text) => {
                let cache = self.cache_text(text.into())?;
                Ok(format!("{}", cache.diary_datetime))
            }
            DiaryAppRequests::Sync => {
                let output = self.sync_everything()?;
                let output = format!(
                    r#"<textarea autofocus readonly="readonly" rows=50 cols=100>{}</textarea>"#,
                    output.join("\n")
                );

                Ok(output)
            }
            DiaryAppRequests::Replace { date, text } => {
                let entry = self.replace_text(date, text.into())?;
                let body = format!("{}\n{}", entry.diary_date, entry.diary_text);
                let body = format!(
                    r#"<textarea autofocus readonly="readonly" rows=50 cols=100>{}</textarea>"#,
                    body
                );
                Ok(body)
            }
        }
    }
}
