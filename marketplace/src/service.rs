use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, warn};

use crate::cache::SledCache;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("sync failed: {0}")]
    Failed(String),
}

pub struct SyncService {
    cache: Arc<SledCache>,
    data_path: PathBuf,
}

impl SyncService {
    pub fn new(cache: Arc<SledCache>, data_path: PathBuf) -> Self {
        Self { cache, data_path }
    }

    pub async fn load_local_data(&self) -> Result<(), SyncError> {
        info!("Loading local data from {:?}", self.data_path);

        let mut all_templates: Vec<Value> = Vec::new();
        let mut all_drivers: Vec<Value> = Vec::new();

        let templates_dir = self.data_path.join("templates");
        if templates_dir.is_dir() {
            match tokio::fs::read_dir(&templates_dir).await {
                Ok(mut entries) => {
                    while let Some(entry) = entries
                        .next_entry()
                        .await
                        .map_err(|e| SyncError::Failed(e.to_string()))?
                    {
                        let path = entry.path();
                        if path.extension().and_then(|s| s.to_str()) == Some("json") {
                            match tokio::fs::read_to_string(&path).await {
                                Ok(content) => match serde_json::from_str::<Value>(&content) {
                                    Ok(item) => all_templates.push(item),
                                    Err(e) => warn!("Failed to parse template {:?}: {}", path, e),
                                },
                                Err(e) => warn!("Failed to read template {:?}: {}", path, e),
                            }
                        }
                    }
                }
                Err(e) => warn!("Failed to read templates directory: {}", e),
            }
        } else {
            info!("Templates directory not found: {:?}", templates_dir);
        }

        let drivers_dir = self.data_path.join("drivers");
        if drivers_dir.is_dir() {
            match tokio::fs::read_dir(&drivers_dir).await {
                Ok(mut entries) => {
                    while let Some(entry) = entries
                        .next_entry()
                        .await
                        .map_err(|e| SyncError::Failed(e.to_string()))?
                    {
                        let path = entry.path();
                        if path.extension().and_then(|s| s.to_str()) == Some("json") {
                            match tokio::fs::read_to_string(&path).await {
                                Ok(content) => match serde_json::from_str::<Value>(&content) {
                                    Ok(item) => all_drivers.push(item),
                                    Err(e) => warn!("Failed to parse driver {:?}: {}", path, e),
                                },
                                Err(e) => warn!("Failed to read driver {:?}: {}", path, e),
                            }
                        }
                    }
                }
                Err(e) => warn!("Failed to read drivers directory: {}", e),
            }
        } else {
            info!("Drivers directory not found: {:?}", drivers_dir);
        }

        self.cache
            .set_templates(&all_templates)
            .map_err(|e| SyncError::Failed(format!("Failed to write templates to cache: {}", e)))?;
        self.cache
            .set_drivers(&all_drivers)
            .map_err(|e| SyncError::Failed(format!("Failed to write drivers to cache: {}", e)))?;

        let now = chrono::Utc::now().timestamp();
        self.cache
            .set_last_sync(now)
            .map_err(|e| SyncError::Failed(format!("Failed to update last_sync: {}", e)))?;

        // Batch flush after all writes complete
        self.cache
            .flush()
            .map_err(|e| SyncError::Failed(format!("Failed to flush cache: {}", e)))?;

        info!(
            "Local data load completed: {} templates, {} drivers",
            all_templates.len(),
            all_drivers.len()
        );
        Ok(())
    }
}
