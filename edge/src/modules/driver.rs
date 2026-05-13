use std::sync::Arc;
use tinyiothub_storage::sqlite::Database;

pub struct DriverService {
    _db: Arc<Database>,
    _scan_timeout_secs: u64,
}

impl DriverService {
    pub fn new(db: Arc<Database>, scan_timeout_secs: u64) -> Arc<Self> {
        Arc::new(Self {
            _db: db,
            _scan_timeout_secs: scan_timeout_secs,
        })
    }
    pub async fn scan_all(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        Ok(Vec::new())
    }
}
