use chrono::{DateTime, NaiveDate, Utc, Duration};
use failure::{err_msg, Error};
use jwalk::WalkDir;
use std::fs::File;
use std::io::Read;

use crate::config::Config;
use crate::models::DiaryEntries;
use crate::pgpool::PgPool;

pub struct LocalInterface {
    pub config: Config,
    pub pool: PgPool,
}

impl LocalInterface {
    pub fn new() -> Self {
        let config = Config::init_config().expect("Failed to load config");
        LocalInterface {
            pool: PgPool::new(&config.database_url),
            config,
        }
    }

    pub fn import_from_local(&self) -> Result<Vec<DiaryEntries>, Error> {
        let existing_map = DiaryEntries::get_modified_map(&self.pool)?;

        WalkDir::new(&self.config.diary_path)
            .sort(true)
            .preload_metadata(true)
            .into_iter()
            .map(|entry| {
                let entry = entry?;
                let filename = entry
                    .file_name
                    .into_string()
                    .map_err(|_| err_msg("Failed parse"))?;
                if let Ok(date) = NaiveDate::parse_from_str(&filename, "%Y-%m-%d.txt") {
                    if let Some(metadata) = entry.metadata.transpose()? {
                        let filepath = format!("{}/{}", self.config.diary_path, filename);
                        let mut val = String::new();
                        File::open(&filepath)?.read_to_string(&mut val)?;
                        let modified: DateTime<Utc> = metadata.modified()?.into();

                        let should_modify = match existing_map.get(&date) {
                            Some(current_modified) => {
                                (*current_modified - modified) < Duration::seconds(1)
                            }
                            None => true,
                        };

                        if metadata.len() > 0 && should_modify {
                            let d = DiaryEntries {
                                diary_date: date,
                                diary_text: val,
                                last_modified: modified,
                            };
                            return Ok(Some(d));
                        }
                    }
                }
                Ok(None)
            })
            .filter_map(|d| d.transpose())
            .map(|result| {
                result.and_then(|entry| {
                    println!(
                        "date {} lines {}",
                        entry.diary_date,
                        entry.diary_text.match_indices('\n').count()
                    );
                    if existing_map.contains_key(&entry.diary_date) {
                        entry.update_entry(&self.pool)?;
                    } else {
                        entry.insert_entry(&self.pool)?;
                    }
                    Ok(entry)
                })
            })
            .collect()
    }
}
