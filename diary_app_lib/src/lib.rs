#![allow(clippy::must_use_candidate)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::used_underscore_binding)]

#[macro_use]
extern crate diesel;

pub mod config;
pub mod diary_app_interface;
pub mod diary_app_opts;
pub mod local_interface;
pub mod models;
pub mod pgpool;
pub mod s3_instance;
pub mod s3_interface;
pub mod schema;
pub mod ssh_instance;
pub mod stdout_channel;

use anyhow::Error;
use rand::{
    distributions::{Distribution, Uniform},
    thread_rng,
};
use std::future::Future;
use tokio::time::{delay_for, Duration};

pub async fn exponential_retry<T, U, F>(f: T) -> Result<U, Error>
where
    T: Fn() -> F,
    F: Future<Output = Result<U, Error>>,
{
    let mut timeout: f64 = 1.0;
    let range = Uniform::from(0..1000);
    loop {
        match f().await {
            Ok(resp) => return Ok(resp),
            Err(err) => {
                delay_for(Duration::from_millis((timeout * 1000.0) as u64)).await;
                timeout *= 4.0 * f64::from(range.sample(&mut thread_rng())) / 1000.0;
                if timeout >= 64.0 {
                    return Err(err);
                }
            }
        }
    }
}
