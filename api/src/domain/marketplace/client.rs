use std::path::Path;
use std::time::Duration;
use reqwest::Client;
use sha2::{Sha256, Digest};

use crate::infrastructure::config::settings::MarketplaceConfig;
use super::error::{MarketplaceError, Result};
use super::metadata::{TemplateIndex, DriverIndex, TemplateMetadata, DriverMetadata};

pub struct MarketplaceClient {
    http_client: Client,
    config: MarketplaceConfig,
}

impl MarketplaceClient {
    pub fn new(config: MarketplaceConfig) -> Result<Self> {
        if !config.enabled {
            return Err(MarketplaceError::Disabled);
        }

        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.download_timeout_secs))
            .build()?;

        Ok(Self {
            http_client,
            config,
        })
    }

    /// 获取模板列表
    pub async fn fetch_templates(&self) -> Result<Vec<TemplateMetadata>> {
        let url = self.build_url("templates/index.json")?;
        tracing::info!("Fetching templates from: {}", url);

        // 检查是否是本地文件
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

    /// 获取驱动列表
    pub async fn fetch_drivers(&self) -> Result<Vec<DriverMetadata>> {
        let url = self.build_url("drivers/index.json")?;
        tracing::info!("Fetching drivers from: {}", url);

        // 检查是否是本地文件
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

    /// 下载资源文件
    pub async fn download_resource(&self, url: &str, dest: &Path) -> Result<()> {
        tracing::info!("Downloading resource from {} to {:?}", url, dest);

        let response = self.http_client.get(url).send().await?;
        let bytes = response.bytes().await?;

        // 确保目标目录存在
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(dest, &bytes).await?;

        tracing::info!("Downloaded {} bytes to {:?}", bytes.len(), dest);
        Ok(())
    }

    /// 验证文件校验和
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

    /// 计算文件校验和
    fn calculate_checksum(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        format!("sha256:{}", hex::encode(result))
    }

    /// 构建资源URL
    fn build_url(&self, path: &str) -> Result<String> {
        // 优先使用 API URL
        if let Some(api_url) = &self.config.api_url {
            return Ok(format!("{}/{}", api_url.trim_end_matches('/'), path));
        }

        // 使用 GitHub 作为市场源
        if let Some(repo) = &self.config.github_repo {
            let url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}",
                repo, self.config.github_branch, path
            );
            return Ok(url);
        }

        Err(MarketplaceError::InvalidConfig(
            "No marketplace source configured".to_string(),
        ))
    }

    /// 获取当前平台标识
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
