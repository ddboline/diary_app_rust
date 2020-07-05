use anyhow::{format_err, Error};
use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use futures::future::try_join_all;
use jwalk::WalkDir;
use log::debug;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::{
    fs::OpenOptions,
    io::AsyncWriteExt,
    task::{spawn, spawn_blocking},
};
use url::Url;

use crate::{
    config::Config,
    local_interface::LocalInterface,
    models::{DiaryCache, DiaryEntries},
    pgpool::PgPool,
    s3_interface::S3Interface,
    ssh_instance::SSHInstance,
    stack_string::StackString,
    stdout_channel::StdoutChannel,
};

#[derive(Clone)]
pub struct DiaryAppInterface {
    pub config: Config,
    pub pool: PgPool,
    pub local: LocalInterface,
    pub s3: S3Interface,
    pub stdout: StdoutChannel,
}

impl DiaryAppInterface {
    pub fn new(config: Config, pool: PgPool) -> Self {
        Self {
            local: LocalInterface::new(config.clone(), pool.clone()),
            s3: S3Interface::new(config.clone(), pool.clone()),
            pool,
            config,
            stdout: StdoutChannel::new(),
        }
    }

    pub async fn cache_text(&self, diary_text: &str) -> Result<DiaryCache, Error> {
        let dc = DiaryCache {
            diary_datetime: Utc::now(),
            diary_text: diary_text.into(),
        };
        dc.insert_entry(&self.pool).await
    }

    pub async fn replace_text(
        &self,
        diary_date: NaiveDate,
        diary_text: &str,
    ) -> Result<(DiaryEntries, Option<DateTime<Utc>>), Error> {
        let de = DiaryEntries::new(diary_date, diary_text);
        de.upsert_entry(&self.pool, true).await
    }

    pub async fn get_list_of_dates(
        &self,
        min_date: Option<NaiveDate>,
        max_date: Option<NaiveDate>,
        start: Option<usize>,
        limit: Option<usize>,
    ) -> Result<Vec<NaiveDate>, Error> {
        let mut dates: Vec<_> = DiaryEntries::get_modified_map(&self.pool)
            .await?
            .into_iter()
            .filter_map(|(d, _)| {
                if let Some(min_date) = min_date {
                    if d < min_date {
                        return None;
                    }
                }
                if let Some(max_date) = max_date {
                    if d > max_date {
                        return None;
                    }
                }
                Some(d)
            })
            .collect();
        dates.sort();
        dates.reverse();
        if let Some(start) = start {
            if start <= dates.len() {
                dates = dates.split_off(start);
            }
        }
        if let Some(limit) = limit {
            dates.truncate(limit);
        }
        Ok(dates)
    }

    fn get_matching_dates(
        mod_map: &HashMap<NaiveDate, DateTime<Utc>>,
        year: Option<i32>,
        month: Option<u32>,
        day: Option<u32>,
    ) -> Result<Vec<NaiveDate>, Error> {
        let matching_dates: Vec<_> = mod_map
            .iter()
            .map(|(d, _)| *d)
            .filter(|date| {
                if let Some(y) = year {
                    let result = if let Some(m) = month {
                        let result = if let Some(d) = day {
                            d == date.day()
                        } else {
                            true
                        };
                        result && (m == date.month())
                    } else {
                        true
                    };
                    result && (y == date.year())
                } else {
                    false
                }
            })
            .collect();
        Ok(matching_dates)
    }

    fn get_dates_from_search_text(
        mod_map: &HashMap<NaiveDate, DateTime<Utc>>,
        search_text: &str,
    ) -> Result<Vec<NaiveDate>, Error> {
        let year_month_day_regex = Regex::new(r"(?P<year>\d{4})-(?P<month>\d{2})-(?P<day>\d{2})")?;
        let year_month_regex = Regex::new(r"(?P<year>\d{4})-(?P<month>\d{2})")?;
        let year_regex = Regex::new(r"(?P<year>\d{4})")?;

        let mut dates = Vec::new();
        if search_text.trim().to_lowercase() == "today" {
            dates.push(Local::now().naive_local().date());
        }
        if year_month_day_regex.is_match(search_text) {
            for cap in year_month_day_regex.captures_iter(search_text) {
                let year: Option<i32> = cap.name("year").and_then(|x| x.as_str().parse().ok());
                let month: Option<u32> = cap.name("month").and_then(|x| x.as_str().parse().ok());
                let day: Option<u32> = cap.name("day").and_then(|x| x.as_str().parse().ok());
                dates.extend_from_slice(&Self::get_matching_dates(&mod_map, year, month, day)?);
            }
        } else if year_month_regex.is_match(search_text) {
            for cap in year_month_regex.captures_iter(search_text) {
                let year: Option<i32> = cap.name("year").and_then(|x| x.as_str().parse().ok());
                let month: Option<u32> = cap.name("month").and_then(|x| x.as_str().parse().ok());
                dates.extend_from_slice(&Self::get_matching_dates(&mod_map, year, month, None)?);
            }
        } else if year_regex.is_match(search_text) {
            for cap in year_regex.captures_iter(search_text) {
                let year: Option<i32> = cap.name("year").and_then(|x| x.as_str().parse().ok());
                dates.extend_from_slice(&Self::get_matching_dates(&mod_map, year, None, None)?);
            }
        }
        Ok(dates)
    }

