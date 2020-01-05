use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use failure::Error;
use jwalk::WalkDir;
use log::debug;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::collections::BTreeMap;
use std::fs::{metadata, read_to_string, remove_file, File};
use std::io::{stdout, Write};
use std::path::Path;
use std::time::SystemTime;

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

    pub fn export_year_to_local(&self) -> Result<Vec<String>, Error> {
        let mod_map = DiaryEntries::get_modified_map(&self.pool)?;
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
        let results: Result<Vec<_>, Error> = year_map
            .into_par_iter()
            .map(|(year, date_list)| {
                let fname = format!("{}/diary_{}.txt", &self.config.diary_path, year);

                let filepath = Path::new(&fname);
                if filepath.exists() {
                    if let Ok(metadata) = filepath.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            let modified: DateTime<Utc> = modified.into();
                            if let Some(maxmod) = year_mod_map.get(&year) {
                                if modified >= *maxmod {
                                    return Ok(format!("{} 0", year));
                                }
                            }
                        }
                    }
                }

                let mut f = File::create(fname)?;
                for date in &date_list {
                    let entry = DiaryEntries::get_by_date(*date, &self.pool)?;
                    writeln!(f, "{}\n", entry.diary_text)?;
                }
                Ok(format!("{} {}", year, date_list.len()))
            })
            .collect();
        let results = results?;
        writeln!(stdout().lock(), "{}", results.join("\n"))?;
        Ok(results)
    }

    pub fn cleanup_local(&self) -> Result<Vec<DiaryEntries>, Error> {
        let stdout = stdout();
        let existing_map = DiaryEntries::get_modified_map(&self.pool)?;

        let dates: Result<BTreeMap<_, _>, Error> = WalkDir::new(&self.config.diary_path)
            .sort(true)
            .preload_metadata(true)
            .into_iter()
            .map(|entry| {
                let entry = entry?;
                let filename = entry.file_name.to_string_lossy();
                if let Ok(date) = NaiveDate::parse_from_str(&filename, "%Y-%m-%d.txt") {
                    let previous_date = (Local::now() - Duration::days(4)).naive_local().date();

                    if date <= previous_date {
                        let filepath = format!("{}/{}", self.config.diary_path, filename);
                        writeln!(stdout.lock(), "{}", filepath)?;
                        remove_file(&filepath)?;
                        Ok(None)
                    } else {
                        let filepath = format!("{}/{}", self.config.diary_path, filename);
                        let metadata = metadata(&filepath)?;
                        let size = metadata.len() as usize;
                        let modified_secs = metadata
                            .modified()?
                            .duration_since(SystemTime::UNIX_EPOCH)?
                            .as_secs() as i64;
                        let modified = Utc.timestamp(modified_secs, 0);
                        Ok(Some((date, (modified, size))))
                    }
                } else {
                    Ok(None)
                }
            })
            .filter_map(Result::transpose)
            .collect();
        let dates = dates?;
        let current_date = Local::now().naive_local().date();

        (0..4)
            .map(|i| (current_date - Duration::days(i)))
            .map(|current_date| {
                if let Some((file_mod, file_size)) = dates.get(&current_date) {
                    if let Some(db_mod) = existing_map.get(&current_date) {
                        if file_mod < db_mod {
                            if let Ok(existing_entry) =
                                DiaryEntries::get_by_date(current_date, &self.pool)
                            {
                                let existing_size = existing_entry.diary_text.len();
                                if existing_size > *file_size {
                                    debug!("file db diff {} {}", file_mod, db_mod);
                                    debug!("file db size {} {}", file_size, db_mod);
                                    let filepath =
                                        format!("{}/{}.txt", self.config.diary_path, current_date);
                                    let mut f = File::create(&filepath)?;
                                    writeln!(f, "{}", &existing_entry.diary_text)?;
                                }
                                return Ok(Some(existing_entry));
                            }
                        }
                        Ok(None)
                    } else {
                        let d = DiaryEntries::new(current_date, "".into());
                        d.upsert_entry(&self.pool)?;
                        Ok(Some(d))
                    }
                } else {
                    let filepath = format!("{}/{}.txt", self.config.diary_path, current_date);
                    let mut f = File::create(&filepath)?;

                    if let Ok(existing_entry) = DiaryEntries::get_by_date(current_date, &self.pool)
                    {
                        writeln!(f, "{}", &existing_entry.diary_text)?;
                        Ok(Some(existing_entry))
                    } else {
                        writeln!(f)?;
                        let d = DiaryEntries::new(current_date, "".into());
                        d.upsert_entry(&self.pool)?;
                        Ok(Some(d))
                    }
                }
            })
            .filter_map(Result::transpose)
            .collect()
    }

    pub fn import_from_local(&self) -> Result<Vec<DiaryEntries>, Error> {
        let stdout = stdout();
        let existing_map = DiaryEntries::get_modified_map(&self.pool)?;

        WalkDir::new(&self.config.diary_path)
            .sort(true)
            .preload_metadata(true)
            .into_iter()
            .filter_map(|entry| {
                let res = || {
                    let entry = entry?;
                    let filename = entry.file_name.to_string_lossy();
                    if let Ok(date) = NaiveDate::parse_from_str(&filename, "%Y-%m-%d.txt") {
                        if let Some(metadata) = entry.metadata.transpose()? {
                            let filepath = format!("{}/{}", self.config.diary_path, filename);
                            let modified: DateTime<Utc> = metadata.modified()?.into();

                            let should_modify = match existing_map.get(&date) {
                                Some(current_modified) => {
                                    (*current_modified - modified).num_seconds() < -1
                                }
                                None => true,
                            };

                            if metadata.len() > 0 && should_modify {
                                let d = DiaryEntries {
                                    diary_date: date,
                                    diary_text: read_to_string(&filepath)?.into(),
                                    last_modified: modified,
                                };
                                return Ok(Some(d));
                            }
                        }
                    }
                    Ok(None)
                };
                res().transpose().map(|result| {
                    result.and_then(|entry| {
                        if !entry.diary_text.trim().is_empty() {
                            writeln!(
                                stdout.lock(),
                                "import local date {} lines {}",
                                entry.diary_date,
                                entry.diary_text.match_indices('\n').count()
                            )?;
                            if existing_map.contains_key(&entry.diary_date) {
                                entry.update_entry(&self.pool)?;
                            } else {
                                entry.upsert_entry(&self.pool)?;
                            }
                        }
                        Ok(entry)
                    })
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use failure::Error;
    use jwalk::WalkDir;
    use tempdir::TempDir;
    use std::io::{stdout, Write};

    use crate::config::{Config, ConfigInner};
    use crate::local_interface::LocalInterface;
    use crate::pgpool::PgPool;

    fn get_tempdir() -> TempDir {
        TempDir::new("test_diary").unwrap()
    }

    fn get_li(tempdir: &TempDir) -> LocalInterface {
        let config = Config::init_config().unwrap().get_inner().unwrap();
        let inner = ConfigInner {
            diary_path: tempdir.path().to_string_lossy().to_string(),
            ssh_url: None,
            ..config
        };
        let config = Config::from_inner(inner);

        let pool = PgPool::new(&config.database_url);
        LocalInterface::new(config, pool)
    }

    #[test]
    #[ignore]
    fn test_export_year_to_local() {
        let t = get_tempdir();
        let li = get_li(&t);
        let results = li.export_year_to_local().unwrap();
        assert!(results.contains(&"2013 296".to_string()));
        let nentries = results.len();
        writeln!(stdout(),"{:?}", results).unwrap();
        writeln!(stdout(),"{:?}", t.path()).unwrap();
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
        let results = results.unwrap();
        assert!(results.len() >= 9);
        assert_eq!(results.len(), nentries);
    }

    #[test]
    #[ignore]
    fn test_cleanup_local() {
        let t = get_tempdir();
        let li = get_li(&t);
        let results = li.cleanup_local().unwrap();
        let nresults = results.len();
        writeln!(stdout(),"{:?}", results).unwrap();
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
        let results = results.unwrap();
        writeln!(stdout(),"{:?}", results).unwrap();
        assert_eq!(results.len(), nresults);
    }
}
