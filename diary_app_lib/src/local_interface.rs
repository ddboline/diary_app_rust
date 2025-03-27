use anyhow::{Error, format_err};
use futures::{TryStreamExt, future::try_join_all, stream::FuturesUnordered};
use jwalk::WalkDir;
use log::debug;
use stack_string::{StackString, format_sstr};
use std::{
    collections::{BTreeMap, HashMap},
    fs::metadata,
    sync::Arc,
    time::SystemTime,
};
use time::{
    Date, Duration, OffsetDateTime,
    macros::{datetime, format_description},
};
use time_tz::OffsetDateTimeExt;
use tokio::{
    fs::{File, read_to_string, remove_file},
    io::AsyncWriteExt,
};

use crate::{
    config::Config, date_time_wrapper::DateTimeWrapper, models::DiaryEntries, pgpool::PgPool,
};

#[derive(Clone, Debug)]
pub struct LocalInterface {
    pub config: Config,
    pub pool: PgPool,
}

impl LocalInterface {
    #[must_use]
    pub fn new(config: Config, pool: PgPool) -> Self {
        Self { config, pool }
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn export_year_to_local(&self) -> Result<Vec<StackString>, Error> {
        let mod_map = DiaryEntries::get_modified_map(&self.pool, None, None).await?;
        let year_mod_map: BTreeMap<i32, OffsetDateTime> =
            mod_map.iter().fold(BTreeMap::new(), |mut acc, (k, v)| {
                let year = k.year();
                let current_timestamp = acc
                    .insert(year, *v)
                    .unwrap_or_else(|| datetime!(0000-01-01 00:00:00).assume_utc());
                if *v < current_timestamp {
                    acc.insert(year, current_timestamp);
                }
                acc
            });
        let year_mod_map = Arc::new(year_mod_map);
        let mut date_list: Vec<_> = mod_map.into_keys().collect();
        date_list.sort();
        let year_map: BTreeMap<i32, Vec<_>> =
            date_list.into_iter().fold(BTreeMap::new(), |mut acc, d| {
                let year = d.year();
                acc.entry(year).or_default().push(d);
                acc
            });

        let futures = year_map.into_iter().map(|(year, date_list)| {
            let year_mod_map = year_mod_map.clone();
            async move {
                let filepath = self
                    .config
                    .diary_path
                    .join(format_sstr!("diary_{year}.txt"));
                if filepath.exists() {
                    if let Ok(metadata) = filepath.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            let modified: OffsetDateTime = modified.into();
                            if let Some(maxmod) = year_mod_map.get(&year) {
                                if modified >= *maxmod {
                                    return Ok(format_sstr!("{year} 0"));
                                }
                            }
                        }
                    }
                }

                let mut f = File::create(filepath).await?;
                for date in &date_list {
                    let entry = DiaryEntries::get_by_date(*date, &self.pool)
                        .await?
                        .ok_or_else(|| format_err!("Date should exist {date}"))?;
                    let entry_text = format_sstr!("{date}\n\n{t}\n\n", t = entry.diary_text);
                    f.write_all(entry_text.as_bytes()).await?;
                }
                Ok(format_sstr!("{year} {l}", l = date_list.len()))
            }
        });
        let output: Result<Vec<_>, Error> = try_join_all(futures).await;
        let output = output?;
        debug!("{}", output.join("\n"));
        Ok(output)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn cleanup_local(&self) -> Result<Vec<DiaryEntries>, Error> {
        let local = DateTimeWrapper::local_tz();
        let existing_map = DiaryEntries::get_modified_map(&self.pool, None, None).await?;
        let previous_date = (OffsetDateTime::now_utc() - Duration::days(4))
            .to_timezone(local)
            .date();

        let futures: FuturesUnordered<_> = WalkDir::new(&self.config.diary_path)
            .sort(true)
            .into_iter()
            .map(|entry| async move {
                let entry = entry?;
                let filename = entry.file_name.to_string_lossy();
                if let Ok(date) =
                    Date::parse(&filename, format_description!("[year]-[month]-[day].txt"))
                {
                    let filepath = self.config.diary_path.join(filename.as_ref());
                    if date <= previous_date {
                        debug!("{:?}\n", filepath);
                        remove_file(&filepath).await?;
                    } else {
                        let metadata = metadata(&filepath)?;
                        let size = metadata.len() as usize;
                        let modified_secs = metadata
                            .modified()?
                            .duration_since(SystemTime::UNIX_EPOCH)?
                            .as_secs() as i64;
                        let modified = OffsetDateTime::from_unix_timestamp(modified_secs)?;
                        return Ok(Some((date, (modified, size))));
                    }
                }
                Ok(None)
            })
            .collect();
        let dates: Result<BTreeMap<_, _>, Error> = futures
            .try_filter_map(|x| async move { Ok(x) })
            .try_collect()
            .await;
        let dates = dates?;

        let current_date = OffsetDateTime::now_utc().to_timezone(local).date();

        let mut entries = Vec::new();
        for current_date in (0..4).map(|i| (current_date - Duration::days(i))) {
            if let Some((file_mod, file_size)) = dates.get(&current_date) {
                if let Some(db_mod) = existing_map.get(&current_date) {
                    if file_mod < db_mod {
                        if let Some(existing_entry) =
                            DiaryEntries::get_by_date(current_date, &self.pool).await?
                        {
                            let existing_size = existing_entry.diary_text.len();
                            if existing_size > *file_size {
                                debug!("file db diff {} {}", file_mod, db_mod);
                                debug!("file db size {} {}", file_size, db_mod);
                                let current_date_str = StackString::from_display(current_date);
                                let filepath = self
                                    .config
                                    .diary_path
                                    .join(current_date_str)
                                    .with_extension("txt");
                                let mut f = File::create(&filepath).await?;
                                f.write_all(existing_entry.diary_text.as_bytes()).await?;
                            }
                            entries.push(existing_entry);
                        }
                    }
                } else {
                    let d = DiaryEntries::new(current_date, "");
                    d.upsert_entry(&self.pool, true).await?;
                    entries.push(d);
                }
            } else {
                let current_date_str = StackString::from_display(current_date);
                let filepath = self
                    .config
                    .diary_path
                    .join(current_date_str)
                    .with_extension("txt");
                let mut f = File::create(&filepath).await?;

                if let Some(existing_entry) =
                    DiaryEntries::get_by_date(current_date, &self.pool).await?
                {
                    f.write_all(existing_entry.diary_text.as_bytes()).await?;
                    entries.push(existing_entry);
                } else {
                    f.write_all(b"").await?;
                    let new_entry = DiaryEntries::new(current_date, "");
                    new_entry.upsert_entry(&self.pool, true).await?;
                    entries.push(new_entry);
                }
            }
        }
        Ok(entries)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn import_from_local(&self) -> Result<Vec<DiaryEntries>, Error> {
        let file_dates: HashMap<Date, _> = WalkDir::new(&self.config.diary_path)
            .sort(true)
            .into_iter()
            .filter_map(|entry| {
                entry.ok().and_then(|entry| {
                    let filename = entry.file_name.to_string_lossy();
                    Date::parse(&filename, format_description!("[year]-[month]-[day].txt"))
                        .ok()
                        .and_then(|d| {
                            let metadata = entry.metadata().ok()?;
                            let modified: OffsetDateTime = metadata.modified().ok()?.into();
                            let size = metadata.len();
                            if size == 0 { None } else { Some((d, modified)) }
                        })
                })
            })
            .collect();
        let min_date = file_dates.keys().min().copied();
        let existing_map = DiaryEntries::get_modified_map(&self.pool, min_date, None).await?;
        let mut entries = Vec::new();
        for (date, modified) in file_dates {
            let filename = format_sstr!("{date}.txt");
            let filepath = self.config.diary_path.join(&filename);
            let should_modify = match existing_map.get(&date) {
                Some(current_modified) => (*current_modified - modified).whole_seconds() < -1,
                None => true,
            };
            if !should_modify {
                continue;
            }
            let diary_text: StackString = read_to_string(&filepath).await?.trim().into();
            if diary_text.is_empty() {
                continue;
            }
            let entry = DiaryEntries {
                diary_date: date,
                diary_text,
                last_modified: modified.into(),
            };
            debug!(
                "import local date {} lines {}\n",
                entry.diary_date,
                entry.diary_text.matches('\n').count()
            );
            entry.upsert_entry(&self.pool, true).await?;
            entries.push(entry);
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use jwalk::WalkDir;
    use log::debug;
    use tempdir::TempDir;

    use crate::{config::Config, local_interface::LocalInterface, pgpool::PgPool};

    fn get_tempdir() -> Result<TempDir, Error> {
        TempDir::new("test_diary").map_err(Into::into)
    }

    fn get_li(tempdir: &TempDir) -> Result<LocalInterface, Error> {
        let config = Config::get_local_config(tempdir.path())?;
        let pool = PgPool::new(&config.database_url)?;
        Ok(LocalInterface::new(config, pool))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_export_year_to_local() -> Result<(), Error> {
        let t = get_tempdir()?;
        let li = get_li(&t)?;
        let results = li.export_year_to_local().await?;
        assert!(results.contains(&"2013 296".into()));
        let nentries = results.len();
        debug!("{:?}", results);
        debug!("{:?}", t.path());
        let results: Result<Vec<_>, Error> = WalkDir::new(t.path())
            .sort(true)
            .into_iter()
            .map(|entry| {
                let entry = entry?;
                let ftype = entry.file_type;
                if ftype.is_dir() {
                    Ok(None)
                } else {
                    let filename = entry.file_name.to_string_lossy().to_string();
                    Ok(Some(filename))
                }
            })
            .filter_map(|x| x.transpose())
            .collect();
        let results = results?;
        assert!(results.len() >= 9);
        assert_eq!(results.len(), nentries);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_cleanup_local() -> Result<(), Error> {
        let t = get_tempdir()?;
        let li = get_li(&t)?;
        let results = li.cleanup_local().await?;
        let number_results = results.len();
        debug!("{:?}", results);
        let results: Result<Vec<_>, Error> = WalkDir::new(t.path())
            .sort(true)
            .into_iter()
            .map(|entry| {
                let entry = entry?;
                let ftype = entry.file_type;
                if ftype.is_dir() {
                    Ok(None)
                } else {
                    let filename = entry.file_name.to_string_lossy().to_string();
                    Ok(Some(filename))
                }
            })
            .filter_map(|x| x.transpose())
            .collect();
        let results = results?;
        debug!("{:?}", results);
        assert_eq!(results.len(), number_results);
        Ok(())
    }
}
