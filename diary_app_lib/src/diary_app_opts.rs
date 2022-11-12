use anyhow::{format_err, Error};
use clap::Parser;
use futures::TryStreamExt;
use refinery::embed_migrations;
use stack_string::StackString;
use std::{collections::BTreeSet, str::FromStr};
use time::{
    format_description::well_known::Rfc3339, macros::format_description, Date, OffsetDateTime,
};
use time_tz::{timezones::db::UTC, OffsetDateTimeExt};

use crate::{
    config::Config,
    diary_app_interface::DiaryAppInterface,
    models::{DiaryCache, DiaryConflict},
    pgpool::PgPool,
};

embed_migrations!("../migrations");

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
    RunMigrations,
}

impl FromStr for DiaryAppCommands {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "search" | "s" => Ok(Self::Search),
            "insert" | "i" => Ok(Self::Insert),
            "sync" => Ok(Self::Sync),
            "ser" | "serialize" => Ok(Self::Serialize),
            "clear" | "clear_cache" => Ok(Self::ClearCache),
            "list" | "list_conflicts" => Ok(Self::ListConflicts),
            "show" | "show_conflict" => Ok(Self::ShowConflict),
            "remove" | "remove_conflict" => Ok(Self::RemoveConflict),
            "run-migrations" => Ok(Self::RunMigrations),
            _ => Err(format_err!("Parse failure")),
        }
    }
}

fn parse_commands_from_str(s: &str) -> Result<DiaryAppCommands, String> {
    s.parse().map_err(|e| format!("{e}"))
}

#[derive(Parser, Debug, Clone)]
pub struct DiaryAppOpts {
    #[clap(value_parser = parse_commands_from_str)]
    /// Available commands are "(s)earch", "(i)nsert", "sync", "serialize,
    /// "clear", "clear_cache", "list", "list_conflicts", "show",
    /// "show_conflict", "remove", "remove_conflict"
    pub command: DiaryAppCommands,
    #[clap(
        short = 't',
        long = "text",
        required_if_eq("command", "search"),
        required_if_eq("command", "insert")
    )]
    pub text: Vec<StackString>,
}

impl DiaryAppOpts {
    /// # Errors
    /// Return error if db query fails
    pub async fn process_args() -> Result<(), Error> {
        let opts = Self::parse();

        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url);
        let dap = DiaryAppInterface::new(config, pool);

        match opts.command {
            DiaryAppCommands::Search => {
                let result = dap.search_text(&opts.text.join(" ")).await?;
                dap.stdout.send(result.join("\n"));
            }
            DiaryAppCommands::Insert => {
                dap.cache_text(&opts.text.join(" ")).await?;
            }
            DiaryAppCommands::Sync => {
                dap.sync_everything().await?;
            }
            DiaryAppCommands::Serialize => {
                for entry in dap.serialize_cache().await? {
                    dap.stdout.send(entry);
                }
            }
            DiaryAppCommands::ClearCache => {
                let entries: Vec<_> = DiaryCache::get_cache_entries(&dap.pool)
                    .await?
                    .try_collect()
                    .await?;
                for entry in entries {
                    dap.stdout.send(serde_json::to_string(&entry)?);
                    entry.delete_entry(&dap.pool).await?;
                }
            }
            DiaryAppCommands::ListConflicts => {
                async fn get_all_conflicts(
                    dap: &DiaryAppInterface,
                    date: Date,
                ) -> Result<(), Error> {
                    let conflicts: BTreeSet<_> = DiaryConflict::get_by_date(date, &dap.pool)
                        .await?
                        .try_collect()
                        .await?;
                    for entry in conflicts {
                        let timestamp: StackString = entry
                            .format(format_description!(
                                "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]Z"
                            ))
                            .unwrap_or_else(|_| String::new())
                            .into();
                        dap.stdout.send(timestamp);
                    }
                    Ok(())
                }

                if let Ok(date) = Date::parse(
                    &opts.text.join(""),
                    format_description!("[year]-[month]-[day]"),
                ) {
                    get_all_conflicts(&dap, date).await?;
                } else {
                    let conflicts: Vec<_> = DiaryConflict::get_all_dates(&dap.pool)
                        .await?
                        .try_collect()
                        .await?;
                    if conflicts.len() > 1 {
                        for date in conflicts {
                            let date = StackString::from_display(date);
                            dap.stdout.send(date);
                        }
                    } else {
                        for date in conflicts {
                            get_all_conflicts(&dap, date).await?;
                        }
                    }
                }
            }
            DiaryAppCommands::ShowConflict => {
                async fn show_conflict(
                    dap: &DiaryAppInterface,
                    datetime: OffsetDateTime,
                ) -> Result<(), Error> {
                    dap.stdout.send(format!("datetime {datetime}"));
                    let conflicts: Vec<_> =
                        DiaryConflict::get_by_datetime(datetime.into(), &dap.pool)
                            .await?
                            .map_ok(|entry| match entry.diff_type.as_str() {
                                "rem" => format!("\x1b[91m{}\x1b[0m", entry.diff_text).into(),
                                "add" => format!("\x1b[92m{}\x1b[0m", entry.diff_text).into(),
                                _ => entry.diff_text,
                            })
                            .try_collect()
                            .await?;
                    for timestamp in conflicts {
                        dap.stdout.send(timestamp);
                    }
                    Ok(())
                }

                if let Ok(datetime) =
                    OffsetDateTime::parse(&opts.text.join("").replace('Z', "+00:00"), &Rfc3339)
                        .map(|x| x.to_timezone(UTC))
                {
                    show_conflict(&dap, datetime).await?;
                } else if let Some(datetime) = DiaryConflict::get_first_conflict(&dap.pool).await? {
                    show_conflict(&dap, datetime).await?;
                }
            }
            DiaryAppCommands::RemoveConflict => {
                if let Ok(datetime) =
                    OffsetDateTime::parse(&opts.text.join("").replace('Z', "+00:00"), &Rfc3339)
                        .map(|x| x.to_timezone(UTC))
                {
                    DiaryConflict::remove_by_datetime(datetime.into(), &dap.pool).await?;
                } else if let Some(datetime) = DiaryConflict::get_first_conflict(&dap.pool).await? {
                    DiaryConflict::remove_by_datetime(datetime.into(), &dap.pool).await?;
                }
            }
            DiaryAppCommands::RunMigrations => {
                let mut client = dap.pool.get().await?;
                migrations::runner().run_async(&mut **client).await?;
            }
        }
        dap.stdout.close().await.map_err(Into::into)
    }
}
