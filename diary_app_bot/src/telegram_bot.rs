use anyhow::Error;
use futures::{future::join, StreamExt, TryStreamExt};
use log::debug;
use once_cell::sync::Lazy;
use stack_string::{format_sstr, StackString};
use std::collections::HashSet;
use telegram_bot::{types::refs::UserId, Api, CanReplySendMessage, MessageKind, UpdateKind};
use tokio::{
    sync::{
        mpsc::{channel, Receiver},
        RwLock,
    },
    task::spawn,
    time::{sleep, timeout, Duration},
};

use diary_app_lib::{
    config::Config, diary_app_interface::DiaryAppInterface, models::AuthorizedUsers, pgpool::PgPool,
};

use crate::failure_count::FailureCount;

type UserIds = RwLock<HashSet<UserId>>;
type OBuffer = RwLock<Vec<StackString>>;

static TELEGRAM_USERIDS: Lazy<UserIds> = Lazy::new(|| RwLock::new(HashSet::new()));
static OUTPUT_BUFFER: Lazy<OBuffer> = Lazy::new(|| RwLock::new(Vec::new()));
static FAILURE_COUNT: Lazy<FailureCount> = Lazy::new(|| FailureCount::new(5));

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
    let (send, recv) = channel(1);
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
                        Some(":search" | ":s") => {
                            let search_text = data.trim_start_matches(first_word.unwrap()).trim();
                            OUTPUT_BUFFER.write().await.clear();
                            if let Ok(mut search_results) =
                                dapp_interface.search_text(search_text).await
                            {
                                search_results.reverse();
                                OUTPUT_BUFFER
                                    .write()
                                    .await
                                    .extend_from_slice(&search_results);
                            }
                            FAILURE_COUNT.check()?;
                            if let Some(entry) = OUTPUT_BUFFER.write().await.pop() {
                                api.send(message.text_reply(entry.as_str())).await?;
                            } else {
                                api.send(message.text_reply("...")).await?;
                            }
                            FAILURE_COUNT.check()?;
                        }
                        Some(":help" | ":h") => {
                            let help_text = format_sstr!(
                                "{}\n{}\n{}\n{}",
                                ":s, :search => search for text, get text for given date, or for \
                                 `today`",
                                ":n, :next => get the next page of search results",
                                ":sync => sync with local and s3",
                                ":i, :insert => insert text (also the action if no other command \
                                 is specified"
                            );
                            api.send(message.text_reply(help_text.as_str())).await?;
                        }
                        Some(":sync") => {
                            send.send(()).await?;
                            api.send(
                                message.text_reply("started sync, reply with :n to see result"),
                            )
                            .await?;
                        }
                        Some(":next" | ":n") => {
                            if let Some(entry) = OUTPUT_BUFFER.write().await.pop() {
                                api.send(message.text_reply(entry.as_str())).await?;
                            } else {
                                api.send(message.text_reply("...")).await?;
                            }
                        }
                        Some(":insert" | ":i") => {
                            let insert_text = data.trim_start_matches(first_word.unwrap()).trim();
                            if let Ok(cache_entry) = dapp_interface.cache_text(insert_text).await {
                                let reply = format_sstr!("cached entry {cache_entry:?}");
                                api.send(message.text_reply(reply.as_str())).await?;
                            } else {
                                api.send(message.text_reply("failed to cache entry"))
                                    .await?;
                            }
                            FAILURE_COUNT.check()?;
                        }
                        _ => {
                            if let Ok(cache_entry) = dapp_interface.cache_text(data).await {
                                let reply = format_sstr!("cached entry {cache_entry:?}");
                                api.send(message.text_reply(reply.as_str())).await?;
                            } else {
                                api.send(message.text_reply("failed to cache entry"))
                                    .await?;
                            }
                            FAILURE_COUNT.check()?;
                        }
                    }
                } else {
                    // Answer message with "Hi".
                    let reply = format_sstr!(
                        "Hi, {n}, user_id {i}! You just wrote '{data}'",
                        n = message.from.first_name,
                        i = message.from.id,
                    );
                    api.send(message.text_reply(reply.as_str())).await?;
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
            Err(_) | Ok(Ok(())) => FAILURE_COUNT.reset()?,
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
                .try_filter_map(|user| async move { Ok(user.telegram_userid.map(UserId::new)) })
                .try_collect()
                .await?;
            *TELEGRAM_USERIDS.write().await = telegram_userid_set;
            FAILURE_COUNT.reset()?;
        } else {
            FAILURE_COUNT.increment()?;
        }
        sleep(Duration::from_secs(60)).await;
    }
}

/// # Errors
/// Returns error if config fails or bot fails
pub async fn run_bot() -> Result<(), Error> {
    let config = Config::init_config()?;
    let pool = PgPool::new(&config.database_url);
    let sdk_config = aws_config::load_from_env().await;
    let dapp = DiaryAppInterface::new(config, &sdk_config, pool);

    let pool_ = dapp.pool.clone();

    let userid_handle = fill_telegram_user_ids(pool_);
    let telegram_handle = telegram_worker(dapp);

    let (r0, r1) = join(userid_handle, telegram_handle).await;
    r0.and(r1)
}
