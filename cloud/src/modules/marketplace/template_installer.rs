use std::{path::PathBuf, sync::Arc};

use super::{
    client::MarketplaceClient,
    error::{MarketplaceError, Result},
};
use crate::{modules::template::TemplateRepository, shared::utils::sanitize_filename};

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

    /// Install template from marketplace.
    /// Fetches the template definition directly from the marketplace API.
    pub async fn install_from_marketplace(
        &self,
        template_id: &str,
        version: Option<&str>,
    ) -> Result<String> {
        tracing::info!("Installing template {} from marketplace", template_id);

        // 1. Fetch template from marketplace API
        let template_data = self.client.fetch_template(template_id).await?;

        // 2. Validate required fields before writing to disk
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

        // 4. Write template data to temp file
        let temp_file = self.write_temp_file(&template_data, template_id).await?;

        // 5. Save to templates/custom/ directory
        let dest_file = self.save_template(&temp_file, template_id).await?;

        // 6. Import to database
        self.import_template(&dest_file).await?;

        tracing::info!("Successfully installed template: {}", template_id);
        Ok(template_id.to_string())
    }

    /// Write template JSON to a temp file.
    async fn write_temp_file(
        &self,
        data: &serde_json::Value,
        template_id: &str,
    ) -> Result<PathBuf> {
        let temp_dir = std::env::temp_dir();
        let safe_id = sanitize_filename(template_id);
        let temp_file = temp_dir.join(format!("template_{}.json", safe_id));

        let json_bytes = serde_json::to_vec_pretty(data).map_err(|e| {
            MarketplaceError::Template(format!("Failed to serialize template: {}", e))
        })?;
        tokio::fs::write(&temp_file, &json_bytes).await?;

        Ok(temp_file)
    }

    /// Save template to target directory.
    async fn save_template(&self, temp_file: &PathBuf, template_id: &str) -> Result<PathBuf> {
        let custom_dir = self.templates_dir.join("custom");
        tokio::fs::create_dir_all(&custom_dir).await?;

        let safe_id = sanitize_filename(template_id);
        let dest_file = custom_dir.join(format!("{}.json", safe_id));

        // Verify the resolved path is still within the intended directory
        if !dest_file.starts_with(&custom_dir) {
            return Err(MarketplaceError::Template(
                "Invalid template ID: path traversal detected".to_string(),
            ));
        }

        tokio::fs::copy(temp_file, &dest_file).await?;

        // Delete temporary file
        let _ = tokio::fs::remove_file(temp_file).await;

        Ok(dest_file)
    }

    /// Import template to database.
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
