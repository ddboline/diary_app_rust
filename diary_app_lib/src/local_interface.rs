use anyhow::Error;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use jwalk::WalkDir;
use log::debug;
use std::collections::BTreeMap;
use std::fs::metadata;
use std::path::Path;
use std::time::SystemTime;
use tokio::fs::{read_to_string, remove_file, File};
use tokio::io::{stdout, AsyncWriteExt};

use crate::config::Config;
use crate::models::DiaryEntries;
use crate::pgpool::PgPool;

#[derive(Clone, Debug)]
pub struct LocalInterface {
    pub config: Config,
    pub pool: PgPool,
}

impl LocalInterface {
    pub fn new(config: Config, pool: PgPool) -> Self {
        Self { pool, config }
    }

    pub async fn export_year_to_local(&self) -> Result<Vec<String>, Error> {
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
        let mut date_list: Vec<_> = mod_map.into_iter().map(|(k, _)| k).collect();
        date_list.sort();
        let year_map: BTreeMap<i32, Vec<_>> =
            date_list.into_iter().fold(BTreeMap::new(), |mut acc, d| {
                let year = d.year();
                acc.entry(year).or_insert_with(Vec::new).push(d);
                acc
            });
        let mut output = Vec::new();
        for (year, date_list) in year_map {
            let fname = format!("{}/diary_{}.txt", &self.config.diary_path, year);

            let filepath = Path::new(&fname);
            if filepath.exists() {
                if let Ok(metadata) = filepath.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        let modified: DateTime<Utc> = modified.into();
                        if let Some(maxmod) = year_mod_map.get(&year) {
                            if modified >= *maxmod {
                                output.push(format!("{} 0", year));
                                continue;
                            }
                        }
                    }
                }
            }

