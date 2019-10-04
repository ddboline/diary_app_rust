use actix::sync::SyncContext;
use actix::Actor;
use chrono::{Datelike, Local, NaiveDate, Utc};
use crossbeam_utils::thread;
use failure::{err_msg, Error};
use log::debug;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use regex::Regex;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::config::Config;
use crate::local_interface::LocalInterface;
use crate::models::{DiaryCache, DiaryEntries};
use crate::pgpool::PgPool;
use crate::s3_interface::S3Interface;
use crate::ssh_instance::SSHInstance;

#[derive(Clone)]
pub struct DiaryAppInterface {
    pub config: Config,
    pub pool: PgPool,
    pub local: LocalInterface,
    pub s3: S3Interface,
}

impl Actor for DiaryAppInterface {
    type Context = SyncContext<Self>;
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

    pub fn cache_text<'a>(&self, diary_text: Cow<'a, str>) -> Result<DiaryCache<'a>, Error> {
        let dc = DiaryCache {
            diary_datetime: Utc::now(),
            diary_text,
        };
        dc.insert_entry(&self.pool)?;
        Ok(dc)
    }

    fn get_matching_dates(
        &self,
        year: Option<&str>,
        month: Option<&str>,
        day: Option<&str>,
    ) -> Result<Vec<NaiveDate>, Error> {
        let matching_dates: Vec<_> = DiaryEntries::get_modified_map(&self.pool)?
            .into_iter()
            .map(|(d, _)| d)
            .filter(|date| {
                if let Some(y) = year {
                    let result = if let Some(m) = month {
                        let result = if let Some(d) = day {
                            d == format!("{:02}", date.day())
                        } else {
                            true
                        };
                        result && (m == format!("{:02}", date.month()))
                    } else {
                        true
                    };
                    result && (y == format!("{:04}", date.year()))
                } else {
                    false
                }
            })
            .collect();
        Ok(matching_dates)
    }

    pub fn search_text(&self, search_text: &str) -> Result<Vec<String>, Error> {
        let ymd_reg = Regex::new(r"(?P<year>\d{4})-(?P<month>\d{2})-(?P<day>\d{2})")?;
        let ym_reg = Regex::new(r"(?P<year>\d{4})-(?P<month>\d{2})")?;
        let y_reg = Regex::new(r"(?P<year>\d{4})")?;

        let mut dates = Vec::new();
        if ymd_reg.is_match(search_text) {
            for cap in ymd_reg.captures_iter(search_text) {
                let year = cap.name("year").map(|x| x.as_str());
                let month = cap.name("month").map(|x| x.as_str());
                let day = cap.name("day").map(|x| x.as_str());
                dates.extend_from_slice(&self.get_matching_dates(year, month, day)?);
            }
        } else if ym_reg.is_match(search_text) {
            for cap in ym_reg.captures_iter(search_text) {
                let year = cap.name("year").map(|x| x.as_str());
                let month = cap.name("month").map(|x| x.as_str());
                dates.extend_from_slice(&self.get_matching_dates(year, month, None)?);
            }
        } else if y_reg.is_match(search_text) {
            for cap in y_reg.captures_iter(search_text) {
                let year = cap.name("year").map(|x| x.as_str());
                dates.extend_from_slice(&self.get_matching_dates(year, None, None)?);
            }
        }

        dates.sort();
        debug!("search dates {}", dates.len());

        if !dates.is_empty() {
            let mut de_entries = Vec::new();
            for date in dates {
                debug!("search date {}", date);
                let entry = DiaryEntries::get_by_date(date, &self.pool)?;
                let entry = format!("{}\n{}", entry.diary_date, entry.diary_text);
                de_entries.push(entry);
                let dc_entries: Vec<_> = DiaryCache::get_cache_entries(&self.pool)?
                    .into_iter()
                    .filter_map(|entry| {
                        if entry
                            .diary_datetime
                            .with_timezone(&Local)
                            .naive_local()
                            .date()
                            == date
                        {
                            Some(format!("{}\n{}", entry.diary_datetime, entry.diary_text))
                        } else {
                            None
                        }
                    })
                    .collect();
                de_entries.extend_from_slice(&dc_entries);
            }
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

    pub fn sync_everything(&self) -> Result<(), Error> {
        thread::scope(|s| {
            self.sync_ssh()?;
            if self.config.ssh_url.is_some() {
                self.sync_merge_cache_to_entries()?;
            }

            let local = s.spawn(move |_| self.local.import_from_local().map(|_| ()));
            let s3 = s.spawn(move |_| self.s3.import_from_s3().map(|_| ()));
            local.join().expect("import_from_local paniced")?;
            s3.join().expect("import_from_s3 paniced")?;

            self.local.cleanup_local()?;

            let s3 = s.spawn(move |_| self.s3.export_to_s3().map(|_| ()));
            let local = s.spawn(move |_| self.local.export_year_to_local().map(|_| ()));
            local.join().expect("import_from_local paniced")?;
            s3.join().expect("import_from_s3 paniced")?;

            Ok(())
        })
        .expect("scoped thread panic")
    }

    pub fn sync_merge_cache_to_entries(&self) -> Result<Vec<DiaryEntries>, Error> {
        let date_entry_map = DiaryCache::get_cache_entries(&self.pool)?.into_iter().fold(
            HashMap::new(),
            |mut acc, entry| {
                let entry_date = entry
                    .diary_datetime
                    .with_timezone(&Local)
                    .naive_local()
                    .date();
                acc.entry(entry_date).or_insert_with(Vec::new).push(entry);
                acc
            },
        );

        date_entry_map
            .into_par_iter()
            .map(|(entry_date, entry_list)| {
                let entry_string: Vec<_> = entry_list
                    .iter()
                    .map(|entry| {
                        let entry_datetime = entry.diary_datetime.with_timezone(&Local);
                        format!("{}\n{}", entry_datetime, entry.diary_text)
                    })
                    .collect();
                let entry_string = entry_string.join("\n\n");

                let diary_file = format!("{}/{}.txt", self.config.diary_path, entry_date);
                let result = if Path::new(&diary_file).exists() {
                    let mut f = OpenOptions::new().append(true).open(&diary_file)?;
                    writeln!(f, "\n\n{}\n\n", entry_string)?;
                    None
                } else if let Ok(mut current_entry) =
                    DiaryEntries::get_by_date(entry_date, &self.pool)
                {
                    current_entry.diary_text =
                        format!("{}\n\n{}", &current_entry.diary_text, entry_string).into();
                    println!("insert into {}", diary_file);
                    current_entry.update_entry(&self.pool)?;
                    Some(current_entry)
                } else {
                    let new_entry = DiaryEntries {
                        diary_date: entry_date,
                        diary_text: entry_string.into(),
                        last_modified: Utc::now(),
                    };
                    new_entry.upsert_entry(&self.pool)?;
                    Some(new_entry)
                };

                let res: Result<Vec<_>, Error> = entry_list
                    .into_par_iter()
                    .map(|entry| entry.delete_entry(&self.pool))
                    .collect();
                res?;

                Ok(result)
            })
            .filter_map(|x| x.transpose())
            .collect()
    }

    pub fn serialize_cache(&self) -> Result<Vec<String>, Error> {
        DiaryCache::get_cache_entries(&self.pool)?
            .into_iter()
            .map(|entry| serde_json::to_string(&entry).map_err(err_msg))
            .collect()
    }

    pub fn sync_ssh(&self) -> Result<Vec<DiaryCache>, Error> {
        if let Some(ssh_url) = self.config.ssh_url.as_ref() {
            if ssh_url.scheme() != "ssh" {
                return Ok(Vec::new());
            }
            let cache_set: HashSet<_> = DiaryCache::get_cache_entries(&self.pool)?
                .into_iter()
                .map(|entry| entry.diary_datetime)
                .collect();
            let command = "/usr/bin/diary-app-rust ser";
            let ssh_inst = SSHInstance::from_url(ssh_url)?;
            let inserted_entries: Result<Vec<_>, Error> = ssh_inst
                .run_command_stream_stdout(command)?
                .into_iter()
                .map(|line| {
                    let item: DiaryCache = serde_json::from_str(&line)?;
                    if !cache_set.contains(&item.diary_datetime) {
                        println!("{:?}", item);
                        item.insert_entry(&self.pool)?;
                        Ok(Some(item))
                    } else {
                        Ok(None)
                    }
                })
                .filter_map(|result| result.transpose())
                .collect();
            let inserted_entries = inserted_entries?;
            if !inserted_entries.is_empty() {
                let command = "/usr/bin/diary-app-rust clear";
                ssh_inst.run_command_ssh(command)?;
            }
            Ok(inserted_entries)
        } else {
            Ok(Vec::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::diary_app_interface::DiaryAppInterface;
    use crate::pgpool::PgPool;

    #[test]
    fn test_search_text() {
        let config = Config::init_config().unwrap();
        let pool = PgPool::new(&config.database_url);
        let dap = DiaryAppInterface::new(config, pool);
        let results = dap.search_text("2011-05-23").unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].starts_with("2011-05-23"));
    }
}
