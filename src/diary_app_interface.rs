use chrono::{Duration, NaiveDate, Utc};
use failure::{err_msg, Error};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::collections::HashSet;

use crate::config::Config;
use crate::local_interface::LocalInterface;
use crate::models::{DiaryCache, DiaryEntries};
use crate::pgpool::PgPool;
use crate::s3_interface::S3Interface;
use crate::ssh_instance::SSHInstance;

pub struct DiaryAppInterface {
    pub config: Config,
    pub pool: PgPool,
    pub local: LocalInterface,
    pub s3: S3Interface,
}

impl DiaryAppInterface {
    pub fn new(config: Config, pool: PgPool) -> Self {
        Self {
            local: LocalInterface::new(config.clone(), pool.clone()),
            s3: S3Interface::new(config.clone(), pool.clone()),
            pool,
            config,
        }
    }

    pub fn cache_text(&self, diary_text: &str) -> Result<DiaryCache, Error> {
        let dc = DiaryCache {
            diary_datetime: Utc::now(),
            diary_text: diary_text.to_string(),
        };
        dc.insert_entry(&self.pool)?;
        Ok(dc)
    }

    pub fn search_text(&self, search_text: &str) -> Result<Vec<String>, Error> {
        if let Ok(date) = NaiveDate::parse_from_str(search_text, "%Y-%m-%d") {
            let mut de_entries: Vec<_> = DiaryEntries::get_by_date(date, &self.pool)?
                .into_iter()
                .map(|entry| format!("{}\n{}", entry.diary_date, entry.diary_text))
                .collect();
            let dc_entries: Vec<_> = DiaryCache::get_cache_entries(&self.pool)?
                .into_iter()
                .filter_map(|entry| {
                    if entry.diary_datetime.naive_local().date() == date {
                        Some(format!("{}\n{}", entry.diary_datetime, entry.diary_text))
                    } else {
                        None
                    }
                })
                .collect();
            de_entries.extend_from_slice(&dc_entries);
            Ok(de_entries)
        } else {
            let mut de_entries: Vec<_> = DiaryEntries::get_by_text(search_text, &self.pool)?
                .into_iter()
                .map(|entry| format!("{}\n{}", entry.diary_date, entry.diary_text))
                .collect();
            let dc_entries: Vec<_> = DiaryCache::get_by_text(search_text, &self.pool)?
                .into_iter()
                .map(|entry| {
                    format!(
                        "{}\n{}",
                        entry.diary_datetime.format("%Y-%m-%dT%H:%M:%SZ"),
                        entry.diary_text
                    )
                })
                .collect();
            de_entries.extend_from_slice(&dc_entries);
            Ok(de_entries)
        }
    }

    pub fn sync_entries(&self) -> Result<Vec<DiaryEntries>, Error> {
        let mut new_entries = self.local.import_from_local()?;
        new_entries.extend_from_slice(&self.s3.import_from_s3()?);
        new_entries.extend_from_slice(&self.s3.export_to_s3()?);
        self.sync_ssh()?;

        Ok(new_entries)
    }

    pub fn sync_merge_cache_to_entries(&self) -> Result<(), Error> {
        let results: Result<Vec<_>, Error> = DiaryCache::get_cache_entries(&self.pool)?
            .into_par_iter()
            .map(|entry| {
                let previous_date = (Utc::now() - Duration::days(4)).naive_local().date();
                let entry_date = entry.diary_datetime.naive_local().date();
                if entry_date <= previous_date {
                    if let Some(mut current_entry) =
                        DiaryEntries::get_by_date(entry_date, &self.pool)?
                            .into_iter()
                            .nth(0)
                    {
                        current_entry.diary_text =
                            format!("{}\n{}", current_entry.diary_text, entry.diary_text);
                        current_entry.update_entry(&self.pool)?;
                        entry.delete_entry(&self.pool)?;
                    }
                }
                Ok(())
            })
            .collect();
        results.map(|_| ())
    }

    pub fn serialize_cache(&self) -> Result<Vec<String>, Error> {
        DiaryCache::get_cache_entries(&self.pool)?
            .into_iter()
            .map(|entry| serde_json::to_string(&entry).map_err(err_msg))
            .collect()
    }

    pub fn sync_ssh(&self) -> Result<(), Error> {
        if let Some(ssh_url) = self.config.ssh_url.as_ref() {
            if ssh_url.scheme() != "ssh" {
                return Ok(());
            }
            let cache_set: HashSet<_> = DiaryCache::get_cache_entries(&self.pool)?.into_iter().map(|entry| {
                entry.diary_datetime
            }).collect();
            let command = format!("/usr/bin/diary-app-rust ser");
            let ssh_inst = SSHInstance::from_url(ssh_url)?;
            for line in ssh_inst.run_command_stream_stdout(&command)? {
                let item: DiaryCache = serde_json::from_str(&line)?;
                if !cache_set.contains(&item.diary_datetime) {
                    println!("{:?}", item);
                    item.insert_entry(&self.pool)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::diary_app_interface::DiaryAppInterface;
    use crate::pgpool::PgPool;

    #[test]
    fn test_sync_ssh() {
        let config = Config::init_config().unwrap();
        let pool = PgPool::new(&config.database_url);
        let dap = DiaryAppInterface::new(config, pool);
        dap.sync_ssh().unwrap();
        assert!(false);
    }
}