            let mut f = File::create(fname).await?;
            for date in &date_list {
                let entry = DiaryEntries::get_by_date(*date, &self.pool).await?;
                f.write_all(format!("{}\n", entry.diary_text).as_bytes())
                    .await?;
            }
            output.push(format!("{} {}", year, date_list.len()));
        }
        debug!("{}", output.join("\n"));
        Ok(output)
    }

    pub async fn cleanup_local(&self) -> Result<Vec<DiaryEntries>, Error> {
        let mut stdout = stdout();
        let existing_map = DiaryEntries::get_modified_map(&self.pool).await?;

        let mut dates = BTreeMap::new();

        for entry in WalkDir::new(&self.config.diary_path)
            .sort(true)
            .preload_metadata(true)
        {
            let entry = entry?;
            let filename = entry.file_name.to_string_lossy();
            if let Ok(date) = NaiveDate::parse_from_str(&filename, "%Y-%m-%d.txt") {
                let previous_date = (Local::now() - Duration::days(4)).naive_local().date();

                if date <= previous_date {
                    let filepath = format!("{}/{}", self.config.diary_path, filename);
                    stdout.write_all(format!("{}\n", filepath).as_bytes()).await?;
                    remove_file(&filepath).await?;
                } else {
                    let filepath = format!("{}/{}", self.config.diary_path, filename);
                    let metadata = metadata(&filepath)?;
                    let size = metadata.len() as usize;
                    let modified_secs = metadata
                        .modified()?
                        .duration_since(SystemTime::UNIX_EPOCH)?
                        .as_secs() as i64;
                    let modified = Utc.timestamp(modified_secs, 0);
                    dates.insert(date, (modified, size));
                }
            }
        }

        let current_date = Local::now().naive_local().date();

        let mut entries = Vec::new();
        for current_date in (0..4).map(|i| (current_date - Duration::days(i))) {
            if let Some((file_mod, file_size)) = dates.get(&current_date) {
                if let Some(db_mod) = existing_map.get(&current_date) {
                    if file_mod < db_mod {
                        if let Ok(existing_entry) =
                            DiaryEntries::get_by_date(current_date, &self.pool).await
                        {
                            let existing_size = existing_entry.diary_text.len();
                            if existing_size > *file_size {
                                debug!("file db diff {} {}", file_mod, db_mod);
                                debug!("file db size {} {}", file_size, db_mod);
                                let filepath =
                                    format!("{}/{}.txt", self.config.diary_path, current_date);
                                let mut f = File::create(&filepath).await?;
                                f.write_all(format!("{}\n", existing_entry.diary_text).as_bytes())
                                    .await?;
                            }
                            entries.push(existing_entry);
                        }
                    }
                } else {
                    let d = DiaryEntries::new(current_date, "");
                    let (d, _) = d.upsert_entry(&self.pool).await?;
                    entries.push(d);
                }
            } else {
                let filepath = format!("{}/{}.txt", self.config.diary_path, current_date);
                let mut f = File::create(&filepath).await?;

                if let Ok(existing_entry) =
                    DiaryEntries::get_by_date(current_date, &self.pool).await
                {
                    f.write_all(format!("{}\n", existing_entry.diary_text).as_bytes())
                        .await?;
                    entries.push(existing_entry)
                } else {
                    f.write_all(b"\n").await?;
                    let d = DiaryEntries::new(current_date, "");
                    let (d, _) = d.upsert_entry(&self.pool).await?;
                    entries.push(d);
                }
            }
        }
        Ok(entries)
    }

    pub async fn import_from_local(&self) -> Result<Vec<DiaryEntries>, Error> {
        let mut stdout = stdout();
        let existing_map = DiaryEntries::get_modified_map(&self.pool).await?;

        let mut entries = Vec::new();
        for entry in WalkDir::new(&self.config.diary_path)
            .sort(true)
            .preload_metadata(true)
        {
            let entry = entry?;
            let filename = entry.file_name.to_string_lossy();
            let mut new_entry = None;
            if let Ok(date) = NaiveDate::parse_from_str(&filename, "%Y-%m-%d.txt") {
                if let Some(metadata) = entry.metadata.transpose()? {
                    let filepath = format!("{}/{}", self.config.diary_path, filename);
                    let modified: DateTime<Utc> = metadata.modified()?.into();

                    let should_modify = match existing_map.get(&date) {
                        Some(current_modified) => (*current_modified - modified).num_seconds() < -1,
                        None => true,
                    };

                    if metadata.len() > 0 && should_modify {
                        let d = DiaryEntries {
                            diary_date: date,
                            diary_text: read_to_string(&filepath).await?,
                            last_modified: modified,
                        };
                        new_entry = Some(d);
                    }
                }
            }
            let entry = match new_entry {
                Some(x) => x,
                None => continue,
            };

            let entry = if entry.diary_text.trim().is_empty() {
                entry
            } else {
                stdout
                    .write_all(
                        format!(
                            "import local date {} lines {}\n",
                            entry.diary_date,
                            entry.diary_text.match_indices('\n').count()
                        )
                        .as_bytes(),
                    )
                    .await?;
                if existing_map.contains_key(&entry.diary_date) {
                    entry.update_entry(&self.pool).await?.0
                } else {
                    entry.upsert_entry(&self.pool).await?.0
                }
            };
            entries.push(entry)
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use jwalk::WalkDir;
    use std::io::{stdout, Write};
    use tempdir::TempDir;

    use crate::config::{Config, ConfigInner};
    use crate::local_interface::LocalInterface;
    use crate::pgpool::PgPool;

    fn get_tempdir() -> Result<TempDir, Error> {
        TempDir::new("test_diary").map_err(Into::into)
    }

    fn get_li(tempdir: &TempDir) -> Result<LocalInterface, Error> {
        let config = Config::init_config()?.get_inner()?;
        let inner = ConfigInner {
            diary_path: tempdir.path().to_string_lossy().to_string(),
            ssh_url: None,
            ..config
        };
        let config = Config::from_inner(inner);

        let pool = PgPool::new(&config.database_url);
        Ok(LocalInterface::new(config, pool))
    }

    #[tokio::test]
    #[ignore]
    async fn test_export_year_to_local() -> Result<(), Error> {
        let t = get_tempdir()?;
        let li = get_li(&t)?;
        let results = li.export_year_to_local().await?;
        assert!(results.contains(&"2013 296".to_string()));
        let nentries = results.len();
        writeln!(stdout(), "{:?}", results)?;
        writeln!(stdout(), "{:?}", t.path())?;
        let results: Result<Vec<_>, Error> = WalkDir::new(t.path())
            .sort(true)
            .preload_metadata(true)
            .into_iter()
            .map(|entry| {
                let entry = entry?;
                let ftype = entry.file_type?;
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

    #[tokio::test]
    #[ignore]
    async fn test_cleanup_local() -> Result<(), Error> {
        let t = get_tempdir()?;
        let li = get_li(&t)?;
        let results = li.cleanup_local().await?;
        let number_results = results.len();
        writeln!(stdout(), "{:?}", results)?;
        let results: Result<Vec<_>, Error> = WalkDir::new(t.path())
            .sort(true)
            .preload_metadata(true)
            .into_iter()
            .map(|entry| {
                let entry = entry?;
                let ftype = entry.file_type?;
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
        writeln!(stdout(), "{:?}", results)?;
        assert_eq!(results.len(), number_results);
        Ok(())
    }
}
