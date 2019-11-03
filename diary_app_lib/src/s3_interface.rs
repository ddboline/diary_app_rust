use chrono::{DateTime, NaiveDate, Utc};
use failure::{err_msg, Error};
use log::debug;
use parking_lot::Mutex;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rusoto_s3::Object;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::io::{stdout, Write};
use std::sync::Arc;

use crate::config::Config;
use crate::models::DiaryEntries;
use crate::pgpool::PgPool;
use crate::s3_instance::S3Instance;

const TIME_BUFFER: i64 = 60;

#[derive(Debug, Clone)]
struct KeyMetaData {
    key: String,
    date: NaiveDate,
    last_modified: DateTime<Utc>,
    size: i64,
}

impl TryFrom<Object> for KeyMetaData {
    type Error = Error;
    fn try_from(obj: Object) -> Result<Self, Error> {
        let key = obj.key.as_ref().ok_or_else(|| err_msg("No Key"))?;
        let date = NaiveDate::parse_from_str(&key, "%Y-%m-%d.txt")?;
        let last_modified = obj
            .last_modified
            .as_ref()
            .and_then(|lm| DateTime::parse_from_rfc3339(&lm).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        let size = obj.size.unwrap_or(0);
        Ok(KeyMetaData {
            key: key.clone(),
            date,
            last_modified,
            size,
        })
    }
}

#[derive(Clone, Debug)]
pub struct S3Interface {
    config: Config,
    s3_client: S3Instance,
    pool: PgPool,
    key_cache: Arc<Mutex<Vec<KeyMetaData>>>,
}

impl S3Interface {
    pub fn new(config: Config, pool: PgPool) -> Self {
        S3Interface {
            s3_client: S3Instance::new(&config.aws_region_name),
            pool,
            config,
            key_cache: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn export_to_s3(&self) -> Result<Vec<DiaryEntries>, Error> {
        let stdout = stdout();
        let s3_key_map: HashMap<NaiveDate, (DateTime<Utc>, i64)> = self
            .key_cache
            .lock()
            .iter()
            .map(|obj| (obj.date, (obj.last_modified, obj.size)))
            .collect();
        let results: Result<Vec<_>, Error> = DiaryEntries::get_modified_map(&self.pool)?
            .into_par_iter()
            .map(|(diary_date, last_modified)| {
                let should_update = match s3_key_map.get(&diary_date) {
                    Some((lm, sz)) => {
                        if (last_modified - *lm).num_seconds() > -TIME_BUFFER {
                            if let Ok(entry) = DiaryEntries::get_by_date(diary_date, &self.pool) {
                                let ln = entry.diary_text.len() as i64;
                                if *sz != ln {
                                    debug!(
                                        "last_modified {} {} {} {} {}",
                                        diary_date, *lm, last_modified, sz, ln
                                    );
                                }
                                *sz < ln
                            } else {
                                false
                            }
                        } else {
                            (last_modified - *lm).num_seconds() > TIME_BUFFER
                        }
                    }
                    None => true,
                };
                if should_update {
                    if let Ok(entry) = DiaryEntries::get_by_date(diary_date, &self.pool) {
                        if entry.diary_text.trim().is_empty() {
                            return Ok(None);
                        }
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
            .filter_map(|x| x.transpose())
            .collect();
        Ok(results?)
    }

    pub fn import_from_s3(&self) -> Result<Vec<DiaryEntries>, Error> {
        let stdout = stdout();
        let existing_map = DiaryEntries::get_modified_map(&self.pool)?;

        debug!("{}", self.config.diary_bucket);
        *self.key_cache.lock() = self
            .s3_client
            .get_list_of_keys(&self.config.diary_bucket, None)?
            .into_par_iter()
            .filter_map(|obj| obj.try_into().ok())
            .collect();

        self.key_cache
            .lock()
            .as_slice()
            .par_iter()
            .filter_map(|obj| {
                let should_modify = match existing_map.get(&obj.date) {
                    Some(current_modified) => {
                        if (*current_modified - obj.last_modified).num_seconds() < TIME_BUFFER {
                            if let Ok(entry) = DiaryEntries::get_by_date(obj.date, &self.pool) {
                                let ln = entry.diary_text.len() as i64;
                                if obj.size != ln {
                                    debug!(
                                        "last_modified {} {} {} {} {}",
                                        obj.date,
                                        *current_modified,
                                        obj.last_modified,
                                        obj.size,
                                        ln
                                    );
                                }
                                obj.size > ln
                            } else {
                                false
                            }
                        } else {
                            (*current_modified - obj.last_modified).num_seconds() < -TIME_BUFFER
                        }
                    }
                    None => true,
                };
                if obj.size > 0 && should_modify {
                    self.s3_client
                        .download_to_string(&self.config.diary_bucket, &obj.key)
                        .ok()
                        .map(|val| DiaryEntries {
                            diary_date: obj.date,
                            diary_text: val.into(),
                            last_modified: obj.last_modified,
                        })
                } else {
                    None
                }
            })
            .filter(|entry| !entry.diary_text.trim().is_empty())
            .map(|entry| {
                writeln!(
                    stdout.lock(),
                    "import s3 date {} lines {}",
                    entry.diary_date,
                    entry.diary_text.match_indices('\n').count()
                )?;
                entry.upsert_entry(&self.pool)?;
                Ok(entry)
            })
            .collect()
    }
}
