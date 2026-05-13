use std::sync::Arc;
use tinyiothub_storage::sqlite::Database;
use crate::config::EdgeConfig;

pub struct OfflineBuffer {
    _db: Arc<Database>,
    _config: EdgeConfig,
}

impl OfflineBuffer {
    pub fn new(db: Arc<Database>, config: EdgeConfig) -> Arc<Self> {
        Arc::new(Self {
            _db: db,
            _config: config,
        })
    }
    pub async fn flush_batch(
        &self,
        _batch_size: usize,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(0)
    }
}
