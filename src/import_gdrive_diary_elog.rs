use anyhow::{format_err, Error};
use chrono::NaiveDate;
use std::collections::HashSet;
use std::fs::read_to_string;
use std::path::Path;

use diary_app_lib::config::Config;
use diary_app_lib::models::DiaryEntries;
use diary_app_lib::pgpool::PgPool;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let config = Config::init_config()?;
    let pool = PgPool::new(&config.database_url);
    let home_dir = dirs::home_dir().ok_or_else(|| format_err!("No HOME directory"))?;
    let diary_dir = home_dir.join("tmp").join("gdrive_diary_parsed");
    let elog_dir = home_dir.join("tmp").join("gdrive_elog_parsed");

    let diary_files = extract_dates(&diary_dir)?;
    let elog_files = extract_dates(&elog_dir)?;

    for date in diary_files.union(&elog_files) {
        let mut original_text = String::new();
        let mut original_length = None;
        let mut diary_text = String::new();
        let mut diary_length = None;
        let mut elog_text = String::new();
        let mut elog_length = None;

        if let Ok(entry) = DiaryEntries::get_by_date(*date, &pool).await {
            original_length.replace(entry.diary_text.len());
            original_text = entry.diary_text.to_string();
        }
        if diary_files.contains(&date) {
            let diary_path = diary_dir.join(date.to_string()).with_extension("txt");
            let new_text = read_to_string(&diary_path)?;

            if !original_text.contains(new_text.trim()) {
                diary_length.replace(new_text.len());
                diary_text = new_text;
            }
        }
        if elog_files.contains(&date) {
            let elog_path = elog_dir.join(date.to_string()).with_extension("txt");
            let new_text = read_to_string(&elog_path)?;

            if !original_text.contains(new_text.trim()) {
                elog_length.replace(new_text.len());
                elog_text = new_text;
            }
        }

        if original_text.is_empty() || diary_length > Some(0) || elog_length > Some(0) {
            println!(
                "date {} {} {} {}",
                date,
                original_length.unwrap_or(0),
                diary_length.unwrap_or(0),
                elog_length.unwrap_or(0),
            );

            let diary_text = [original_text, diary_text, elog_text].join("\n\n");
            let diary_entry = DiaryEntries::new(*date, &diary_text);
            diary_entry.upsert_entry(&pool, true).await?;
        }
    }

    Ok(())
}

fn extract_dates(path: &Path) -> Result<HashSet<NaiveDate>, Error> {
    path.read_dir()?
        .map(|entry| {
            if let Some(filename) = entry?.path().file_name() {
                let filename = filename.to_string_lossy().replace(".txt", "");
                let date: NaiveDate = filename.parse()?;
                Ok(Some(date))
            } else {
                Ok(None)
            }
        })
        .filter_map(Result::transpose)
        .collect()
}
