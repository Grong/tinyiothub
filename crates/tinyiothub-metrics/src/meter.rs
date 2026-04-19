//! Metric instrument types: Counter, Gauge, Histogram

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// A monotonically increasing counter.
#[derive(Debug)]
pub struct Counter {
    name: String,
    value: AtomicU64,
}

impl Counter {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: AtomicU64::new(0),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn increment(&self, delta: u64) {
        self.value.fetch_add(delta, Ordering::Relaxed);
    }

    pub fn value(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// A gauge that can go up or down.
#[derive(Debug)]
pub struct Gauge {
    name: String,
    value: AtomicI64,
}

impl Gauge {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: AtomicI64::new(0),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set(&self, value: i64) {
        self.value.store(value, Ordering::Relaxed);
    }

    pub fn value(&self) -> i64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// A histogram for recording distributions.
#[derive(Debug)]
pub struct Histogram {
    name: String,
    buckets: Vec<f64>,
    counts: Vec<AtomicU64>,
}

impl Histogram {
    pub fn new(name: impl Into<String>, buckets: Vec<f64>) -> Self {
        let len = buckets.len() + 1; // +1 for overflow bucket
        Self {
            name: name.into(),
            buckets,
            counts: (0..len).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn record(&self, value: f64) {
        let idx = self.buckets.partition_point(|b| *b < value);
        if idx < self.counts.len() {
            self.counts[idx].fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn counts(&self) -> Vec<u64> {
        self.counts
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .collect()
    }
}

/// Unified metric value type for registry snapshots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MetricValue {
    Counter(u64),
    Gauge(i64),
    Histogram { sum: f64, count: u64 },
}
