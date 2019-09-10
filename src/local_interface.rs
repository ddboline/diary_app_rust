use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};
use failure::Error;
use jwalk::WalkDir;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::collections::{BTreeMap, HashSet};
use std::fs::{read_to_string, remove_file, File};
use std::io::{stdout, Write};

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
        LocalInterface { pool, config }
    }

    pub fn export_year_to_local(&self) -> Result<(), Error> {
        let mut date_list: Vec<_> = DiaryEntries::get_modified_map(&self.pool)?
            .into_iter()
            .map(|(k, _)| k)
            .collect();
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
                let mut f = File::create(fname)?;
                for date in &date_list {
                    let entries = DiaryEntries::get_by_date(*date, &self.pool)?;
                    if !entries.is_empty() {
                        writeln!(f, "{}\n", date)?;
                        for entry in entries {
                            writeln!(f, "{}\n", entry.diary_text)?;
                        }
                    }
                }
                Ok(format!("{} {}", year, date_list.len()))
            })
            .collect();
        writeln!(stdout().lock(), "{}", results?.join("\n"))?;
        Ok(())
    }

    pub fn cleanup_local(&self) -> Result<Vec<DiaryEntries>, Error> {
        let mut new_entries = Vec::new();
        let stdout = stdout();
        let dates: Result<HashSet<_>, Error> = WalkDir::new(&self.config.diary_path)
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
                        return Ok(None);
                    }
                    return Ok(Some(date));
                }
                Ok(None)
            })
            .filter_map(|x| x.transpose())
            .collect();
        let dates = dates?;
        let current_date = Local::now().naive_local().date();
        if !dates.contains(&current_date) {
            let existing_entries = DiaryEntries::get_by_date(current_date, &self.pool)?;

            let filepath = format!("{}/{}.txt", self.config.diary_path, current_date);

            let mut f = File::create(&filepath)?;
            if existing_entries.is_empty() {
                writeln!(f)?;
                let d = DiaryEntries {
                    diary_date: current_date,
                    diary_text: "".into(),
                    last_modified: Utc::now(),
                };
                d.insert_entry(&self.pool)?;
                new_entries.push(d);
            } else {
                for entry in existing_entries {
                    writeln!(f, "{}", &entry.diary_text)?;
                    new_entries.push(entry);
                }
            }
        }
        Ok(new_entries)
    }

    pub fn import_from_local(&self) -> Result<Vec<DiaryEntries>, Error> {
        let stdout = stdout();
        let existing_map = DiaryEntries::get_modified_map(&self.pool)?;

        WalkDir::new(&self.config.diary_path)
            .sort(true)
            .preload_metadata(true)
            .into_iter()
            .map(|entry| {
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
            })
            .filter_map(|d| d.transpose())
            .map(|result| {
                result.and_then(|entry| {
                    writeln!(
                        stdout.lock(),
                        "import local date {} lines {}",
                        entry.diary_date,
                        entry.diary_text.match_indices('\n').count()
                    )?;
                    if existing_map.contains_key(&entry.diary_date) {
                        entry.update_entry(&self.pool)?;
                    } else {
                        entry.insert_entry(&self.pool)?;
                    }
                    Ok(entry)
                })
            })
            .collect()
    }
}
