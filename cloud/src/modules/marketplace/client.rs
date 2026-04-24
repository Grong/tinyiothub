use std::{path::Path, time::Duration};

use reqwest::Client;
use sha2::{Digest, Sha256};

use super::{
    error::{MarketplaceError, Result},
    metadata::{DriverIndex, DriverMetadata, TemplateIndex, TemplateMetadata},
};
use tinyiothub_config::MarketplaceConfig;

pub struct MarketplaceClient {
    http_client: Client,
    config: MarketplaceConfig,
}

impl MarketplaceClient {
    pub fn new(config: MarketplaceConfig) -> Result<Self> {
        if !config.enabled {
            return Err(MarketplaceError::Disabled);
        }

        let http_client =
            Client::builder().timeout(Duration::from_secs(config.download_timeout_secs)).build()?;

        Ok(Self { http_client, config })
    }

    /// Fetch template list
    pub async fn fetch_templates(&self) -> Result<Vec<TemplateMetadata>> {
        let url = self.build_url("templates/index.json")?;
        tracing::info!("Fetching templates from: {}", url);

        // Check if it's a local file
        if url.starts_with("file://") || !url.starts_with("http") {
            let file_path = url.trim_start_matches("file://");
            let content = tokio::fs::read_to_string(file_path).await?;
            let index: TemplateIndex = serde_json::from_str(&content)?;
            return Ok(index.templates);
        }

        let response = self.http_client.get(&url).send().await?;
        let index: TemplateIndex = response.json().await?;

        Ok(index.templates)
    }

    /// Fetch driver list
    pub async fn fetch_drivers(&self) -> Result<Vec<DriverMetadata>> {
        let url = self.build_url("drivers/index.json")?;
        tracing::info!("Fetching drivers from: {}", url);

        // Check if it's a local file
        if url.starts_with("file://") || !url.starts_with("http") {
            let file_path = url.trim_start_matches("file://");
            let content = tokio::fs::read_to_string(file_path).await?;
            let index: DriverIndex = serde_json::from_str(&content)?;
            return Ok(index.drivers);
        }

        let response = self.http_client.get(&url).send().await?;
        let index: DriverIndex = response.json().await?;

        Ok(index.drivers)
    }

    /// Download resource file
    pub async fn download_resource(&self, url: &str, dest: &Path) -> Result<()> {
        tracing::info!("Downloading resource from {} to {:?}", url, dest);

        let response = self.http_client.get(url).send().await?;
        let bytes = response.bytes().await?;

        // Ensure target directory exists
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(dest, &bytes).await?;

        tracing::info!("Downloaded {} bytes to {:?}", bytes.len(), dest);
        Ok(())
    }

    /// Verify file checksum
    pub async fn verify_checksum(&self, file_path: &Path, expected: &str) -> Result<()> {
        let content = tokio::fs::read(file_path).await?;
        let actual = self.calculate_checksum(&content);

        if actual != expected {
            return Err(MarketplaceError::InvalidChecksum {
                expected: expected.to_string(),
                actual,
            });
        }

        Ok(())
    }

    /// Calculate file checksum
    fn calculate_checksum(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        format!("sha256:{}", hex::encode(result))
    }

    /// Build resource URL
    fn build_url(&self, path: &str) -> Result<String> {
        // Prefer API URL
        if let Some(api_url) = &self.config.api_url {
            return Ok(format!("{}/{}", api_url.trim_end_matches('/'), path));
        }

        // Use GitHub as marketplace source
        if let Some(repo) = &self.config.github_repo {
            let url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}",
                repo, self.config.github_branch, path
            );
            return Ok(url);
        }

        Err(MarketplaceError::InvalidConfig("No marketplace source configured".to_string()))
    }

    /// Get current platform identifier
    pub fn get_current_platform() -> String {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            return "windows-x64".to_string();
        }

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return "linux-x64".to_string();
        }

        #[cfg(all(target_os = "linux", target_arch = "arm"))]
        {
            return "linux-armv7".to_string();
        }

        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            return "linux-arm64".to_string();
        }

        #[cfg(not(any(
            all(target_os = "windows", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "arm"),
            all(target_os = "linux", target_arch = "aarch64")
        )))]
        {
            "unknown".to_string()
        }
    }
}
