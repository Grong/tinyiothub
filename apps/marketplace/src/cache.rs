use parking_lot::RwLock;
use sled::{Db, IVec};
use std::sync::Arc;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("sled error: {0}")]
    Sled(#[from] sled::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

const SYNC_LOCK_TTL_SECS: i64 = 600;

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
        Ok(Self {
            db: Arc::new(RwLock::new(db)),
        })
    }

    fn get(&self, key: &str) -> Result<Option<IVec>, CacheError> {
        let db = self.db.read();
        Ok(db.get(key)?)
    }

    fn set(&self, key: &str, value: &[u8]) -> Result<(), CacheError> {
        let db = self.db.read();
        db.insert(key, value)?;
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

        let existing = db.get(Self::SYNC_LOCK)?;
        if let Some(ref existing_bytes) = existing {
            let lock: SyncLock = serde_json::from_slice(existing_bytes)?;
            if now - lock.ts < SYNC_LOCK_TTL_SECS {
                return Ok(false);
            }
        }

        let lock = SyncLock {
            holder_id: holder_id.to_string(),
            ts: now,
        };
        let new_json = serde_json::to_vec(&lock)?;

        // Atomic compare-and-swap: only write if the old value hasn't changed
        match db.compare_and_swap(Self::SYNC_LOCK, existing.as_deref(), Some(new_json.as_slice()))? {
            Ok(()) => Ok(true),
            Err(_) => Ok(false), // CAS failed — another caller won the race
        }
    }

    pub fn release_sync_lock(&self, holder_id: &str) -> Result<(), CacheError> {
        let db = self.db.read();
        if let Some(existing) = db.get(Self::SYNC_LOCK)? {
            let lock: SyncLock = serde_json::from_slice(&existing)?;
            if lock.holder_id == holder_id {
                db.remove(Self::SYNC_LOCK)?;
            }
        }
        Ok(())
    }

    // ── Idempotency ────────────────────────────────────

    pub fn check_idempotency(&self, delivery_id: &str) -> Result<bool, CacheError> {
        let key = format!("{}{}", Self::IDEMPOTENCY_PREFIX, delivery_id);
        let db = self.db.read();
        let marker = serde_json::to_vec(&serde_json::json!({"ts": chrono::Utc::now().timestamp()}))?;

        // Atomic: only insert if key doesn't exist
        match db.compare_and_swap(&key, None as Option<&[u8]>, Some(marker.as_slice()))? {
            Ok(()) => Ok(false), // CAS succeeded — first time seeing this delivery_id
            Err(_) => Ok(true),  // CAS failed — key already exists, already processed
        }
    }

    pub fn flush(&self) -> Result<(), CacheError> {
        let db = self.db.read();
        db.flush()?;
        Ok(())
    }

    // ── Cache health ───────────────────────────────────

    pub fn is_cold(&self) -> bool {
        !self.contains_key(Self::TEMPLATES_INDEX) && !self.contains_key(Self::DRIVERS_INDEX)
    }
}
