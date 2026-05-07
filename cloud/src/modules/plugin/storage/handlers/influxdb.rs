//! InfluxDB 存储处理器

use std::any::Any;

use async_trait::async_trait;
use futures::stream::iter;
use influxdb2::{Client as InfluxClient, models::DataPoint};
use tracing::debug;

use super::{super::config::InfluxdbConfig, StorageHandler};
use crate::{
    modules::plugin::{PluginHandler, PluginManifest, PluginType, storage::StorageData},
    shared::error::Error,
};

pub struct InfluxdbHandler {
    config: InfluxdbConfig,
    client: InfluxClient,
    manifest: PluginManifest,
}

impl InfluxdbHandler {
    pub fn new(config: InfluxdbConfig) -> Self {
        let client = InfluxClient::new(&config.url, &config.org, &config.token);
        Self {
            config,
            client,
            manifest: PluginManifest {
                name: "influxdb".to_string(),
                version: Some("1.0.0".to_string()),
                plugin_type: PluginType::Storage,
                description: Some("InfluxDB storage handler".to_string()),
            },
        }
    }
}

#[async_trait]
impl StorageHandler for InfluxdbHandler {
    async fn write(&self, data: &StorageData) -> Result<(), Error> {
        debug!("Writing {} values to InfluxDB for device {}", data.values.len(), data.device_id);

        let measurement = self.config.measurement.as_deref().unwrap_or("device_data");

        let mut point = DataPoint::builder(measurement)
            .tag("device_id", &data.device_id)
            .field("timestamp", data.timestamp as f64);

        for (key, value) in &data.values {
            match value {
                serde_json::Value::Number(n) => {
                    if let Some(f) = n.as_f64() {
                        point = point.field(key, f);
                    }
                }
                serde_json::Value::Bool(b) => {
                    point = point.field(key, *b);
                }
                serde_json::Value::String(s) => {
                    point = point.field(key, s.as_str());
                }
                _ => {
                    point = point.field(key, value.to_string());
                }
            }
        }

        point = point.timestamp(data.timestamp);

        // Convert Vec to Stream using futures::stream::iter
        let data_points = vec![point.build().unwrap()];
        self.client
            .write(&self.config.bucket, iter(data_points))
            .await
            .map_err(|e| Error::DatabaseError(format!("Failed to write to InfluxDB: {}", e)))?;

        Ok(())
    }

    fn name(&self) -> &str {
        "InfluxdbHandler"
    }
}

impl PluginHandler for InfluxdbHandler {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn plugin_type(&self) -> PluginType {
        self.manifest.plugin_type
    }
}
