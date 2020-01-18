#![allow(clippy::must_use_candidate)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]

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

use anyhow::{format_err, Error};
use log::error;
use retry::{delay::jitter, delay::Exponential, retry};

pub fn exponential_retry<T, U>(closure: T) -> Result<U, Error>
where
    T: Fn() -> Result<U, Error>,
{
    retry(
        Exponential::from_millis(2)
            .map(jitter)
            .map(|x| x * 500)
            .take(6),
        || {
            closure().map_err(|e| {
                error!("Got error {:?} , retrying", e);
                e
            })
        },
    )
    .map_err(|e| format_err!("{:?}", e))
}
