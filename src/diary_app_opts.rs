use failure::{err_msg, Error};
use std::io::{stdout, Write};
use std::str::FromStr;
use structopt::StructOpt;

use crate::config::Config;
use crate::diary_app_interface::DiaryAppInterface;
use crate::pgpool::PgPool;

#[derive(Debug, Clone, Copy)]
pub enum DiaryAppCommands {
    Search,
    Insert,
    Sync,
}

impl FromStr for DiaryAppCommands {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "search" | "s" => Ok(DiaryAppCommands::Search),
            "insert" | "i" => Ok(DiaryAppCommands::Insert),
            "sync" => Ok(DiaryAppCommands::Sync),
            _ => Err(err_msg("Parse failure")),
        }
    }
}

#[derive(StructOpt, Debug, Clone)]
pub struct DiaryAppOpts {
    #[structopt(parse(try_from_str))]
    /// Available commands are "(s)earch", "(i)nsert", "sync"
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
        let opts = DiaryAppOpts::from_args();

        let config = Config::init_config()?;
        let pool = PgPool::new(&config.database_url);
        let dap = DiaryAppInterface::new(config, pool);

        match opts.command {
            DiaryAppCommands::Search => {
                let result = dap.search_text(&opts.text.join(" "))?;
                writeln!(stdout().lock(), "{}", result.join("\n"))?;
            }
            DiaryAppCommands::Insert => {
                dap.cache_text(&opts.text.join(" "))?;
            }
            DiaryAppCommands::Sync => {
                dap.sync_merge_cache_to_entries()?;
                dap.sync_entries()?;
                dap.local.cleanup_local()?;
                dap.local.export_year_to_local()?;
            }
        }
        Ok(())
    }
}
