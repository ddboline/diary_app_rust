use anyhow::Error;
use chrono::Utc;
use deadqueue::limited::Queue;
use log::debug;
use std::sync::Arc;
use tokio::{
    sync::Mutex,
    task::{spawn, JoinHandle},
    time::{sleep, Duration},
};

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<RateLimiterInner>,
    #[allow(dead_code)]
    rate_task: Arc<JoinHandle<()>>,
}

impl RateLimiter {
    pub fn new(max_per_unit_time: usize, unit_time_ms: usize) -> Self {
        let inner = Arc::new(RateLimiterInner::new(max_per_unit_time, unit_time_ms));
        let rate_task = Arc::new({
            let inner = inner.clone();
            spawn(async move {
                inner.check_reset().await;
            })
        });
        Self { inner, rate_task }
    }

    pub async fn acquire(&self) {
        self.inner.rate_queue.pop().await;
    }
}

struct RateLimiterInner {
    max_per_unit_time: usize,
    unit_time_ms: usize,
    rate_queue: Queue<()>,
}

impl RateLimiterInner {
    fn new(max_per_unit_time: usize, unit_time_ms: usize) -> Self {
        Self {
            max_per_unit_time,
            unit_time_ms,
            rate_queue: Queue::new(max_per_unit_time),
        }
    }

    async fn check_reset(&self) {
        loop {
            for _ in 0..self.max_per_unit_time {
                if self.rate_queue.try_push(()).is_err() {
                    break;
                }
            }
            sleep(Duration::from_millis(self.unit_time_ms as u64)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use chrono::Utc;
    use log::debug;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::{
        task::spawn,
        time::{sleep, Duration},
    };

    use crate::rate_limiter::RateLimiter;

    #[tokio::test]
    async fn test_rate_limiter() -> Result<(), Error> {
        env_logger::init();

        let rate_limiter = RateLimiter::new(1000, 100);
        let test_count = Arc::new(AtomicUsize::new(0));

        let start = Utc::now();

        let tasks: Vec<_> = (0..10_000)
            .map(|_| {
                let rate_limiter = rate_limiter.clone();
                let test_count = test_count.clone();
                spawn(async move {
                    rate_limiter.acquire().await;
                    test_count.fetch_add(1, Ordering::SeqCst);
                })
            })
            .collect();

        sleep(Duration::from_millis(100)).await;

        for _ in 0..5 {
            let count = test_count.load(Ordering::SeqCst);
            debug!("{}", count);
            sleep(Duration::from_millis(100)).await;
        }
        for t in tasks {
            t.await?;
        }

        let elapsed = Utc::now() - start;

        debug!("{}", elapsed);
        assert!(elapsed.num_milliseconds() >= 900);
        assert_eq!(test_count.load(Ordering::SeqCst), 10_000);
        Ok(())
    }
}
