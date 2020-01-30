use actix_threadpool::run as block;
use anyhow::{format_err, Error};
use futures::future::join;
use futures::StreamExt;
use lazy_static::lazy_static;
use log::debug;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use telegram_bot::types::refs::UserId;
use telegram_bot::{Api, CanReplySendMessage, MessageKind, UpdateKind};
use tokio::sync::RwLock;
use tokio::time::{delay_for, timeout, Duration};

use diary_app_lib::config::Config;
use diary_app_lib::diary_app_interface::DiaryAppInterface;
use diary_app_lib::models::AuthorizedUsers;
use diary_app_lib::pgpool::PgPool;

type UserIds = RwLock<HashSet<UserId>>;
type OBuffer = RwLock<Vec<String>>;

lazy_static! {
    static ref TELEGRAM_USERIDS: UserIds = RwLock::new(HashSet::new());
    static ref OUTPUT_BUFFER: OBuffer = RwLock::new(Vec::new());
    static ref FAILURE_COUNT: FailureCount = FailureCount::new(5);
}

struct FailureCount {
    max_count: usize,
    counter: AtomicUsize,
}

impl FailureCount {
    fn new(max_count: usize) -> Self {
        Self {
            max_count,
            counter: AtomicUsize::new(0),
        }
    }

    fn check(&self) -> Result<(), Error> {
        if self.counter.load(Ordering::SeqCst) > self.max_count {
            Err(format_err!(
                "Failed after retrying {} times",
                self.max_count
            ))
        } else {
            Ok(())
        }
    }

    fn reset(&self) -> Result<(), Error> {
        if self.counter.swap(0, Ordering::SeqCst) > self.max_count {
            Err(format_err!(
                "Failed after retrying {} times",
                self.max_count
            ))
        } else {
            Ok(())
        }
    }

    fn increment(&self) -> Result<(), Error> {
        if self.counter.fetch_add(1, Ordering::SeqCst) > self.max_count {
            Err(format_err!(
                "Failed after retrying {} times",
                self.max_count
            ))
        } else {
            Ok(())
        }
    }
}

async fn bot_handler(dapp_interface: DiaryAppInterface) -> Result<(), Error> {
    let api = Api::new(&dapp_interface.config.telegram_bot_token);
    let mut stream = api.stream();
    while let Some(update) = stream.next().await {
        FAILURE_COUNT.check()?;
        // If the received update contains a new message...
        if let UpdateKind::Message(message) = update?.kind {
            FAILURE_COUNT.check()?;
            if let MessageKind::Text { ref data, .. } = message.kind {
                FAILURE_COUNT.check()?;
                // Print received text message to stdout.
                debug!("{:?}", message);
                if TELEGRAM_USERIDS.read().await.contains(&message.from.id) {
                    FAILURE_COUNT.check()?;
                    let first_word = data.split_whitespace().nth(0);
                    match first_word
                        .map(str::to_lowercase)
                        .as_ref()
                        .map(String::as_str)
                    {
                        Some(":search") | Some(":s") => {
                            let search_text = data
                                .trim_start_matches(first_word.unwrap())
                                .trim()
                                .to_string();
                            OUTPUT_BUFFER.write().await.clear();
                            let d = dapp_interface.clone();
                            if let Ok(mut search_results) =
                                block(move || d.search_text(&search_text)).await
                            {
                                search_results.reverse();
                                OUTPUT_BUFFER
                                    .write()
                                    .await
                                    .extend_from_slice(&search_results);
                            }
                            FAILURE_COUNT.check()?;
                            if let Some(entry) = OUTPUT_BUFFER.write().await.pop() {
                                api.send(message.text_reply(entry)).await?;
                            } else {
                                api.send(message.text_reply("...")).await?;
                            }
                            FAILURE_COUNT.check()?;
                        }
                        Some(":next") | Some(":n") => {
                            if let Some(entry) = OUTPUT_BUFFER.write().await.pop() {
                                api.send(message.text_reply(entry)).await?;
                            } else {
                                api.send(message.text_reply("...")).await?;
                            }
                        }
                        Some(":insert") | Some(":i") => {
                            let insert_text = data
                                .trim_start_matches(first_word.unwrap())
                                .trim()
                                .to_string();
                            let d = dapp_interface.clone();
                            if let Ok(cache_entry) =
                                block(move || d.cache_text(insert_text.into())).await
                            {
                                api.send(
                                    message.text_reply(format!("cached entry {:?}", cache_entry)),
                                )
                                .await?;
                            } else {
                                api.send(message.text_reply("failed to cache entry"))
                                    .await?;
                            }
                            FAILURE_COUNT.check()?;
                        }
                        _ => {
                            let d = dapp_interface.clone();
                            let data = data.to_string();
                            if let Ok(cache_entry) = block(move || d.cache_text(data.into())).await
                            {
                                api.send(
                                    message.text_reply(format!("cached entry {:?}", cache_entry)),
                                )
                                .await?;
                            } else {
                                api.send(message.text_reply("failed to cache entry"))
                                    .await?;
                            }
                            FAILURE_COUNT.check()?;
                        }
                    }
                } else {
                    // Answer message with "Hi".
                    api.send(message.text_reply(format!(
                        "Hi, {}, user_id {}! You just wrote '{}'",
                        &message.from.first_name, &message.from.id, data
                    )))
                    .await?;
                }
            }
        }
    }
    Ok(())
}

async fn telegram_worker(dapp: &DiaryAppInterface) -> Result<(), Error> {
    loop {
        FAILURE_COUNT.check()?;
        let d = dapp.clone();

        match timeout(Duration::from_secs(3600), bot_handler(d)).await {
            Err(_) | Ok(Ok(_)) => FAILURE_COUNT.reset()?,
            Ok(Err(_)) => FAILURE_COUNT.increment()?,
        }
    }
}

async fn fill_telegram_user_ids(pool: PgPool) -> Result<(), Error> {
    loop {
        FAILURE_COUNT.check()?;
        let p = pool.clone();
        if let Ok(authorized_users) = block(move || AuthorizedUsers::get_authorized_users(&p)).await
        {
            let mut telegram_userid_set = TELEGRAM_USERIDS.write().await;
            telegram_userid_set.clear();
            for user in authorized_users {
                if let Some(userid) = user.telegram_userid {
                    telegram_userid_set.insert(UserId::new(userid));
                }
            }
            FAILURE_COUNT.reset()?;
        } else {
            FAILURE_COUNT.increment()?
        }
        delay_for(Duration::from_secs(60)).await;
    }
}

pub async fn run_bot() -> Result<(), Error> {
    let config = Config::init_config().unwrap();
    let pool = PgPool::new(&config.database_url);
    let dapp = DiaryAppInterface::new(config, pool);

    let pool_ = dapp.pool.clone();

    let userid_handle = fill_telegram_user_ids(pool_);
    let telegram_handle = telegram_worker(&dapp);

    let (r0, r1) = join(userid_handle, telegram_handle).await;
    r0.and_then(|_| r1)
}
