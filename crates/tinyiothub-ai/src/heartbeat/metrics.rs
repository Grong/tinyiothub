//! Heartbeat & LLM Metrics — atomic counters and latency histograms.

use std::sync::atomic::{AtomicU64, Ordering};

const LATENCY_BUCKETS_MS: [u64; 7] = [500, 1000, 2000, 5000, 10000, 30000, 120000];

/// Heartbeat and LLM operational metrics.
pub struct Metrics {
    pub active_loops: AtomicU64,
    pub paused_loops: AtomicU64,
    pub failed_loops: AtomicU64,
    pub loops_completed: AtomicU64,

    llm_latency_buckets: [AtomicU64; 7],
    llm_calls_total: AtomicU64,
    llm_calls_failed: AtomicU64,
    llm_total_latency_ms: AtomicU64,

    pub events_published: AtomicU64,
    pub events_dropped: AtomicU64,
    pub reflection_skips: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            active_loops: AtomicU64::new(0),
            paused_loops: AtomicU64::new(0),
            failed_loops: AtomicU64::new(0),
            loops_completed: AtomicU64::new(0),
            llm_latency_buckets: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            llm_calls_total: AtomicU64::new(0),
            llm_calls_failed: AtomicU64::new(0),
            llm_total_latency_ms: AtomicU64::new(0),
            events_published: AtomicU64::new(0),
            events_dropped: AtomicU64::new(0),
            reflection_skips: AtomicU64::new(0),
        }
    }

    pub fn record_llm_call(&self, latency_ms: u64, success: bool) {
        self.llm_calls_total.fetch_add(1, Ordering::Relaxed);
        if success {
            self.llm_total_latency_ms.fetch_add(latency_ms, Ordering::Relaxed);
            for (i, &boundary) in LATENCY_BUCKETS_MS.iter().enumerate() {
                if latency_ms <= boundary {
                    self.llm_latency_buckets[i].fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        } else {
            self.llm_calls_failed.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn avg_llm_latency_ms(&self) -> f64 {
        let total = self.llm_calls_total.load(Ordering::Relaxed);
        let failed = self.llm_calls_failed.load(Ordering::Relaxed);
        let success = total.saturating_sub(failed);
        if success == 0 {
            return 0.0;
        }
        self.llm_total_latency_ms.load(Ordering::Relaxed) as f64 / success as f64
    }

    pub fn latency_histogram(&self) -> Vec<(u64, u64)> {
        LATENCY_BUCKETS_MS
            .iter()
            .enumerate()
            .map(|(i, &b)| (b, self.llm_latency_buckets[i].load(Ordering::Relaxed)))
            .collect()
    }

    pub fn llm_calls_total(&self) -> u64 {
        self.llm_calls_total.load(Ordering::Relaxed)
    }

    pub fn llm_calls_failed(&self) -> u64 {
        self.llm_calls_failed.load(Ordering::Relaxed)
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub active_loops: u64,
    pub paused_loops: u64,
    pub failed_loops: u64,
    pub loops_completed: u64,
    pub llm_calls_total: u64,
    pub llm_calls_failed: u64,
    pub avg_llm_latency_ms: f64,
    pub latency_histogram: Vec<(u64, u64)>,
    pub events_published: u64,
    pub events_dropped: u64,
    pub reflection_skips: u64,
}

impl Metrics {
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            active_loops: self.active_loops.load(Ordering::Relaxed),
            paused_loops: self.paused_loops.load(Ordering::Relaxed),
            failed_loops: self.failed_loops.load(Ordering::Relaxed),
            loops_completed: self.loops_completed.load(Ordering::Relaxed),
            llm_calls_total: self.llm_calls_total(),
            llm_calls_failed: self.llm_calls_failed(),
            avg_llm_latency_ms: self.avg_llm_latency_ms(),
            latency_histogram: self.latency_histogram(),
            events_published: self.events_published.load(Ordering::Relaxed),
            events_dropped: self.events_dropped.load(Ordering::Relaxed),
            reflection_skips: self.reflection_skips.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_initial_state() {
        let m = Metrics::new();
        assert_eq!(m.llm_calls_total(), 0);
        assert_eq!(m.avg_llm_latency_ms(), 0.0);
        assert!(m.latency_histogram().iter().all(|(_, c)| *c == 0));
    }

    #[test]
    fn test_record_llm_call_updates_histogram() {
        let m = Metrics::new();
        m.record_llm_call(400, true);
        m.record_llm_call(800, true);
        m.record_llm_call(3000, true);
        m.record_llm_call(100, false);

        assert_eq!(m.llm_calls_total(), 4);
        assert_eq!(m.llm_calls_failed(), 1);

        let hist = m.latency_histogram();
        assert_eq!(hist[0].1, 1);
        assert_eq!(hist[1].1, 1);
        assert_eq!(hist[2].1, 0);
        assert_eq!(hist[3].1, 1);

        let avg = m.avg_llm_latency_ms();
        assert!((avg - 1400.0).abs() < 1.0);
    }

    #[test]
    fn test_atomic_counters() {
        let m = Metrics::new();
        m.active_loops.fetch_add(3, Ordering::Relaxed);
        m.paused_loops.fetch_add(1, Ordering::Relaxed);
        m.failed_loops.fetch_add(2, Ordering::Relaxed);

        let snap = m.snapshot();
        assert_eq!(snap.active_loops, 3);
        assert_eq!(snap.paused_loops, 1);
        assert_eq!(snap.failed_loops, 2);
    }
}
