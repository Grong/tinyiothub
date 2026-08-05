use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::RwLock;

const PAIRING_CODE_TTL: Duration = Duration::from_secs(300);
const MAX_ATTEMPTS_PER_USER: u32 = 5;
const DEFAULT_MAX_ENTRIES: usize = 10000;

pub struct PairingCache {
    entries: Arc<RwLock<HashMap<String, PairingEntry>>>,
    max_entries: usize,
}

#[derive(Debug, Clone)]
pub struct PairingEntry {
    pub fingerprint: String,
    pub hostname: String,
    pub os: String,
    pub ip: String,
    pub hw_model: String,
    pub created_at: Instant,
    pub attempts: HashMap<String, u32>,
}

impl PairingCache {
    pub fn new(max_entries: usize) -> Self {
        let cache = Self { entries: Arc::new(RwLock::new(HashMap::new())), max_entries };
        cache.spawn_cleanup_task();
        cache
    }

    pub async fn get(&self, code: &str) -> Option<PairingEntry> {
        let entries = self.entries.read().await;
        entries.get(code).and_then(|e| {
            if e.created_at.elapsed() > PAIRING_CODE_TTL { None } else { Some(e.clone()) }
        })
    }

    pub async fn try_insert(&self, code: String, entry: PairingEntry) -> bool {
        let mut entries = self.entries.write().await;
        if entries.len() >= self.max_entries {
            return false;
        }
        entries.insert(code, entry);
        true
    }

    pub async fn insert(&self, code: String, entry: PairingEntry) {
        let mut entries = self.entries.write().await;
        entries.insert(code, entry);
    }

    pub async fn remove(&self, code: &str) {
        let mut entries = self.entries.write().await;
        entries.remove(code);
    }

    pub async fn check_and_increment_attempts(&self, code: &str, user_id: &str) -> bool {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(code) {
            if entry.created_at.elapsed() > PAIRING_CODE_TTL {
                return false;
            }
            let count = entry.attempts.entry(user_id.to_string()).or_insert(0);
            if *count >= MAX_ATTEMPTS_PER_USER {
                return false;
            }
            *count += 1;
            true
        } else {
            false
        }
    }

    pub async fn is_full(&self) -> bool {
        let entries = self.entries.read().await;
        entries.len() >= self.max_entries
    }

    /// Returns true if the code exists in cache but its TTL has expired.
    /// Distinguishes "expired" from "never existed" for 410 Gone responses.
    pub async fn is_code_expired(&self, code: &str) -> bool {
        let entries = self.entries.read().await;
        match entries.get(code) {
            Some(entry) => entry.created_at.elapsed() > PAIRING_CODE_TTL,
            None => false,
        }
    }

    fn spawn_cleanup_task(&self) {
        let entries = Arc::clone(&self.entries);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let mut map = entries.write().await;
                map.retain(|_, entry| entry.created_at.elapsed() < PAIRING_CODE_TTL);
            }
        });
    }
}

impl Default for PairingCache {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_ENTRIES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry() -> PairingEntry {
        PairingEntry {
            fingerprint: "aa:bb:cc".into(),
            hostname: "gw-01".into(),
            os: "Linux".into(),
            ip: "192.168.1.1".into(),
            hw_model: "RPi5".into(),
            created_at: Instant::now(),
            attempts: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let cache = PairingCache::new(100);
        cache.insert("123456".into(), make_entry()).await;
        assert!(cache.get("123456").await.is_some());
        assert!(cache.get("999999").await.is_none());
    }

    #[tokio::test]
    async fn test_ttl_expiry() {
        let cache = PairingCache::new(100);
        let mut entry = make_entry();
        entry.created_at = Instant::now() - Duration::from_secs(301);
        cache.insert("123456".into(), entry).await;
        assert!(cache.get("123456").await.is_none());
    }

    #[tokio::test]
    async fn test_rate_limit_per_user() {
        let cache = PairingCache::new(100);
        cache.insert("123456".into(), make_entry()).await;
        for _ in 0..5 {
            assert!(cache.check_and_increment_attempts("123456", "user1").await);
        }
        assert!(!cache.check_and_increment_attempts("123456", "user1").await);
    }

    #[tokio::test]
    async fn test_remove() {
        let cache = PairingCache::new(100);
        cache.insert("123456".into(), make_entry()).await;
        cache.remove("123456").await;
        assert!(cache.get("123456").await.is_none());
    }

    #[tokio::test]
    async fn test_is_full() {
        let cache = PairingCache::new(2);
        assert!(cache.try_insert("111111".into(), make_entry()).await);
        assert!(cache.try_insert("222222".into(), make_entry()).await);
        assert!(cache.is_full().await);
        assert!(!cache.try_insert("333333".into(), make_entry()).await);
    }
}
