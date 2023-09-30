use anyhow::{format_err, Error};
use aws_config::SdkConfig;
use aws_sdk_s3::types::Object;
use futures::{stream::FuturesUnordered, TryStreamExt};
use lazy_static::lazy_static;
use log::debug;
use stack_string::{format_sstr, StackString};
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    sync::Arc,
};
use time::{macros::format_description, Date, OffsetDateTime};
use tokio::sync::RwLock;

use crate::{config::Config, models::DiaryEntries, pgpool::PgPool, s3_instance::S3Instance};

const TIME_BUFFER: i64 = 60;

lazy_static! {
    static ref KEY_CACHE: RwLock<(OffsetDateTime, Arc<[KeyMetaData]>)> =
        RwLock::new((OffsetDateTime::now_utc(), Arc::new([])));
}

#[derive(Debug, Clone)]
struct KeyMetaData {
    date: Date,
    last_modified: OffsetDateTime,
    size: i64,
}

impl TryFrom<Object> for KeyMetaData {
    type Error = Error;
    fn try_from(obj: Object) -> Result<Self, Error> {
        let key: StackString = obj
            .key
            .as_ref()
            .ok_or_else(|| format_err!("No Key"))?
            .into();
        let date = Date::parse(&key, format_description!("[year]-[month]-[day].txt"))?;
        let last_modified = obj
            .last_modified
            .and_then(|d| OffsetDateTime::from_unix_timestamp(d.as_secs_f64() as i64).ok())
            .unwrap_or_else(OffsetDateTime::now_utc);
        Ok(Self {
            date,
            last_modified,
            size: obj.size,
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
    #[must_use]
    pub fn new(config: Config, sdk_config: &SdkConfig, pool: PgPool) -> Self {
        Self {
            s3_client: S3Instance::new(sdk_config),
            pool,
            config,
        }
    }

    async fn fill_cache(&self) -> Result<(), Error> {
        let list_of_keys = self
            .s3_client
            .get_list_of_keys(&self.config.diary_bucket, None)
            .await?;
        *KEY_CACHE.write().await = (
            OffsetDateTime::now_utc(),
            list_of_keys
                .into_iter()
                .filter_map(|obj| obj.try_into().ok())
                .collect(),
        );
        Ok(())
    }

    /// # Errors
    /// Return error if s3 api fails
    pub async fn export_to_s3(&self) -> Result<Vec<DiaryEntries>, Error> {
        {
            let key_cache = KEY_CACHE.read().await;
            if (OffsetDateTime::now_utc() - key_cache.0).whole_seconds() > 5 * TIME_BUFFER {
                self.fill_cache().await?;
            }
        }
        let s3_key_map: HashMap<Date, (OffsetDateTime, i64)> = KEY_CACHE
            .read()
            .await
            .1
            .iter()
            .map(|obj| (obj.date, (obj.last_modified, obj.size)))
            .collect();
        let s3_key_map = Arc::new(s3_key_map);
        {
            let mut key_cache = KEY_CACHE.write().await;
            key_cache.1 = Arc::new([]);
        }

        let futures: FuturesUnordered<_> = DiaryEntries::get_modified_map(&self.pool)
            .await?
            .into_iter()
            .map(|(diary_date, last_modified)| {
                let s3_key_map = s3_key_map.clone();
                async move {
                    let should_update = match s3_key_map.get(&diary_date) {
                        Some((lm, s3_size)) => {
                            if (last_modified - *lm).whole_seconds() > 0 {
                                if let Some(entry) =
                                    DiaryEntries::get_by_date(diary_date, &self.pool).await?
                                {
                                    let db_size = entry.diary_text.len() as i64;
                                    if *s3_size != db_size {
                                        debug!(
                                            "last_modified {} {} {} {} {}",
                                            diary_date, *lm, last_modified, s3_size, db_size
                                        );
                                    }
                                    *s3_size < db_size
                                } else {
                                    false
                                }
                            } else {
                                (last_modified - *lm).whole_seconds() > 0
                            }
                        }
                        None => true,
                    };
                    if should_update {
                        return self.upload_entry(diary_date).await;
                    }
                    Ok(None)
                }
            })
            .collect();
        futures
            .try_filter_map(|x| async move { Ok(x) })
            .try_collect()
            .await
    }

    /// # Errors
    /// Return error if s3 api fails
    pub async fn upload_entry(&self, date: Date) -> Result<Option<DiaryEntries>, Error> {
        let Some(entry) = DiaryEntries::get_by_date(date, &self.pool).await? else {
            return Ok(None);
        };
        if entry.diary_text.trim().is_empty() {
            return Ok(None);
        }
        debug!(
            "export s3 date {} lines {}",
            entry.diary_date,
            entry.diary_text.matches('\n').count()
        );
        let key = format_sstr!("{}.txt", entry.diary_date);
        self.s3_client
            .upload_from_string(&entry.diary_text, &self.config.diary_bucket, &key)
            .await?;
        Ok(Some(entry))
    }

    /// # Errors
    /// Return error if s3 api fails
    pub async fn download_entry(&self, date: Date) -> Result<Option<DiaryEntries>, Error> {
        let key = format_sstr!("{date}.txt");
        let (text, last_modified) = self
            .s3_client
            .download_to_string(&self.config.diary_bucket, &key)
            .await?;
        if text.trim().is_empty() {
            return Ok(None);
        }
        let entry = DiaryEntries {
            diary_date: date,
            diary_text: text.into(),
            last_modified: last_modified.into(),
        };
        Ok(Some(entry))
    }

    /// # Errors
    /// Return error if s3 api fails
    pub async fn import_from_s3(&self) -> Result<Vec<DiaryEntries>, Error> {
        let existing_map = Arc::new(DiaryEntries::get_modified_map(&self.pool).await?);

        debug!("{}", self.config.diary_bucket);
        self.fill_cache().await?;

        let key_cache = KEY_CACHE.read().await.1.clone();

        let futures: FuturesUnordered<_> = key_cache
            .iter()
            .map(|obj| {
                let existing_map = existing_map.clone();
                async move {
                    let mut insert_new = true;
                    let should_modify = match existing_map.get(&obj.date) {
                        Some(current_modified) => {
                            insert_new =
                                (*current_modified - obj.last_modified).whole_seconds() < 0;
                            if (*current_modified - obj.last_modified).whole_seconds() < 0 {
                                if let Some(entry) =
                                    DiaryEntries::get_by_date(obj.date, &self.pool).await?
                                {
                                    let db_size = entry.diary_text.len() as i64;
                                    if obj.size != db_size {
                                        debug!(
                                            "last_modified {} {} {} {} {}",
                                            obj.date,
                                            *current_modified,
                                            obj.last_modified,
                                            obj.size,
                                            db_size
                                        );
                                    }
                                    obj.size != db_size
                                } else {
                                    false
                                }
                            } else {
                                (*current_modified - obj.last_modified).whole_seconds() < 0
                            }
                        }
                        None => true,
                    };
                    if obj.size > 0 && should_modify {
                        if let Some(entry) = self.download_entry(obj.date).await? {
                            debug!(
                                "import s3 date {} lines {}",
                                entry.diary_date,
                                entry.diary_text.matches('\n').count()
                            );
                            entry.upsert_entry(&self.pool, insert_new).await?;
                            return Ok(Some(entry));
                        }
                    }
                    Ok(None)
                }
            })
            .collect();
        futures
            .try_filter_map(|x| async move { Ok(x) })
            .try_collect()
            .await
    }

    /// # Errors
    /// Return error if s3 api fails
    pub async fn validate_s3(&self) -> Result<Vec<(Date, usize, usize)>, Error> {
        self.fill_cache().await?;
        let s3_key_map: HashMap<Date, usize> = KEY_CACHE
            .read()
            .await
            .1
            .iter()
            .map(|obj| (obj.date, obj.size as usize))
            .collect();

        let futures: FuturesUnordered<_> = s3_key_map
            .iter()
            .map(|(date, backup_len)| {
                let pool = self.pool.clone();
                async move {
                    let entry = DiaryEntries::get_by_date(*date, &pool)
                        .await?
                        .ok_or_else(|| format_err!("Date should exist {date}"))?;
                    let diary_len = entry.diary_text.len();
                    if diary_len == *backup_len {
                        Ok(None)
                    } else {
                        Ok(Some((*date, *backup_len, diary_len)))
                    }
                }
            })
            .collect();
        futures
            .try_filter_map(|x| async move { Ok(x) })
            .try_collect()
            .await
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use log::debug;

    use crate::{
        config::Config, pgpool::PgPool, s3_instance::S3Instance, s3_interface::S3Interface,
    };

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn test_validate_s3() -> Result<(), Error> {
        let config = Config::init_config()?;
        let sdk_config = aws_config::load_from_env().await;
        let pool = PgPool::new(&config.database_url);
        let s3 = S3Interface::new(config, &sdk_config, pool);
        let results = s3.validate_s3().await?;
        for (date, backup_len, diary_len) in results.iter() {
            println!(
                "date {} backup_len {} diary_len {}",
                date, backup_len, diary_len
            );
        }
        assert!(results.is_empty());

        let sdk_config = aws_config::load_from_env().await;
        let s3_instance = S3Instance::new(&sdk_config).max_keys(100);

        let bucket = s3_instance
            .get_list_of_buckets()
            .await?
            .get(0)
            .and_then(|b| b.name.clone())
            .unwrap_or_else(|| "".to_string());

        let key_list = s3_instance.get_list_of_keys(&bucket, None).await?;
        debug!("{} {}", bucket, key_list.len());
        assert!(key_list.len() > 0);
        Ok(())
    }
}
