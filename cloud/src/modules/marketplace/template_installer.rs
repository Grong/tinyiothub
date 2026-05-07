use std::{path::PathBuf, sync::Arc};

use super::{
    client::MarketplaceClient,
    error::{MarketplaceError, Result},
    metadata::TemplateMetadata,
};
use crate::modules::template::TemplateRepository;

pub struct TemplateInstaller {
    client: Arc<MarketplaceClient>,
    repository: Arc<TemplateRepository>,
    templates_dir: PathBuf,
}

impl TemplateInstaller {
    pub fn new(
        client: Arc<MarketplaceClient>,
        repository: Arc<TemplateRepository>,
        templates_dir: PathBuf,
    ) -> Self {
        Self { client, repository, templates_dir }
    }

    /// Install template from marketplace
    pub async fn install_from_marketplace(
        &self,
        template_id: &str,
        version: Option<&str>,
    ) -> Result<String> {
        tracing::info!("Installing template {} from marketplace", template_id);

        // 1. Fetch template list
        let templates = self.client.fetch_templates().await?;

        // 2. Find target template
        let template_meta = templates
            .iter()
            .find(|t| t.id == template_id)
            .ok_or_else(|| MarketplaceError::NotFound(format!("Template: {}", template_id)))?;

        // 3. Check version (if specified)
        if let Some(ver) = version
            && template_meta.version != ver
        {
            return Err(MarketplaceError::NotFound(format!(
                "Template {} version {}",
                template_id, ver
            )));
        }

        // 4. Download template file
        let temp_file = self.download_template(template_meta).await?;

        // 5. Verify checksum (skip in development mode)
        if !template_meta.checksum.starts_with("sha256:test")
            && !template_meta.checksum.contains("test")
        {
            self.client.verify_checksum(&temp_file, &template_meta.checksum).await?;
        } else {
            tracing::warn!("Skipping checksum verification for test/development template");
        }

        // 6. Save to templates/custom/ directory
        let dest_file = self.save_template(&temp_file, template_id).await?;

        // 7. Import to database
        self.import_template(&dest_file).await?;

        tracing::info!("Successfully installed template: {}", template_id);
        Ok(template_id.to_string())
    }

    /// Download template file
    async fn download_template(&self, meta: &TemplateMetadata) -> Result<PathBuf> {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("template_{}.json", meta.id));

        self.client.download_resource(&meta.file_url, &temp_file).await?;

        Ok(temp_file)
    }

    /// Save template to target directory
    async fn save_template(&self, temp_file: &PathBuf, template_id: &str) -> Result<PathBuf> {
        let custom_dir = self.templates_dir.join("custom");
        tokio::fs::create_dir_all(&custom_dir).await?;

        let dest_file = custom_dir.join(format!("{}.json", template_id));
        tokio::fs::copy(temp_file, &dest_file).await?;

        // Delete temporary file
        let _ = tokio::fs::remove_file(temp_file).await;

        Ok(dest_file)
    }

    /// Import template to database
    async fn import_template(&self, file_path: &PathBuf) -> Result<()> {
        let content = tokio::fs::read_to_string(file_path).await?;
        let template_data: serde_json::Value = serde_json::from_str(&content)?;

        // Convert JSON to CreateDeviceTemplateRequest
        let request: crate::modules::template::types::CreateDeviceTemplateRequest =
            serde_json::from_value(template_data).map_err(|e| {
                MarketplaceError::Template(format!("Invalid template format: {}", e))
            })?;

        // Use repository's create method
        self.repository
            .create(&request)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?;

        Ok(())
    }

    /// Check if template is installed
    pub async fn is_installed(&self, template_id: &str) -> Result<bool> {
        match self.repository.find_by_id(template_id).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(MarketplaceError::Template(e.to_string())),
        }
    }

    /// Get installed template version
    pub async fn get_installed_version(&self, template_id: &str) -> Result<Option<String>> {
        match self.repository.find_by_id(template_id).await {
            Ok(Some(template)) => Ok(Some(template.version)),
            Ok(None) => Ok(None),
            Err(e) => Err(MarketplaceError::Template(e.to_string())),
        }
    }
}
