// Performance optimization utilities for improved user experience
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
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

            info!(
                "Evicted {} LRU cache entries",
                cache.len() - self.max_size + 1
            );
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

/// Optimized string handling to reduce allocations
pub struct StringOptimizer;

impl StringOptimizer {
    /// Convert to Cow<str> to avoid unnecessary cloning
    pub fn to_cow(s: &str) -> Cow<'_, str> {
        Cow::Borrowed(s)
    }

    /// Convert owned string to Cow<str>
    pub fn owned_to_cow(s: String) -> Cow<'static, str> {
        Cow::Owned(s)
    }

    /// Efficient string concatenation for small strings
    pub fn concat_small(parts: &[&str]) -> String {
        let total_len: usize = parts.iter().map(|s| s.len()).sum();
        let mut result = String::with_capacity(total_len);
        for part in parts {
            result.push_str(part);
        }
        result
    }

    /// Efficient string formatting with pre-allocated capacity
    pub fn format_with_capacity(
        capacity: usize,
        template: &str,
        replacements: &[(&str, &str)],
    ) -> String {
        let mut result = String::with_capacity(capacity);
        result.push_str(template);

        for (placeholder, replacement) in replacements {
            if let Some(pos) = result.find(placeholder) {
                result.replace_range(pos..pos + placeholder.len(), replacement);
            }
        }

        result
    }
}

/// Batch processing utilities for improved performance
pub struct BatchProcessor<T> {
    batch_size: usize,
    timeout: Duration,
    buffer: Vec<T>,
    last_flush: Instant,
}

impl<T> BatchProcessor<T> {
    /// Create a new batch processor
    pub fn new(batch_size: usize, timeout: Duration) -> Self {
        Self {
            batch_size,
            timeout,
            buffer: Vec::with_capacity(batch_size),
            last_flush: Instant::now(),
        }
    }

    /// Add item to batch, returns true if batch should be flushed
    pub fn add(&mut self, item: T) -> bool {
        self.buffer.push(item);

        // Check if we should flush
        self.buffer.len() >= self.batch_size || self.last_flush.elapsed() >= self.timeout
    }

    /// Get current batch and reset buffer
    pub fn flush(&mut self) -> Vec<T> {
        let batch = std::mem::replace(&mut self.buffer, Vec::with_capacity(self.batch_size));
        self.last_flush = Instant::now();
        batch
    }

    /// Check if batch should be flushed due to timeout
    pub fn should_flush_timeout(&self) -> bool {
        !self.buffer.is_empty() && self.last_flush.elapsed() >= self.timeout
    }

    /// Get current buffer size
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

/// Connection pooling for database operations
pub struct ConnectionPool<T> {
    connections: Arc<RwLock<Vec<T>>>,
    max_size: usize,
    current_size: Arc<RwLock<usize>>,
}

impl<T> ConnectionPool<T>
where
    T: Clone,
{
    /// Create a new connection pool
    pub fn new(max_size: usize) -> Self {
        Self {
            connections: Arc::new(RwLock::new(Vec::with_capacity(max_size))),
            max_size,
            current_size: Arc::new(RwLock::new(0)),
        }
    }

    /// Get a connection from the pool
    pub async fn get(&self) -> Option<T> {
        let mut connections = self.connections.write().await;
        connections.pop()
    }

    /// Return a connection to the pool
    pub async fn return_connection(&self, connection: T) {
        let mut connections = self.connections.write().await;
        if connections.len() < self.max_size {
            connections.push(connection);
        }
    }

    /// Get pool statistics
    pub async fn stats(&self) -> PoolStats {
        let connections = self.connections.read().await;
        let current_size = *self.current_size.read().await;

        PoolStats {
            available: connections.len(),
            total: current_size,
            max_size: self.max_size,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub available: usize,
    pub total: usize,
    pub max_size: usize,
}

/// Memory-efficient data structures for common use cases
pub mod efficient_collections {
    use smallvec::SmallVec;
    use std::collections::HashMap;

    /// Small vector that stores up to N elements on the stack
    pub type SmallStringVec<const N: usize> = SmallVec<[String; N]>;

    /// Efficient map for small number of entries
    pub type SmallMap<K, V> = HashMap<K, V>;

    /// Pre-allocated string for common operations
    pub struct PreAllocatedString {
        buffer: String,
    }

    impl PreAllocatedString {
        pub fn new(capacity: usize) -> Self {
            Self {
                buffer: String::with_capacity(capacity),
            }
        }

        pub fn format(&mut self, template: &str, args: &[&str]) -> &str {
            self.buffer.clear();
            self.buffer.push_str(template);

            for (i, arg) in args.iter().enumerate() {
                let placeholder = format!("{{{}}}", i);
                if let Some(pos) = self.buffer.find(&placeholder) {
                    self.buffer.replace_range(pos..pos + placeholder.len(), arg);
                }
            }

            &self.buffer
        }

        pub fn as_str(&self) -> &str {
            &self.buffer
        }
    }
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
        if let Some(measurements) = times.get_mut(operation) {
            if measurements.len() > 100 {
                measurements.drain(0..measurements.len() - 100);
            }
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
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache = Cache::new(Duration::from_secs(1), 10);

        // Test set and get
        cache.set("key1".to_string(), "value1".to_string()).await;
        assert_eq!(
            cache.get(&"key1".to_string()).await,
            Some("value1".to_string())
        );

        // Test cache miss
        assert_eq!(cache.get(&"nonexistent".to_string()).await, None);
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let cache = Cache::new(Duration::from_millis(50), 10);

        cache.set("key1".to_string(), "value1".to_string()).await;
        assert_eq!(
            cache.get(&"key1".to_string()).await,
            Some("value1".to_string())
        );

        // Wait for expiration
        sleep(Duration::from_millis(100)).await;
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

    #[test]
    fn test_batch_processor() {
        let mut processor = BatchProcessor::new(3, Duration::from_secs(1));

        assert!(!processor.add("item1"));
        assert!(!processor.add("item2"));
        assert!(processor.add("item3")); // Should trigger flush

        let batch = processor.flush();
        assert_eq!(batch.len(), 3);
        assert!(processor.is_empty());
    }

    #[test]
    fn test_string_optimizer() {
        let parts = ["Hello", " ", "World", "!"];
        let result = StringOptimizer::concat_small(&parts);
        assert_eq!(result, "Hello World!");

        let formatted =
            StringOptimizer::format_with_capacity(20, "Hello {name}!", &[("{name}", "Rust")]);
        assert_eq!(formatted, "Hello Rust!");
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