    pub async fn search_text(&self, search_text: &str) -> Result<Vec<StackString>, Error> {
        let mod_map = DiaryEntries::get_modified_map(&self.pool).await?;

        let mut dates = Self::get_dates_from_search_text(&mod_map, search_text)?;

        dates.sort();
        debug!("search dates {}", dates.len());

        if dates.is_empty() {
            let mut diary_entries: Vec<_> = DiaryEntries::get_by_text(search_text, &self.pool)
                .await?
                .into_iter()
                .map(|entry| format!("{}\n{}", entry.diary_date, entry.diary_text).into())
                .collect();
            let diary_cache_entries: Vec<_> = DiaryCache::get_by_text(search_text, &self.pool)
                .await?
                .into_iter()
                .map(|entry| {
                    format!(
                        "{}\n{}",
                        entry.diary_datetime.format("%Y-%m-%dT%H:%M:%SZ"),
                        entry.diary_text
                    )
                    .into()
                })
                .collect();
            diary_entries.extend_from_slice(&diary_cache_entries);
            Ok(diary_entries)
        } else {
            let mut diary_entries = Vec::new();
            for date in dates {
                debug!("search date {}", date);
                let entry = DiaryEntries::get_by_date(date, &self.pool).await?;
                let entry = format!("{}\n{}", entry.diary_date, entry.diary_text).into();
                diary_entries.push(entry);
                let diary_cache_entries: Vec<_> = DiaryCache::get_cache_entries(&self.pool)
                    .await?
                    .into_iter()
                    .filter_map(|entry| {
                        if entry
                            .diary_datetime
                            .with_timezone(&Local)
                            .naive_local()
                            .date()
                            == date
                        {
                            Some(format!("{}\n{}", entry.diary_datetime, entry.diary_text).into())
                        } else {
                            None
                        }
                    })
                    .collect();
                diary_entries.extend_from_slice(&diary_cache_entries);
            }
            Ok(diary_entries)
        }
    }

    pub async fn sync_everything(&self) -> Result<Vec<StackString>, Error> {
        let mut output = Vec::new();
        output.extend(
            self.sync_ssh()
                .await?
                .into_iter()
                .map(|c| format!("ssh cache {}", c.diary_datetime).into()),
        );

        output.extend(
            self.sync_merge_cache_to_entries()
                .await?
                .into_iter()
                .map(|c| format!("update {}", c.diary_date).into()),
        );

        let local = spawn({
            let local = self.local.clone();
            async move { local.import_from_local().await }
        });

        let s3 = spawn({
            let s3 = self.s3.clone();
            async move { s3.import_from_s3().await }
        });
        output.extend(
            local
                .await??
                .into_iter()
                .map(|c| format!("local import {}", c.diary_date).into()),
        );
        output.extend(
            s3.await??
                .into_iter()
                .map(|c| format!("s3 import {}", c.diary_date).into()),
        );
        output.extend(
            self.local
                .cleanup_local()
                .await?
                .into_iter()
                .map(|c| format!("local cleanup {}", c.diary_date).into()),
        );
        let s3 = spawn({
            let s3 = self.s3.clone();
            async move { s3.export_to_s3().await }
        });
        let local = spawn({
            let local = self.local.clone();
            async move { local.export_year_to_local().await }
        });
        output.extend_from_slice(&local.await??);
        output.extend(
            s3.await??
                .into_iter()
                .map(|c| format!("s3 export {}", c.diary_date).into()),
        );

        Ok(output)
    }

