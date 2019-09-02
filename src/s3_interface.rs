use chrono::{DateTime, NaiveDate, Utc};
use failure::Error;
use log::debug;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::collections::HashMap;
use std::io::{stdout, Write};

use crate::config::Config;
use crate::models::DiaryEntries;
use crate::pgpool::PgPool;
use crate::s3_instance::S3Instance;

#[derive(Clone, Debug)]
pub struct S3Interface {
    config: Config,
    s3_client: S3Instance,
    pool: PgPool,
}

impl S3Interface {
    pub fn new(config: Config, pool: PgPool) -> Self {
        S3Interface {
            s3_client: S3Instance::new(&config.aws_region_name),
            pool,
            config,
        }
    }

    pub fn export_to_s3(&self) -> Result<Vec<DiaryEntries>, Error> {
        let stdout = stdout();
        let s3_key_map: HashMap<NaiveDate, DateTime<Utc>> = self
            .s3_client
            .get_list_of_keys(&self.config.diary_bucket, None)?
            .into_par_iter()
            .filter_map(|obj| {
                obj.key.as_ref().and_then(|key| {
                    NaiveDate::parse_from_str(&key, "%Y-%m-%d.txt")
                        .ok()
                        .and_then(|date| {
                            let last_modified = obj
                                .last_modified
                                .as_ref()
                                .and_then(|lm| DateTime::parse_from_rfc3339(&lm).ok())
                                .map(|d| d.with_timezone(&Utc))
                                .unwrap_or_else(Utc::now);
                            Some((date, last_modified))
                        })
                })
            })
            .collect();
        let results: Result<Vec<_>, Error> = DiaryEntries::get_modified_map(&self.pool)?
            .into_par_iter()
            .map(|(diary_date, last_modified)| {
                let should_update = match s3_key_map.get(&diary_date) {
                    Some(lm) => (*lm - last_modified).num_seconds() > 1,
                    None => true,
                };
                if should_update {
                    if let Some(entry) = DiaryEntries::get_by_date(diary_date, &self.pool)?
                        .into_iter()
                        .nth(0)
                    {
                        writeln!(
                            stdout.lock(),
                            "export s3 date {} lines {}",
                            entry.diary_date,
                            entry.diary_text.match_indices('\n').count()
                        )?;
                        let key = format!("{}.txt", entry.diary_date);
                        self.s3_client.upload_from_string(
                            &entry.diary_text,
                            &self.config.diary_bucket,
                            &key,
                        )?;
                        return Ok(Some(entry));
                    }
                }
                Ok(None)
            })
            .collect();
        Ok(results?.into_iter().filter_map(|x| x).collect())
    }

    pub fn import_from_s3(&self) -> Result<Vec<DiaryEntries>, Error> {
        let stdout = stdout();
        let existing_map = DiaryEntries::get_modified_map(&self.pool)?;

        debug!("{}", self.config.diary_bucket);
        self.s3_client
            .get_list_of_keys(&self.config.diary_bucket, None)?
            .into_par_iter()
            .filter_map(|obj| {
                obj.key.as_ref().and_then(|key| {
                    NaiveDate::parse_from_str(&key, "%Y-%m-%d.txt")
                        .ok()
                        .and_then(|date| {
                            let last_modified = obj
                                .last_modified
                                .as_ref()
                                .and_then(|lm| DateTime::parse_from_rfc3339(&lm).ok())
                                .map(|d| d.with_timezone(&Utc))
                                .unwrap_or_else(Utc::now);
                            let size = obj.size.unwrap_or(0);

                            let should_modify = match existing_map.get(&date) {
                                Some(current_modified) => {
                                    (*current_modified - last_modified).num_seconds() < -1
                                }
                                None => true,
                            };
                            if size > 0 && should_modify {
                                self.s3_client
                                    .download_to_string(&self.config.diary_bucket, &key)
                                    .ok()
                                    .map(|val| DiaryEntries {
                                        diary_date: date,
                                        diary_text: val,
                                        last_modified,
                                    })
                            } else {
                                None
                            }
                        })
                })
            })
            .map(|entry| {
                writeln!(
                    stdout.lock(),
                    "import s3 date {} lines {}",
                    entry.diary_date,
                    entry.diary_text.match_indices('\n').count()
                )?;
                if existing_map.contains_key(&entry.diary_date) {
                    entry.update_entry(&self.pool)?;
                } else {
                    entry.insert_entry(&self.pool)?;
                }
                Ok(entry)
            })
            .collect()
    }
}