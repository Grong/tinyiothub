use sled::{Db, IVec};
use std::sync::Arc;
use parking_lot::RwLock;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("sled error: {0}")]
    Sled(#[from] sled::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct SledCache {
    db: Arc<RwLock<Db>>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SyncLock {
    pub holder_id: String,
    pub ts: i64,
}

impl SledCache {
    const TEMPLATES_INDEX: &'static str = "templates_index";
    const DRIVERS_INDEX: &'static str = "drivers_index";
    const LAST_SYNC: &'static str = "last_sync";
    const SYNC_LOCK: &'static str = "sync_lock";
    const IDEMPOTENCY_PREFIX: &'static str = "idempotency:";

    pub fn new(path: impl Into<String>) -> Result<Self, CacheError> {
        let path = path.into();
        info!("Opening Sled cache at {}", path);
        let db = sled::open(&path)?;
        Ok(Self { db: Arc::new(RwLock::new(db)) })
    }

    fn get(&self, key: &str) -> Result<Option<IVec>, CacheError> {
        let db = self.db.read();
        Ok(db.get(key)?)
    }

    fn set(&self, key: &str, value: &[u8]) -> Result<(), CacheError> {
        let db = self.db.read();
        db.insert(key, value)?;
        db.flush()?;
        Ok(())
    }

    fn contains_key(&self, key: &str) -> bool {
        let db = self.db.read();
        db.contains_key(key).unwrap_or(false)
    }

    // ── Templates ──────────────────────────────────────

    pub fn get_templates(&self) -> Result<Option<Vec<serde_json::Value>>, CacheError> {
        match self.get(Self::TEMPLATES_INDEX)? {
            Some(v) => Ok(Some(serde_json::from_slice(&v)?)),
            None => Ok(None),
        }
    }

    pub fn set_templates(&self, templates: &[serde_json::Value]) -> Result<(), CacheError> {
        let json = serde_json::to_vec(templates)?;
        self.set(Self::TEMPLATES_INDEX, &json)
    }

    // ── Drivers ────────────────────────────────────────

    pub fn get_drivers(&self) -> Result<Option<Vec<serde_json::Value>>, CacheError> {
        match self.get(Self::DRIVERS_INDEX)? {
            Some(v) => Ok(Some(serde_json::from_slice(&v)?)),
            None => Ok(None),
        }
    }

    pub fn set_drivers(&self, drivers: &[serde_json::Value]) -> Result<(), CacheError> {
        let json = serde_json::to_vec(drivers)?;
        self.set(Self::DRIVERS_INDEX, &json)
    }

    // ── Last Sync ──────────────────────────────────────

    pub fn get_last_sync(&self) -> Result<Option<i64>, CacheError> {
        match self.get(Self::LAST_SYNC)? {
            Some(v) => Ok(Some(serde_json::from_slice(&v)?)),
            None => Ok(None),
        }
    }

    pub fn set_last_sync(&self, ts: i64) -> Result<(), CacheError> {
        let json = serde_json::to_vec(&ts)?;
        self.set(Self::LAST_SYNC, &json)
    }

    // ── Sync Lock ──────────────────────────────────────

    pub fn acquire_sync_lock(&self, holder_id: &str) -> Result<bool, CacheError> {
        let now = chrono::Utc::now().timestamp();
        let db = self.db.read();

        if let Some(existing) = db.get(Self::SYNC_LOCK)? {
            let lock: SyncLock = serde_json::from_slice(&existing)?;
            if now - lock.ts < 600 {
                return Ok(false);
            }
        }

        drop(db);

        let lock = SyncLock { holder_id: holder_id.to_string(), ts: now };
        let json = serde_json::to_vec(&lock)?;
        let db = self.db.read();
        db.insert(Self::SYNC_LOCK, json)?;
        db.flush()?;
        Ok(true)
    }

    pub fn release_sync_lock(&self, holder_id: &str) -> Result<(), CacheError> {
        let db = self.db.read();
        if let Some(existing) = db.get(Self::SYNC_LOCK)? {
            let lock: SyncLock = serde_json::from_slice(&existing)?;
            if lock.holder_id == holder_id {
                db.remove(Self::SYNC_LOCK)?;
                db.flush()?;
            }
        }
        Ok(())
    }

    // ── Idempotency ────────────────────────────────────

    pub fn check_idempotency(&self, delivery_id: &str) -> Result<bool, CacheError> {
        let key = format!("{}{}", Self::IDEMPOTENCY_PREFIX, delivery_id);
        if self.contains_key(&key) {
            return Ok(true);
        }
        let json = serde_json::to_vec(&serde_json::json!({"ts": chrono::Utc::now().timestamp()}))?;
        self.set(&key, &json)?;
        Ok(false)
    }

    // ── Cache health ───────────────────────────────────

    pub fn is_cold(&self) -> bool {
        !self.contains_key(Self::TEMPLATES_INDEX) && !self.contains_key(Self::DRIVERS_INDEX)
    }
}
