use std::sync::Arc;
use tinyiothub_storage::sqlite::Database;
use crate::config::EdgeConfig;
use super::gateway::GatewayService;

pub struct ConfigService;

impl ConfigService {
    pub fn new(
        _db: Arc<Database>,
        _config: EdgeConfig,
        _gateway: Arc<GatewayService>,
    ) -> Arc<Self> {
        Arc::new(Self)
    }
    pub async fn sync_from_cloud(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
    pub async fn load_defaults(&self) {}
    pub async fn cloud_version_is_newer(&self, _cloud_version: &str) -> bool {
        true
    }
}
