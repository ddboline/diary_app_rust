#[macro_use]
extern crate diesel;

pub mod config;
pub mod diary_app_interface;
pub mod local_interface;
pub mod models;
pub mod pgpool;
pub mod s3_instance;
pub mod s3_interface;
pub mod schema;

use failure::{err_msg, Error};
use log::error;
use rand::distributions::{Distribution, Uniform};
use rand::thread_rng;
use std::thread::sleep;
use std::time::Duration;

pub fn exponential_retry<T, U>(closure: T) -> Result<U, Error>
where
    T: Fn() -> Result<U, Error>,
{
    let mut timeout: f64 = 1.0;
    let mut rng = thread_rng();
    let range = Uniform::from(0..1000);
    loop {
        match closure() {
            Ok(x) => return Ok(x),
            Err(e) => {
                error!("Got error {:?} , retrying", e);
                sleep(Duration::from_millis((timeout * 1000.0) as u64));
                timeout *= 4.0 * f64::from(range.sample(&mut rng)) / 1000.0;
                if timeout >= 64.0 {
                    return Err(err_msg(e));
                }
            }
        }
    }
}
