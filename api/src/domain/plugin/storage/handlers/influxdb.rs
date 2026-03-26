//! InfluxDB 存储处理器

use async_trait::async_trait;
use influxdb2::Client as InfluxClient;
use influxdb2::models::DataPoint;
use tracing::debug;

use super::StorageHandler;
use crate::domain::plugin::storage::StorageData;
use crate::shared::error::Error;

use super::super::config::InfluxdbConfig;

pub struct InfluxdbHandler {
    config: InfluxdbConfig,
    client: InfluxClient,
}

impl InfluxdbHandler {
    pub fn new(config: InfluxdbConfig) -> Self {
        let client = InfluxClient::new(&config.url, &config.org, &config.token);
        Self { config, client }
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

        self.client.write(&self.config.bucket, vec![point.build().unwrap()])
            .await
            .map_err(|e| Error::DatabaseError(format!("Failed to write to InfluxDB: {}", e)))?;

        Ok(())
    }

    fn name(&self) -> &str {
        "InfluxdbHandler"
    }
}
