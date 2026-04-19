//! In-memory repository implementations for testing.
//!
//! These implementations store data in `std::collections::HashMap`
//! and are useful for unit tests that don't need a real database.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// An in-memory data store backed by a `HashMap`.
#[derive(Debug, Clone)]
pub struct MemoryStore<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    data: Arc<RwLock<HashMap<K, V>>>,
}

impl<K, V> Default for MemoryStore<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    fn default() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl<K, V> MemoryStore<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, key: K, value: V) -> Option<V> {
        self.data.write().unwrap().insert(key, value)
    }

    pub fn get(&self, key: &K) -> Option<V> {
        self.data.read().unwrap().get(key).cloned()
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        self.data.write().unwrap().remove(key)
    }

    pub fn len(&self) -> usize {
        self.data.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.read().unwrap().is_empty()
    }

    pub fn clear(&self) {
        self.data.write().unwrap().clear();
    }

    pub fn values(&self) -> Vec<V> {
        self.data.read().unwrap().values().cloned().collect()
    }

    pub fn keys(&self) -> Vec<K> {
        self.data.read().unwrap().keys().cloned().collect()
    }
}
