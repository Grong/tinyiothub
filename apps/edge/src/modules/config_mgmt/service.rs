use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::EdgeConfig;
use crate::shared::error::EdgeResult;
use tinyiothub_storage::sqlite::Database;

pub struct ConfigService {
    db: Arc<Database>,
    config: EdgeConfig,
    merged: RwLock<HashMap<String, serde_json::Value>>,
}

impl ConfigService {
    pub fn new(db: Arc<Database>, config: EdgeConfig) -> Arc<Self> {
        Arc::new(Self {
            db,
            config,
            merged: RwLock::new(HashMap::new()),
        })
    }

    pub async fn load_defaults(&self) {
        let mut merged = self.merged.write().await;
        merged
            .entry("telemetry_interval_secs".into())
            .or_insert(serde_json::Value::from(30));
        merged
            .entry("heartbeat_interval_secs".into())
            .or_insert(serde_json::Value::from(30));
        merged
            .entry("intelligence_interval_secs".into())
            .or_insert(serde_json::Value::from(60));
        merged
            .entry("scan_timeout_secs".into())
            .or_insert(serde_json::Value::from(10));
        merged
            .entry("offline_buffer_max_telemetry".into())
            .or_insert(serde_json::Value::from(100_000));
    }

    pub async fn sync_from_cloud(&self) -> EdgeResult<()> {
        // In production: fetch current config from cloud via GatewayService MQTT
        // For now, no-op — config comes via apply_cloud_config
        Ok(())
    }

    /// Get the current merged configuration (local defaults + cloud overrides)
    pub async fn get_merged_config(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let merged = self.merged.read().await;
        if merged.is_empty() {
            drop(merged);
            self.load_defaults().await;
            return Ok(self.merged.read().await.clone());
        }
        Ok(merged.clone())
    }

    /// Apply cloud config — last-write-wins merge
    pub async fn apply_cloud_config(&self, cloud: &serde_json::Value) -> EdgeResult<()> {
        {
            let mut merged = self.merged.write().await;
            if merged.is_empty() {
                drop(merged);
                self.load_defaults().await;
                let mut merged = self.merged.write().await;

                if let Some(obj) = cloud.as_object() {
                    for (k, v) in obj {
                        merged.insert(k.clone(), v.clone());
                    }
                }
            } else if let Some(obj) = cloud.as_object() {
                for (k, v) in obj {
                    merged.insert(k.clone(), v.clone());
                }
            }
        }

        // Atomic write: write to tmp file, then rename
        let tmp = self.config.config_file.with_extension("tmp");
        if let Some(parent) = self.config.config_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let merged = self.merged.read().await;
        let yaml = serde_yaml::to_string(&*merged)?;
        drop(merged);
        std::fs::write(&tmp, &yaml)?;
        std::fs::rename(&tmp, &self.config.config_file)?;

        Ok(())
    }

    /// Check if cloud version is newer than local version
    pub async fn cloud_version_is_newer(&self, cloud_version: &str) -> bool {
        let local = self.get_local_version().await.unwrap_or_default();
        cloud_version > local.as_str()
    }

    async fn get_local_version(&self) -> Option<String> {
        let pool = self.db.pool();
        sqlx::query_scalar::<_, String>("SELECT local_version FROM config_meta WHERE key = 'main'")
            .fetch_one(pool)
            .await
            .ok()
    }

    pub async fn set_local_version(&self, version: &str) {
        let pool = self.db.pool();
        sqlx::query("INSERT OR REPLACE INTO config_meta (key, local_version, updated_at) VALUES ('main', ?, ?)")
            .bind(version)
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(pool)
            .await
            .ok();
    }

    pub fn config_path(&self) -> &std::path::Path {
        &self.config.config_file
    }
}