    pub async fn sync_merge_cache_to_entries(&self) -> Result<Vec<DiaryEntries>, Error> {
        let date_entry_map = DiaryCache::get_cache_entries(&self.pool)
            .await?
            .into_iter()
            .fold(HashMap::new(), |mut acc, entry| {
                let entry_date = entry
                    .diary_datetime
                    .with_timezone(&Local)
                    .naive_local()
                    .date();
                acc.entry(entry_date).or_insert_with(Vec::new).push(entry);
                acc
            });

        let futures = date_entry_map.into_iter().map(|(entry_date, entry_list)| {
            let entry_string: Vec<_> = entry_list
                .iter()
                .map(|entry| {
                    let entry_datetime = entry.diary_datetime.with_timezone(&Local);
                    format!("{}\n{}", entry_datetime, entry.diary_text)
                })
                .collect();
            let entry_string = entry_string.join("\n\n");

            let diary_file = self.config.diary_path.join(format!("{}.txt", entry_date));

            async move {
                let result = if diary_file.exists() {
                    let mut f = OpenOptions::new().append(true).open(&diary_file).await?;
                    f.write_all(format!("\n\n{}\n\n", entry_string).as_bytes())
                        .await?;
                    None
                } else if let Ok(mut current_entry) =
                    DiaryEntries::get_by_date(entry_date, &self.pool).await
                {
                    current_entry.diary_text =
                        format!("{}\n\n{}", &current_entry.diary_text, entry_string).into();
                    self.stdout
                        .send(format!("update {}", diary_file.to_string_lossy()).into())?;
                    let (current_entry, _) = current_entry.update_entry(&self.pool, true).await?;
                    Some(current_entry)
                } else {
                    let new_entry = DiaryEntries::new(entry_date, &entry_string);
                    self.stdout
                        .send(format!("upsert {}", diary_file.to_string_lossy()).into())?;
                    let (new_entry, _) = new_entry.upsert_entry(&self.pool, true).await?;
                    Some(new_entry)
                };
                for entry in entry_list {
                    entry.delete_entry(&self.pool).await?;
                }
                Ok(result)
            }
        });
        let results: Result<Vec<_>, Error> = try_join_all(futures).await;
        let entries: Vec<_> = results?.into_iter().filter_map(|x| x).collect();
        Ok(entries)
    }

    pub async fn serialize_cache(&self) -> Result<Vec<StackString>, Error> {
        DiaryCache::get_cache_entries(&self.pool)
            .await?
            .into_iter()
            .map(|entry| {
                serde_json::to_string(&entry)
                    .map(Into::into)
                    .map_err(Into::into)
            })
            .collect()
    }

    fn process_ssh(
        ssh_url: &Url,
        cache_set: &HashSet<DateTime<Utc>>,
    ) -> Result<Vec<DiaryCache>, Error> {
        let ssh_inst = SSHInstance::from_url(ssh_url)?;
        let mut entries = Vec::new();
        for line in ssh_inst.run_command_stream_stdout("/usr/bin/diary-app-rust ser")? {
            let item: DiaryCache = serde_json::from_str(&line)?;
            if !cache_set.contains(&item.diary_datetime) {
                debug!("{:?}", item);
                entries.push(item);
            }
        }
        Ok(entries)
    }

    pub async fn sync_ssh(&self) -> Result<Vec<DiaryCache>, Error> {
        let ssh_url = match self
            .config
            .ssh_url
            .as_ref()
            .and_then(|s| s.parse::<Url>().ok())
        {
            Some(ssh_url) => Arc::new(ssh_url),
            None => return Ok(Vec::new()),
        };

        if ssh_url.scheme() != "ssh" {
            return Ok(Vec::new());
        }
        let cache_set: HashSet<_> = DiaryCache::get_cache_entries(&self.pool)
            .await?
            .into_iter()
            .map(|entry| entry.diary_datetime)
            .collect();
        let entries = {
            let ssh_url = ssh_url.clone();
            spawn_blocking(move || Self::process_ssh(&ssh_url, &cache_set)).await?
        }?;
        let futures = entries.into_iter().map(|item| {
            let pool = self.pool.clone();
            async move { item.insert_entry(&pool).await }
        });
        let results: Result<Vec<_>, Error> = try_join_all(futures).await;
        let inserted_entries: Vec<_> = results?;
        if !inserted_entries.is_empty() {
            spawn_blocking(move || {
                SSHInstance::from_url(&ssh_url)?.run_command_ssh("/usr/bin/diary-app-rust clear")
            })
            .await??;
        }
        Ok(inserted_entries)
    }

    fn get_file_date_len_map() -> Result<HashMap<NaiveDate, usize>, Error> {
        let home_dir = dirs::home_dir().ok_or_else(|| format_err!("No HOME directory"))?;
        let backup_directory = home_dir
            .join("Dropbox")
            .join("backup")
            .join("epistle_backup")
            .join("backup");
        if !backup_directory.exists() {
            return Err(format_err!("{:?} doesn't exist", backup_directory));
        }
        let files: Result<Vec<_>, Error> = WalkDir::new(backup_directory)
            .into_iter()
            .map(|entry| {
                let entry = entry?;
                let metadata = entry.metadata()?;
                Ok((entry.file_name, metadata.len() as usize))
            })
            .collect();
        let results: HashMap<_, _> = files?
            .into_par_iter()
            .filter_map(|(filename, backup_size)| {
                NaiveDate::parse_from_str(&filename.to_string_lossy(), "%Y-%m-%d.txt")
                    .ok()
                    .map(|date| (date, backup_size))
            })
            .collect();
        Ok(results)
    }

