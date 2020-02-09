use anyhow::{format_err, Error};
use chrono::{DateTime, NaiveDate, Utc};
use lazy_static::lazy_static;
use log::debug;
use rusoto_s3::Object;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::io::{stdout, Write};
use tokio::sync::Mutex;

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

    async fn fill_cache(&self) -> Result<(), Error> {
        *KEY_CACHE.lock().await = (
            Utc::now(),
            self.s3_client
                .get_list_of_keys(&self.config.diary_bucket, None)
                .await?
                .into_iter()
                .filter_map(|obj| obj.try_into().ok())
                .collect(),
        );
        Ok(())
    }

    pub async fn export_to_s3(&self) -> Result<Vec<DiaryEntries>, Error> {
        let stdout = stdout();
        {
            let key_cache = KEY_CACHE.lock().await;
            if (Utc::now() - key_cache.0).num_seconds() > 5 * TIME_BUFFER {
                self.fill_cache().await?;
            }
        }
        let s3_key_map: HashMap<NaiveDate, (DateTime<Utc>, i64)> = KEY_CACHE
            .lock()
            .await
            .1
            .iter()
            .map(|obj| (obj.date, (obj.last_modified, obj.size)))
            .collect();
        {
            let mut key_cache = KEY_CACHE.lock().await;
            key_cache.1.clear();
        }
        let mut results = Vec::new();
        for (diary_date, last_modified) in DiaryEntries::get_modified_map(&self.pool).await? {
            let should_update = match s3_key_map.get(&diary_date) {
                Some((lm, sz)) => {
                    if (last_modified - *lm).num_seconds() > -TIME_BUFFER {
                        if let Ok(entry) = DiaryEntries::get_by_date(diary_date, &self.pool).await {
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
                if let Ok(entry) = DiaryEntries::get_by_date(diary_date, &self.pool).await {
                    if entry.diary_text.trim().is_empty() {
                        continue;
                    }
                    writeln!(
                        stdout.lock(),
                        "export s3 date {} lines {}",
                        entry.diary_date,
                        entry.diary_text.match_indices('\n').count()
                    )?;
                    let key = format!("{}.txt", entry.diary_date);
                    self.s3_client
                        .upload_from_string(&entry.diary_text, &self.config.diary_bucket, &key)
                        .await?;
                    results.push(entry);
                }
            }
        }
        Ok(results)
    }

    pub async fn import_from_s3(&self) -> Result<Vec<DiaryEntries>, Error> {
        let stdout = stdout();
        let existing_map = DiaryEntries::get_modified_map(&self.pool).await?;

        debug!("{}", self.config.diary_bucket);
        self.fill_cache().await?;

        let mut entries = Vec::new();

        for obj in KEY_CACHE.lock().await.1.as_slice() {
            let should_modify = match existing_map.get(&obj.date) {
                Some(current_modified) => {
                    if (*current_modified - obj.last_modified).num_seconds() < TIME_BUFFER {
                        if let Ok(entry) = DiaryEntries::get_by_date(obj.date, &self.pool).await {
                            let ln = entry.diary_text.len() as i64;
                            if obj.size != ln {
                                debug!(
                                    "last_modified {} {} {} {} {}",
                                    obj.date, *current_modified, obj.last_modified, obj.size, ln
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
                if let Ok(val) = self
                    .s3_client
                    .download_to_string(&self.config.diary_bucket, &obj.key)
                    .await
                {
                    let entry = DiaryEntries {
                        diary_date: obj.date,
                        diary_text: val,
                        last_modified: obj.last_modified,
                    };
                    if entry.diary_text.trim().is_empty() {
                        writeln!(
                            stdout.lock(),
                            "import s3 date {} lines {}",
                            entry.diary_date,
                            entry.diary_text.match_indices('\n').count()
                        )?;
                        let (entry, _) = entry.upsert_entry(&self.pool).await?;
                        entries.push(entry);
                    }
                }
            }
        }
        Ok(entries)
    }
}
