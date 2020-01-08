use anyhow::{format_err, Error};
use chrono::{DateTime, NaiveDate, Utc};
use lazy_static::lazy_static;
use log::debug;
use parking_lot::Mutex;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rusoto_s3::Object;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::io::{stdout, Write};

use crate::config::Config;
use crate::models::DiaryEntries;
use crate::pgpool::PgPool;
use crate::s3_instance::S3Instance;

const TIME_BUFFER: i64 = 60;

lazy_static! {
    static ref KEY_CACHE: Mutex<(DateTime<Utc>, Vec<KeyMetaData>)> =
        Mutex::new((Utc::now(), Vec::new()));
}

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
        let key = obj.key.as_ref().ok_or_else(|| format_err!("No Key"))?;
        let date = NaiveDate::parse_from_str(&key, "%Y-%m-%d.txt")?;
        let last_modified = obj
            .last_modified
            .as_ref()
            .and_then(|lm| DateTime::parse_from_rfc3339(&lm).ok())
            .map_or_else(Utc::now, |d| d.with_timezone(&Utc));
        let size = obj.size.unwrap_or(0);
        Ok(Self {
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
}

impl S3Interface {
    pub fn new(config: Config, pool: PgPool) -> Self {
        Self {
            s3_client: S3Instance::new(&config.aws_region_name),
            pool,
            config,
        }
    }

    fn fill_cache(&self) -> Result<(), Error> {
        *KEY_CACHE.lock() = (
            Utc::now(),
            self.s3_client
                .get_list_of_keys(&self.config.diary_bucket, None)?
                .into_par_iter()
                .filter_map(|obj| obj.try_into().ok())
                .collect(),
        );
        Ok(())
    }

    pub fn export_to_s3(&self) -> Result<Vec<DiaryEntries>, Error> {
        let stdout = stdout();
        if let Some(key_cache) = KEY_CACHE.try_lock() {
            if (Utc::now() - key_cache.0).num_seconds() > 5 * TIME_BUFFER {
                self.fill_cache()?;
            }
        }
        let s3_key_map: HashMap<NaiveDate, (DateTime<Utc>, i64)> = KEY_CACHE
            .lock()
            .1
            .iter()
            .map(|obj| (obj.date, (obj.last_modified, obj.size)))
            .collect();
        if let Some(mut key_cache) = KEY_CACHE.try_lock() {
            key_cache.1.clear();
        }
        DiaryEntries::get_modified_map(&self.pool)?
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
            .filter_map(Result::transpose)
            .collect()
    }

    pub fn import_from_s3(&self) -> Result<Vec<DiaryEntries>, Error> {
        let stdout = stdout();
        let existing_map = DiaryEntries::get_modified_map(&self.pool)?;

        debug!("{}", self.config.diary_bucket);
        self.fill_cache()?;

        KEY_CACHE
            .lock()
            .1
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
