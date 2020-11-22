use anyhow::Error;
use futures::{future::join, StreamExt};
use lazy_static::lazy_static;
use log::debug;
use stack_string::StackString;
use std::collections::HashSet;
use telegram_bot::{types::refs::UserId, Api, CanReplySendMessage, MessageKind, UpdateKind};
use tokio::{
    sync::{
        mpsc::{channel, Receiver},
        RwLock,
    },
    task::spawn,
    time::{delay_for, timeout, Duration},
};

use diary_app_lib::{
    config::Config, diary_app_interface::DiaryAppInterface, models::AuthorizedUsers, pgpool::PgPool,
};

use crate::failure_count::FailureCount;

type UserIds = RwLock<HashSet<UserId>>;
type OBuffer = RwLock<Vec<StackString>>;

lazy_static! {
    static ref TELEGRAM_USERIDS: UserIds = RwLock::new(HashSet::new());
    static ref OUTPUT_BUFFER: OBuffer = RwLock::new(Vec::new());
    static ref FAILURE_COUNT: FailureCount = FailureCount::new(5);
}

async fn diary_sync(
    dapp_interface: DiaryAppInterface,
    mut recv: Receiver<()>,
) -> Result<(), Error> {
    while recv.recv().await.is_some() {
        let output = dapp_interface.sync_everything().await?;
        let mut buf = OUTPUT_BUFFER.write().await;
        buf.clear();
        buf.push(output.join("\n").into());
    }
    Ok(())
}

async fn bot_handler(dapp_interface: DiaryAppInterface) -> Result<(), Error> {
    let (mut send, recv) = channel(1);
    let sync_task = {
        let d = dapp_interface.clone();
        spawn(diary_sync(d, recv))
    };
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
                    let first_word = data.split_whitespace().next();
                    match first_word.map(str::to_lowercase).as_deref() {
                        Some(":search") | Some(":s") => {
                            let search_text = data
                                .trim_start_matches(first_word.unwrap())
                                .trim()
                                .to_string();
                            OUTPUT_BUFFER.write().await.clear();
                            if let Ok(mut search_results) =
                                dapp_interface.search_text(&search_text).await
                            {
                                search_results.reverse();
                                OUTPUT_BUFFER
                                    .write()
                                    .await
                                    .extend_from_slice(&search_results);
                            }
                            FAILURE_COUNT.check()?;
                            if let Some(entry) = OUTPUT_BUFFER.write().await.pop() {
                                api.send(message.text_reply(entry.to_string())).await?;
                            } else {
                                api.send(message.text_reply("...")).await?;
                            }
                            FAILURE_COUNT.check()?;
                        }
                        Some(":help") | Some(":h") => {
                            let help_text = format!(
                                "{}\n{}\n{}\n{}",
                                ":s, :search => search for text, get text for given date, or for \
                                 `today`",
                                ":n, :next => get the next page of search results",
                                ":sync => sync with local and s3",
                                ":i, :insert => insert text (also the action if no other command \
                                 is specified"
                            );
                            api.send(message.text_reply(help_text)).await?;
                        }
                        Some(":sync") => {
                            send.send(()).await?;
                            api.send(
                                message.text_reply("started sync, reply with :n to see result"),
                            )
                            .await?;
                        }
                        Some(":next") | Some(":n") => {
                            if let Some(entry) = OUTPUT_BUFFER.write().await.pop() {
                                api.send(message.text_reply(entry.to_string())).await?;
                            } else {
                                api.send(message.text_reply("...")).await?;
                            }
                        }
                        Some(":insert") | Some(":i") => {
                            let insert_text = data
                                .trim_start_matches(first_word.unwrap())
                                .trim()
                                .to_string();
                            if let Ok(cache_entry) = dapp_interface.cache_text(&insert_text).await {
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
                            let data = data.to_string();
                            if let Ok(cache_entry) = dapp_interface.cache_text(&data).await {
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
    sync_task.await?
}

async fn telegram_worker(dapp: DiaryAppInterface) -> Result<(), Error> {
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
        if let Ok(authorized_users) = AuthorizedUsers::get_authorized_users(&p).await {
            let telegram_userid_set: HashSet<_> = authorized_users
                .into_iter()
                .filter_map(|user| user.telegram_userid.map(UserId::new))
                .collect();
            *TELEGRAM_USERIDS.write().await = telegram_userid_set;
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
    let telegram_handle = telegram_worker(dapp);

    let (r0, r1) = join(userid_handle, telegram_handle).await;
    r0.and(r1)
}
