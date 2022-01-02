use anyhow::{format_err, Error};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use futures::future::try_join_all;
use jwalk::WalkDir;
use log::debug;
use stack_string::StackString;
use std::{collections::BTreeMap, fs::metadata, sync::Arc, time::SystemTime};
use tokio::{
    fs::{read_to_string, remove_file, File},
    io::AsyncWriteExt,
};

use crate::{config::Config, models::DiaryEntries, pgpool::PgPool};

#[derive(Clone, Debug)]
pub struct LocalInterface {
    pub config: Config,
    pub pool: PgPool,
}

impl LocalInterface {
    pub fn new(config: Config, pool: PgPool) -> Self {
        Self { config, pool }
    }

    pub async fn export_year_to_local(&self) -> Result<Vec<StackString>, Error> {
        let mod_map = DiaryEntries::get_modified_map(&self.pool).await?;
        let year_mod_map: BTreeMap<i32, DateTime<Utc>> =
            mod_map.iter().fold(BTreeMap::new(), |mut acc, (k, v)| {
                let year = k.year();
                let current_timestamp = acc
                    .insert(year, *v)
                    .unwrap_or_else(|| Utc.ymd(0, 1, 1).and_hms(0, 0, 0));
                if *v < current_timestamp {
                    acc.insert(year, current_timestamp);
                }
                acc
            });
        let year_mod_map = Arc::new(year_mod_map);
        let mut date_list: Vec<_> = mod_map.into_iter().map(|(k, _)| k).collect();
        date_list.sort();
        let year_map: BTreeMap<i32, Vec<_>> =
            date_list.into_iter().fold(BTreeMap::new(), |mut acc, d| {
                let year = d.year();
                acc.entry(year).or_insert_with(Vec::new).push(d);
                acc
            });

        let futures = year_map.into_iter().map(|(year, date_list)| {
            let year_mod_map = year_mod_map.clone();
            async move {
                let filepath = self.config.diary_path.join(format!("diary_{}.txt", year));
                if filepath.exists() {
                    if let Ok(metadata) = filepath.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            let modified: DateTime<Utc> = modified.into();
                            if let Some(maxmod) = year_mod_map.get(&year) {
                                if modified >= *maxmod {
                                    return Ok(format!("{} 0", year).into());
                                }
                            }
                        }
                    }
                }

                let mut f = File::create(filepath).await?;
                for date in &date_list {
                    let entry = DiaryEntries::get_by_date(*date, &self.pool)
                        .await?
                        .ok_or_else(|| format_err!("Date should exist {}", date))?;
                    f.write_all(format!("{}\n\n{}\n\n", date, entry.diary_text).as_bytes())
                        .await?;
                }
                Ok(format!("{} {}", year, date_list.len()).into())
            }
        });
        let output: Result<Vec<_>, Error> = try_join_all(futures).await;
        let output = output?;
        debug!("{}", output.join("\n"));
        Ok(output)
    }

    pub async fn cleanup_local(&self) -> Result<Vec<DiaryEntries>, Error> {
        let existing_map = DiaryEntries::get_modified_map(&self.pool).await?;
        let previous_date = (Local::now() - Duration::days(4)).naive_local().date();

        let futures = WalkDir::new(&self.config.diary_path)
            .sort(true)
            .into_iter()
            .map(|entry| async move {
                let entry = entry?;
                let filename = entry.file_name.to_string_lossy();
                if let Ok(date) = NaiveDate::parse_from_str(&filename, "%Y-%m-%d.txt") {
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
                        let modified = Utc.timestamp(modified_secs, 0);
                        return Ok(Some((date, (modified, size))));
                    }
                }
                Ok(None)
            });
        let results: Result<Vec<_>, Error> = try_join_all(futures).await;
        let dates: BTreeMap<_, _> = results?.into_iter().flatten().collect();

        let current_date = Local::now().naive_local().date();

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
                                let current_date_str = StackString::from_display(current_date)?;
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
                let current_date_str = StackString::from_display(current_date)?;
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

    pub async fn import_from_local(&self) -> Result<Vec<DiaryEntries>, Error> {
        let existing_map = DiaryEntries::get_modified_map(&self.pool).await?;
        let mut entries = Vec::new();
        for entry in WalkDir::new(&self.config.diary_path).sort(true) {
            let entry = entry?;
            let filename = entry.file_name.to_string_lossy();
            let entry = if let Ok(date) = NaiveDate::parse_from_str(&filename, "%Y-%m-%d.txt") {
                if let Ok(metadata) = entry.metadata() {
                    let filepath = self.config.diary_path.join(filename.as_ref());
                    let modified: DateTime<Utc> = metadata.modified()?.into();
                    let should_modify = match existing_map.get(&date) {
                        Some(current_modified) => (*current_modified - modified).num_seconds() < -1,
                        None => true,
                    };

                    if metadata.len() > 0 && should_modify {
                        DiaryEntries {
                            diary_date: date,
                            diary_text: read_to_string(&filepath).await?.into(),
                            last_modified: modified,
                        }
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            } else {
                continue;
            };

            if !entry.diary_text.trim().is_empty() {
                debug!(
                    "import local date {} lines {}\n",
                    entry.diary_date,
                    entry.diary_text.matches('\n').count()
                );
                entry.upsert_entry(&self.pool, true).await?;
            };
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

    use crate::{
        config::{Config, ConfigInner},
        local_interface::LocalInterface,
        pgpool::PgPool,
    };

    fn get_tempdir() -> Result<TempDir, Error> {
        TempDir::new("test_diary").map_err(Into::into)
    }

    fn get_li(tempdir: &TempDir) -> Result<LocalInterface, Error> {
        let config = Config::init_config()?.get_inner()?;
        let inner = ConfigInner {
            diary_path: tempdir.path().to_path_buf(),
            ssh_url: None,
            ..config
        };
        let config = Config::from_inner(inner);

        let pool = PgPool::new(&config.database_url);
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
