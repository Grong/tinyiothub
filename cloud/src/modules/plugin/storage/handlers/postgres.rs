//! PostgreSQL 存储处理器

use std::any::Any;
use async_trait::async_trait;
use tokio_postgres::NoTls;
use tracing::{debug, error};

use super::StorageHandler;
use crate::modules::plugin::storage::StorageData;
use crate::shared::error::Error;

use super::super::config::PostgresConfig;
use crate::modules::plugin::{PluginHandler, PluginManifest, PluginType};

pub struct PostgresHandler {
    config: PostgresConfig,
    client: tokio_postgres::Client,
    manifest: PluginManifest,
}

impl PostgresHandler {
    pub async fn new(config: PostgresConfig) -> Result<Self, Error> {
        let (client, connection) = tokio_postgres::connect(&config.connection_string, NoTls)
            .await
            .map_err(|e| Error::DatabaseError(format!("Failed to connect to Postgres: {}", e)))?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("Postgres connection error: {}", e);
            }
        });

        Ok(Self {
            config,
            client,
            manifest: PluginManifest {
                name: "postgres".to_string(),
                version: Some("1.0.0".to_string()),
                plugin_type: PluginType::Storage,
                description: Some("PostgreSQL storage handler".to_string()),
            },
        })
    }
}

#[async_trait]
impl StorageHandler for PostgresHandler {
    async fn write(&self, data: &StorageData) -> Result<(), Error> {
        debug!("Writing {} values to Postgres for device {}", data.values.len(), data.device_id);

        let query = format!(
            "INSERT INTO {} (device_id, timestamp, data) VALUES ($1, $2, $3)",
            self.config.table_name
        );

        let data_json = serde_json::to_string(&data.values)
            .map_err(|e| Error::SerializationError(format!("Failed to serialize data: {}", e)))?;

        self.client.execute(&query, &[&data.device_id, &data.timestamp, &data_json])
            .await
            .map_err(|e| Error::DatabaseError(format!("Failed to write to Postgres: {}", e)))?;

        Ok(())
    }

    fn name(&self) -> &str {
        "PostgresHandler"
    }
}

impl PluginHandler for PostgresHandler {
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
