use anyhow::{format_err, Error};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct FailureCount {
    max_count: usize,
    counter: AtomicUsize,
}

impl FailureCount {
    #[must_use]
    pub fn new(max_count: usize) -> Self {
        Self {
            max_count,
            counter: AtomicUsize::new(0),
        }
    }

    /// # Errors
    /// Return error if retry more than `max_count` times
    pub fn check(&self) -> Result<(), Error> {
        if self.counter.load(Ordering::SeqCst) > self.max_count {
            Err(format_err!(
                "Failed after retrying {} times",
                self.max_count
            ))
        } else {
            Ok(())
        }
    }

    /// # Errors
    /// Return error if retry more than `max_count` times
    pub fn reset(&self) -> Result<(), Error> {
        if self.counter.swap(0, Ordering::SeqCst) > self.max_count {
            Err(format_err!(
                "Failed after retrying {} times",
                self.max_count
            ))
        } else {
            Ok(())
        }
    }

    /// # Errors
    /// Return error if retry more than `max_count` times
    pub fn increment(&self) -> Result<(), Error> {
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