    pub async fn validate_backup(&self) -> Result<Vec<(NaiveDate, usize, usize)>, Error> {
        let file_date_len_map = spawn_blocking(Self::get_file_date_len_map).await?;
        let file_date_len_map = Arc::new(file_date_len_map?);
        println!("len file_date_len_map {}", file_date_len_map.len());

        let futures = file_date_len_map.iter().map(|(date, backup_len)| {
            let pool = self.pool.clone();
            async move {
                let entry = DiaryEntries::get_by_date(*date, &pool).await?;
                let diary_len = entry.diary_text.len();
                if diary_len == *backup_len {
                    Ok(None)
                } else {
                    Ok(Some((*date, *backup_len, diary_len)))
                }
            }
        });
        let results: Result<Vec<_>, Error> = try_join_all(futures).await;
        let results: Vec<_> = results?.into_iter().filter_map(|x| x).collect();
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use chrono::NaiveDate;
    use log::debug;

    use crate::{
        config::Config,
        diary_app_interface::DiaryAppInterface,
        models::{DiaryCache, DiaryConflict, DiaryEntries},
        pgpool::PgPool,
    };

    fn get_dap() -> Result<DiaryAppInterface, Error> {
        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url);
        Ok(DiaryAppInterface::new(config, pool))
    }

    #[tokio::test]
    #[ignore]
    async fn test_search_text() -> Result<(), Error> {
        let dap = get_dap()?;

        let results = dap.search_text("2011-05-23").await?;
        assert_eq!(results.len(), 1);
        assert!(results[0].starts_with("2011-05-23"));
        let results = dap.search_text("1952-01-01").await?;
        assert_eq!(results.len(), 0);
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_list_of_dates() -> Result<(), Error> {
        let dap = get_dap()?;

        let results = dap
            .get_list_of_dates(
                Some(NaiveDate::from_ymd(2011, 5, 23)),
                Some(NaiveDate::from_ymd(2012, 1, 1)),
                None,
                None,
            )
            .await?;
        assert_eq!(results.len(), 167);

        let results = dap
            .get_list_of_dates(
                Some(NaiveDate::from_ymd(2011, 5, 23)),
                Some(NaiveDate::from_ymd(2012, 1, 1)),
                None,
                Some(10),
            )
            .await?;
        assert_eq!(results.len(), 10);
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_matching_dates() -> Result<(), Error> {
        let dap = get_dap()?;
        let mod_map = DiaryEntries::get_modified_map(&dap.pool).await?;

        let results = DiaryAppInterface::get_matching_dates(&mod_map, Some(2011), None, None)?;
        assert_eq!(results.len(), 288);

        let results = DiaryAppInterface::get_matching_dates(&mod_map, Some(2011), Some(6), None)?;
        assert_eq!(results.len(), 23);
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_cache_text() -> Result<(), Error> {
        let dap = get_dap()?;

        let test_text = "Test text";
        let result = dap.cache_text(test_text.into()).await?;
        debug!("{}", result.diary_datetime);
        let results = DiaryCache::get_cache_entries(&dap.pool)
            .await
            .unwrap_or_else(|_| Vec::new());
        let results2 = dap.serialize_cache().await?;
        let result = result.delete_entry(&dap.pool).await?;
        assert_eq!(result.diary_text.as_str(), "Test text");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], result);
        assert!(results2[0].contains("Test text"));
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_replace_text() -> Result<(), Error> {
        let dap = get_dap()?;
        let test_date = NaiveDate::from_ymd(1950, 1, 1);
        let test_text = "Test text";

        let (result, conflict) = dap.replace_text(test_date, test_text.into()).await?;

        let test_text2 = "Test text2";
        let (result2, conflict2) = dap.replace_text(test_date, test_text2.into()).await?;

        let result = result.delete_entry(&dap.pool).await?;

        assert_eq!(result.diary_date, test_date);
        assert!(conflict.is_none());
        assert_eq!(result2.diary_date, test_date);
        assert_eq!(result2.diary_text.as_str(), test_text2);
        assert!(conflict2.is_some());
        let conflict2 = conflict2.unwrap();
        let result3 = DiaryConflict::get_by_datetime(conflict2, &dap.pool).await?;
        assert_eq!(result3.len(), 2);
        DiaryConflict::remove_by_datetime(conflict2, &dap.pool).await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_validate_backup() -> Result<(), Error> {
        let dap = get_dap()?;
        let results = dap.validate_backup().await?;
        for (date, backup_len, diary_len) in results.iter() {
            println!(
                "date {} backup_len {} diary_len {}",
                date, backup_len, diary_len
            );
        }
        assert!(results.is_empty());
        Ok(())
    }
}
