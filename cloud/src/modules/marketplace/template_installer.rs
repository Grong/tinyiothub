use std::sync::Arc;

use super::{
    client::MarketplaceClient,
    error::{MarketplaceError, Result},
};
use crate::modules::template::TemplateRepository;

pub struct TemplateInstaller {
    client: Arc<MarketplaceClient>,
    repository: Arc<TemplateRepository>,
}

impl TemplateInstaller {
    pub fn new(client: Arc<MarketplaceClient>, repository: Arc<TemplateRepository>) -> Self {
        Self { client, repository }
    }

    /// Install template from marketplace.
    /// Fetches the template definition from the marketplace API and imports directly into the database.
    pub async fn install_from_marketplace(
        &self,
        template_id: &str,
        version: Option<&str>,
    ) -> Result<String> {
        tracing::info!("Installing template {} from marketplace", template_id);

        // 1. Fetch template from marketplace API
        let template_data = self.client.fetch_template(template_id).await?;

        // 2. Validate required fields
        self.validate_template_structure(&template_data)?;

        // 3. Check version (if specified)
        if let Some(ver) = version {
            let api_version = template_data.get("version").and_then(|v| v.as_str()).unwrap_or("");
            if api_version != ver {
                return Err(MarketplaceError::NotFound(format!(
                    "Template {} version {}",
                    template_id, ver
                )));
            }
        }

        // 4. Import to database directly
        let request: crate::modules::template::types::CreateDeviceTemplateRequest =
            serde_json::from_value(template_data).map_err(|e| {
                MarketplaceError::Template(format!("Invalid template format: {}", e))
            })?;

        self.repository
            .create(&request)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?;

        tracing::info!("Successfully installed template: {}", template_id);
        Ok(template_id.to_string())
    }

    /// Validate that fetched template data has the required structure.
    fn validate_template_structure(&self, data: &serde_json::Value) -> Result<()> {
        let required = ["name", "category", "version"];
        let mut missing = Vec::new();
        for field in &required {
            if data.get(field).is_none() {
                missing.push(*field);
            }
        }
        if !missing.is_empty() {
            return Err(MarketplaceError::Template(format!(
                "Template missing required fields: {}",
                missing.join(", ")
            )));
        }
        Ok(())
    }

    /// Check if template is installed.
    pub async fn is_installed(&self, template_id: &str) -> Result<bool> {
        match self.repository.find_by_id(template_id).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(MarketplaceError::Template(e.to_string())),
        }
    }

    /// Get installed template version.
    pub async fn get_installed_version(&self, template_id: &str) -> Result<Option<String>> {
        match self.repository.find_by_id(template_id).await {
            Ok(Some(template)) => Ok(Some(template.version)),
            Ok(None) => Ok(None),
            Err(e) => Err(MarketplaceError::Template(e.to_string())),
        }
    }
}
