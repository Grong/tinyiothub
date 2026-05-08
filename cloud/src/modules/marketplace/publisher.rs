// cloud/src/modules/marketplace/publisher.rs

use reqwest::Client;
use tinyiothub_config::MarketplaceConfig;

use super::error::{MarketplaceError, Result};
use crate::modules::template::types::DeviceTemplate;

/// Publishes templates and drivers to the Marketplace.
pub struct MarketplacePublisher {
    client: Client,
    base_url: String,
    api_key: String,
}

impl MarketplacePublisher {
    pub fn new(config: &MarketplaceConfig) -> Result<Self> {
        let base_url = config
            .api_url
            .as_ref()
            .ok_or_else(|| MarketplaceError::InvalidConfig("marketplace api_url not set".into()))?
            .clone();
        let api_key = config
            .api_key
            .as_ref()
            .ok_or_else(|| MarketplaceError::InvalidConfig("marketplace api_key not set".into()))?
            .clone();

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(MarketplaceError::Network)?;

        Ok(Self { client, base_url, api_key })
    }

    /// Publish a device template to the Marketplace.
    pub async fn publish_template(&self, template: &DeviceTemplate) -> Result<serde_json::Value> {
        let url = format!("{}/templates", self.base_url.trim_end_matches('/'));

        let tags: Vec<String> = template.get_tags();

        let body = serde_json::json!({
            "name": template.name,
            "version": template.version,
            "description": template.description,
            "category": template.category,
            "tags": tags,
            "content": {
                "name": template.name,
                "display_name": template.display_name,
                "description": template.description,
                "version": template.version,
                "category": template.category,
                "manufacturer": template.manufacturer,
                "device_type": template.device_type,
                "protocol_type": template.protocol_type,
                "driver_name": template.driver_name,
                "tags": template.tags,
                "device_info": template.device_info,
                "properties": template.properties,
                "commands": template.commands,
            }
        });

        let response = self
            .client
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(MarketplaceError::Network)?;

        let status = response.status();
        let body_text = response.text().await.map_err(MarketplaceError::Network)?;

        if !status.is_success() {
            return Err(MarketplaceError::Driver(format!(
                "marketplace returned {}: {}",
                status, body_text
            )));
        }

        let value: serde_json::Value =
            serde_json::from_str(&body_text).map_err(MarketplaceError::JsonParse)?;
        Ok(value)
    }
}
