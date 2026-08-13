// Performance optimization utilities for improved user experience
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::RwLock;
use tracing::{debug, info};

/// Generic cache implementation with TTL support
pub struct Cache<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    data: Arc<RwLock<HashMap<K, CacheEntry<V>>>>,
    ttl: Duration,
    max_size: usize,
}

#[derive(Clone)]
struct CacheEntry<V> {
    value: V,
    expires_at: Instant,
    access_count: u64,
    last_accessed: Instant,
}

impl<K, V> Cache<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    /// Create a new cache with specified TTL and maximum size
    pub fn new(ttl: Duration, max_size: usize) -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            ttl,
            max_size,
        }
    }

    /// Get value from cache if it exists and hasn't expired
    pub async fn get(&self, key: &K) -> Option<V> {
        let mut cache = self.data.write().await;

        if let Some(entry) = cache.get_mut(key) {
            if entry.expires_at > Instant::now() {
                entry.access_count += 1;
                entry.last_accessed = Instant::now();
                debug!("Cache hit for key (access count: {})", entry.access_count);
                return Some(entry.value.clone());
            } else {
                // Entry expired, remove it
                cache.remove(key);
                debug!("Cache entry expired and removed");
            }
        }

        debug!("Cache miss");
        None
    }

    /// Set value in cache with automatic eviction if needed
    pub async fn set(&self, key: K, value: V) {
        let mut cache = self.data.write().await;

        // Check if we need to evict entries
        if cache.len() >= self.max_size {
            self.evict_lru(&mut cache).await;
        }

        let entry = CacheEntry {
            value,
            expires_at: Instant::now() + self.ttl,
            access_count: 1,
            last_accessed: Instant::now(),
        };

        cache.insert(key, entry);
        debug!("Cache entry added (cache size: {})", cache.len());
    }

    /// Get or compute value with caching
    pub async fn get_or_compute<F, Fut, E>(&self, key: K, compute: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<V, E>>,
    {
        // Try to get from cache first
        if let Some(value) = self.get(&key).await {
            return Ok(value);
        }

        // Compute the value
        let value = compute().await?;

        // Store in cache
        self.set(key, value.clone()).await;

        Ok(value)
    }

    /// Remove expired entries and evict least recently used if needed
    async fn evict_lru(&self, cache: &mut HashMap<K, CacheEntry<V>>) {
        // First, remove expired entries
        let now = Instant::now();
        cache.retain(|_, entry| entry.expires_at > now);

        // If still over capacity, remove LRU entries
        if cache.len() >= self.max_size {
            let mut entries_to_remove = Vec::new();
            {
                let mut entries: Vec<_> = cache.iter().collect();
                entries.sort_by_key(|(_, entry)| entry.last_accessed);

                let to_remove = cache.len() - self.max_size + 1;
                for (key, _) in entries.iter().take(to_remove) {
                    entries_to_remove.push((*key).clone());
                }
            }

            for key in entries_to_remove {
                cache.remove(&key);
            }

            info!("Evicted {} LRU cache entries", cache.len() - self.max_size + 1);
        }
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let cache = self.data.read().await;
        let now = Instant::now();

        let mut expired_count = 0;
        let mut total_access_count = 0;

        for entry in cache.values() {
            if entry.expires_at <= now {
                expired_count += 1;
            }
            total_access_count += entry.access_count;
        }

        CacheStats {
            total_entries: cache.len(),
            expired_entries: expired_count,
            total_access_count,
            max_size: self.max_size,
            ttl_seconds: self.ttl.as_secs(),
        }
    }

    /// Clear all cache entries
    pub async fn clear(&self) {
        let mut cache = self.data.write().await;
        cache.clear();
        info!("Cache cleared");
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub total_access_count: u64,
    pub max_size: usize,
    pub ttl_seconds: u64,
}

/// Performance metrics collection
pub struct PerformanceMetrics {
    operation_times: Arc<RwLock<HashMap<String, Vec<Duration>>>>,
    error_counts: Arc<RwLock<HashMap<String, u64>>>,
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self {
            operation_times: Arc::new(RwLock::new(HashMap::new())),
            error_counts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record operation time
    pub async fn record_operation_time(&self, operation: &str, duration: Duration) {
        let mut times = self.operation_times.write().await;
        times
            .entry(operation.to_string())
            .or_insert_with(Vec::new)
            .push(duration);

        // Keep only last 100 measurements to prevent memory growth
        if let Some(measurements) = times.get_mut(operation)
            && measurements.len() > 100
        {
            measurements.drain(0..measurements.len() - 100);
        }
    }

    /// Record error occurrence
    pub async fn record_error(&self, operation: &str) {
        let mut errors = self.error_counts.write().await;
        *errors.entry(operation.to_string()).or_insert(0) += 1;
    }

    /// Get average operation time
    pub async fn get_average_time(&self, operation: &str) -> Option<Duration> {
        let times = self.operation_times.read().await;
        if let Some(measurements) = times.get(operation) {
            if !measurements.is_empty() {
                let total: Duration = measurements.iter().sum();
                Some(total / measurements.len() as u32)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Get error count
    pub async fn get_error_count(&self, operation: &str) -> u64 {
        let errors = self.error_counts.read().await;
        errors.get(operation).copied().unwrap_or(0)
    }

    /// Get all metrics
    pub async fn get_all_metrics(&self) -> MetricsSnapshot {
        let times = self.operation_times.read().await;
        let errors = self.error_counts.read().await;

        let mut operation_averages = HashMap::new();
        for (operation, measurements) in times.iter() {
            if !measurements.is_empty() {
                let total: Duration = measurements.iter().sum();
                let average = total / measurements.len() as u32;
                operation_averages.insert(operation.clone(), average);
            }
        }

        MetricsSnapshot {
            operation_averages,
            error_counts: errors.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub operation_averages: HashMap<String, Duration>,
    pub error_counts: HashMap<String, u64>,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::sleep;

    use super::*;

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache = Cache::new(Duration::from_secs(1), 10);

        // Test set and get
        cache.set("key1".to_string(), "value1".to_string()).await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some("value1".to_string()));

        // Test cache miss
        assert_eq!(cache.get(&"nonexistent".to_string()).await, None);
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        // Use a longer TTL to avoid flaky failures when the test runtime is
        // contended — between set().await and get().await other tasks may run
        // and consume the TTL window (both methods acquire write locks).
        let cache = Cache::new(Duration::from_secs(1), 10);

        cache.set("key1".to_string(), "value1".to_string()).await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some("value1".to_string()));

        // Wait for expiration
        sleep(Duration::from_secs(2)).await;
        assert_eq!(cache.get(&"key1".to_string()).await, None);
    }

    #[tokio::test]
    async fn test_cache_get_or_compute() {
        let cache = Cache::new(Duration::from_secs(1), 10);

        let result = cache
            .get_or_compute("key1".to_string(), || async {
                Ok::<String, String>("computed_value".to_string())
            })
            .await;

        assert_eq!(result, Ok("computed_value".to_string()));

        // Should get from cache on second call
        let result2 = cache
            .get_or_compute("key1".to_string(), || async {
                Ok::<String, String>("should_not_compute".to_string())
            })
            .await;

        assert_eq!(result2, Ok("computed_value".to_string()));
    }

    

    

    #[tokio::test]
    async fn test_performance_metrics() {
        let metrics = PerformanceMetrics::new();

        metrics
            .record_operation_time("test_op", Duration::from_millis(100))
            .await;
        metrics
            .record_operation_time("test_op", Duration::from_millis(200))
            .await;
        metrics.record_error("test_op").await;

        let avg_time = metrics.get_average_time("test_op").await;
        assert_eq!(avg_time, Some(Duration::from_millis(150)));

        let error_count = metrics.get_error_count("test_op").await;
        assert_eq!(error_count, 1);
    }
}
