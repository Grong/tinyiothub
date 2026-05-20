use std::sync::atomic::{AtomicU64, Ordering};

pub struct ReflectionMetrics {
    pub total: AtomicU64,
    pub failures: AtomicU64,
    pub consecutive_failures: AtomicU64,
}

impl ReflectionMetrics {
    pub fn new() -> Self {
        Self {
            total: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            consecutive_failures: AtomicU64::new(0),
        }
    }

    pub fn record_success(&self) {
        self.total.fetch_add(1, Ordering::Relaxed);
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        self.total.fetch_add(1, Ordering::Relaxed);
        self.failures.fetch_add(1, Ordering::Relaxed);
        let consecutive = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if consecutive >= 10 {
            tracing::error!(
                consecutive_failures = consecutive,
                "Reflection pipeline has failed 10+ consecutive times"
            );
        }
    }
}
