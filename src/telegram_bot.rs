#[macro_use]
extern crate lazy_static;

use crossbeam_utils::thread::{self, Scope};
use failure::{err_msg, Error};
use futures::Stream;
use log::debug;
use parking_lot::RwLock;
use std::collections::HashSet;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use telegram_bot::types::refs::UserId;
use telegram_bot::{Api, CanReplySendMessage, MessageKind, UpdateKind};
use tokio_core::reactor::Core;

use diary_app_rust::config::Config;
use diary_app_rust::diary_app_interface::DiaryAppInterface;
use diary_app_rust::models::AuthorizedUsers;
use diary_app_rust::pgpool::PgPool;

lazy_static! {
    static ref TELEGRAM_USERIDS: Arc<RwLock<HashSet<UserId>>> =
        Arc::new(RwLock::new(HashSet::new()));
    static ref OUTPUT_BUFFER: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
}

pub fn run_bot(telegram_bot_token: &str, pool: PgPool, scope: &Scope) -> Result<(), Error> {
    // let (s, r) = unbounded();

    let pool_ = pool.clone();
    let userid_handle = scope.spawn(move |_| fill_telegram_user_ids(pool_));
    let config = Config::init_config()?;
    let dapp_interface = DiaryAppInterface::new(config, pool);

    let mut core = Core::new()?;

    let api = Api::configure(telegram_bot_token)
        .build(core.handle())
        .map_err(|e| err_msg(format!("{}", e)))?;

    // Fetch new updates via long poll method
    let future = api.stream().for_each(|update| {
        // If the received update contains a new message...
        if let UpdateKind::Message(message) = update.kind {
            if let MessageKind::Text { ref data, .. } = message.kind {
                // Print received text message to stdout.
                debug!("{:?}", message);
                if TELEGRAM_USERIDS.read().contains(&message.from.id) {
                    let first_word = data.split_whitespace().nth(0);
                    match first_word
                        .map(|d| d.to_lowercase())
                        .as_ref()
                        .map(|d| d.as_str())
                    {
                        Some("search") | Some("s") => {
                            let search_text = data.trim_start_matches(first_word.unwrap()).trim();
                            OUTPUT_BUFFER.write().clear();
                            if let Ok(mut search_results) = dapp_interface.search_text(search_text)
                            {
                                search_results.reverse();
                                OUTPUT_BUFFER.write().extend_from_slice(&search_results);
                            }
                            if let Some(entry) = OUTPUT_BUFFER.write().pop() {
                                api.spawn(message.text_reply(entry));
                            } else {
                                api.spawn(message.text_reply("..."));
                            }
                        }
                        Some("next") | Some("n") => {
                            if let Some(entry) = OUTPUT_BUFFER.write().pop() {
                                api.spawn(message.text_reply(entry));
                            } else {
                                api.spawn(message.text_reply("..."));
                            }
                        }
                        Some("insert") | Some("i") => {
                            let insert_text = data.trim_start_matches(first_word.unwrap()).trim();
                            if let Ok(cache_entry) = dapp_interface.cache_text(insert_text.into()) {
                                api.spawn(
                                    message.text_reply(format!("cached entry {:?}", cache_entry)),
                                );
                            } else {
                                api.spawn(message.text_reply("failed to cache entry"));
                            }
                        }
                        _ => api.spawn(message.text_reply(
                            "Possible commands are: search (s), next (n) or insert (i)",
                        )),
                    }
                } else {
                    // Answer message with "Hi".
                    api.spawn(message.text_reply(format!(
                        "Hi, {}, user_id {}! You just wrote '{}'",
                        &message.from.first_name, &message.from.id, data
                    )));
                }
            }
        }

        Ok(())
    });

    core.run(future).map_err(|e| err_msg(format!("{}", e)))?;
    drop(userid_handle);
    Ok(())
}

fn fill_telegram_user_ids(pool: PgPool) {
    loop {
        if let Ok(authorized_users) = AuthorizedUsers::get_authorized_users(&pool) {
            let mut telegram_userid_set = TELEGRAM_USERIDS.write();
            telegram_userid_set.clear();
            for user in authorized_users {
                if let Some(userid) = user.telegram_userid {
                    telegram_userid_set.insert(UserId::new(userid));
                }
            }
        }
        sleep(Duration::from_secs(60));
    }
}

fn main() {
    env_logger::init();
    let config = Config::init_config().unwrap();
    let pool = PgPool::new(&config.database_url);
    thread::scope(|scope| run_bot(&config.telegram_bot_token, pool, scope))
        .map_err(|x| err_msg(format!("{:?}", x)))
        .and_then(|r| r)
        .unwrap();
}
