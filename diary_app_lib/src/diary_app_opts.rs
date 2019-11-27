use chrono::{DateTime, NaiveDate, Utc};
use failure::{err_msg, Error};
use std::collections::BTreeSet;
use std::io::{stdout, Stdout, Write};
use std::str::FromStr;
use structopt::StructOpt;

use crate::config::Config;
use crate::diary_app_interface::DiaryAppInterface;
use crate::models::{DiaryCache, DiaryConflict};
use crate::pgpool::PgPool;

#[derive(Debug, Clone, Copy)]
pub enum DiaryAppCommands {
    Search,
    Insert,
    Sync,
    Serialize,
    ClearCache,
    ListConflicts,
    ShowConflict,
    RemoveConflict,
}

impl FromStr for DiaryAppCommands {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "search" | "s" => Ok(DiaryAppCommands::Search),
            "insert" | "i" => Ok(DiaryAppCommands::Insert),
            "sync" => Ok(DiaryAppCommands::Sync),
            "ser" | "serialize" => Ok(DiaryAppCommands::Serialize),
            "clear" | "clear_cache" => Ok(DiaryAppCommands::ClearCache),
            "list" | "list_conflicts" => Ok(DiaryAppCommands::ListConflicts),
            "show" | "show_conflict" => Ok(DiaryAppCommands::ShowConflict),
            "remove" | "remove_conflict" => Ok(DiaryAppCommands::RemoveConflict),
            _ => Err(err_msg("Parse failure")),
        }
    }
}

#[derive(StructOpt, Debug, Clone)]
pub struct DiaryAppOpts {
    #[structopt(parse(try_from_str))]
    /// Available commands are "(s)earch", "(i)nsert", "sync", "serialize, "clear", "clear_cache",
    /// "list", "list_conflicts", "show", "show_conflict", "remove", "remove_conflict"
    pub command: DiaryAppCommands,
    #[structopt(
        short = "t",
        long = "text",
        required_if("command", "search"),
        required_if("command", "insert")
    )]
    pub text: Vec<String>,
}

impl DiaryAppOpts {
    pub fn process_args() -> Result<(), Error> {
        let stdout = stdout();
        let opts = DiaryAppOpts::from_args();

        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url);
        let dap = DiaryAppInterface::new(config, pool);

        match opts.command {
            DiaryAppCommands::Search => {
                let result = dap.search_text(&opts.text.join(" "))?;
                writeln!(stdout.lock(), "{}", result.join("\n"))?;
            }
            DiaryAppCommands::Insert => {
                dap.cache_text(opts.text.join(" ").into())?;
            }
            DiaryAppCommands::Sync => {
                dap.sync_everything()?;
            }
            DiaryAppCommands::Serialize => {
                for entry in dap.serialize_cache()? {
                    writeln!(stdout.lock(), "{}", entry)?;
                }
            }
            DiaryAppCommands::ClearCache => {
                for entry in DiaryCache::get_cache_entries(&dap.pool)? {
                    writeln!(stdout.lock(), "{}", serde_json::to_string(&entry)?)?;
                    entry.delete_entry(&dap.pool)?;
                }
            }
            DiaryAppCommands::ListConflicts => {
                fn _get_all_conflicts(
                    date: NaiveDate,
                    pool: &PgPool,
                    stdout: &Stdout,
                ) -> Result<(), Error> {
                    let conflicts: BTreeSet<_> = DiaryConflict::get_by_date(date, pool)?
                        .into_iter()
                        .map(|entry| entry.format("%Y-%m-%dT%H:%M:%S%.fZ").to_string())
                        .collect();
                    for timestamp in conflicts {
                        writeln!(stdout.lock(), "{}", timestamp)?;
                    }
                    Ok(())
                }

                if let Ok(date) = opts.text.join("").parse() {
                    _get_all_conflicts(date, &dap.pool, &stdout)?;
                } else {
                    let mut conflicts = DiaryConflict::get_all_dates(&dap.pool)?;
                    conflicts.sort();
                    if conflicts.len() > 1 {
                        for date in conflicts {
                            writeln!(stdout.lock(), "{}", date)?;
                        }
                    } else {
                        for date in conflicts {
                            _get_all_conflicts(date, &dap.pool, &stdout)?;
                        }
                    }
                }
            }
            DiaryAppCommands::ShowConflict => {
                if let Ok(datetime) =
                    DateTime::parse_from_rfc3339(&opts.text.join("").replace("Z", "+00:00"))
                        .map(|x| x.with_timezone(&Utc))
                {
                    println!("datetime {}", datetime);
                    let conflicts: Vec<_> = DiaryConflict::get_by_datetime(datetime, &dap.pool)?
                        .into_iter()
                        .map(|entry| match entry.diff_type.as_ref() {
                            "rem" => format!("\x1b[91m{}\x1b[0m", entry.diff_text),
                            "add" => format!("\x1b[92m{}\x1b[0m", entry.diff_text),
                            _ => format!("{}", entry.diff_text),
                        })
                        .collect();
                    for timestamp in conflicts {
                        writeln!(stdout.lock(), "{}", timestamp)?;
                    }
                }
            }
            DiaryAppCommands::RemoveConflict => {
                if let Ok(datetime) =
                    DateTime::parse_from_rfc3339(&opts.text.join("").replace("Z", "+00:00"))
                        .map(|x| x.with_timezone(&Utc))
                {
                    DiaryConflict::remove_by_datetime(datetime, &dap.pool)?;
                }
            }
        }
        Ok(())
    }
}
