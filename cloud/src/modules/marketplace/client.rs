use std::{path::Path, time::Duration};

use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tinyiothub_config::MarketplaceConfig;

use super::{
    error::{MarketplaceError, Result},
    metadata::{AuthorInfo, DriverMetadata, TemplateMetadata},
};

/// Marketplace API response wrapper.
#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    #[allow(dead_code)]
    code: i32,
    #[allow(dead_code)]
    msg: String,
    result: T,
}

/// Marketplace paginated list.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PaginatedList<T> {
    items: Vec<T>,
    total: usize,
    page: usize,
    per_page: usize,
}

/// Template as returned by marketplace API.
#[derive(Debug, Deserialize)]
struct MarketplaceTemplate {
    name: String,
    version: String,
    category: String,
    protocol_type: String,
    #[serde(default)]
    manufacturer: Option<String>,
    #[serde(default)]
    description: serde_json::Value,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    author: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    downloads: i64,
    #[serde(default)]
    rating: Option<f64>,
    #[serde(default)]
    reviews: Option<i32>,
    #[serde(default)]
    license: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
}

/// Driver as returned by marketplace API.
#[derive(Debug, Deserialize)]
struct MarketplaceDriver {
    id: String,
    name: String,
    version: String,
    protocol: String,
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    author_name: String,
    #[serde(default)]
    author_email: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    downloads: i64,
    #[serde(default)]
    rating: Option<f64>,
    #[serde(default)]
    reviews: Option<i32>,
    license: String,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    documentation: Option<String>,
    #[serde(default)]
    platforms: Option<serde_json::Value>,
    #[serde(default)]
    requirements: Option<serde_json::Value>,
    updated_at: String,
}

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

    /// Get API base URL (includes /api/v1).
    fn api_base(&self) -> Result<&str> {
        self.config
            .api_url
            .as_deref()
            .ok_or_else(|| MarketplaceError::InvalidConfig("No marketplace API URL configured".to_string()))
    }

    /// Fetch template list from marketplace API.
    pub async fn fetch_templates(&self) -> Result<Vec<TemplateMetadata>> {
        let base = self.api_base()?;
        let url = format!("{}/templates", base);
        tracing::info!("Fetching templates from: {}", url);

        let response: ApiResponse<PaginatedList<MarketplaceTemplate>> =
            self.http_client.get(&url).send().await?.json().await?;

        let templates = response
            .result
            .items
            .into_iter()
            .map(|t| {
                let name = t.name;
                TemplateMetadata {
                    id: name.clone(),
                    file_url: format!("{}/templates/{}", base, urlencoding::encode(&name)),
                    name,
                    version: t.version,
                    category: t.category,
                    protocol: t.protocol_type,
                    manufacturer: t.manufacturer.unwrap_or_default(),
                    description: t
                        .description
                        .get("zh")
                        .or_else(|| t.description.get("en"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    tags: t.tags,
                    author: AuthorInfo {
                        name: t.author,
                        email: String::new(),
                    },
                    icon: t.icon,
                    downloads: t.downloads as u64,
                    rating: t.rating.unwrap_or(0.0) as f32,
                    reviews: t.reviews.unwrap_or(0) as u32,
                    license: if t.license.is_empty() { "MIT".to_string() } else { t.license },
                    checksum: String::new(),
                    size: 0,
                    created_at: t.created_at,
                    updated_at: t.updated_at,
                }
            })
            .collect();

        Ok(templates)
    }

    /// Fetch a single template definition from marketplace API.
    /// Returns the raw template JSON value (the `result` field).
    pub async fn fetch_template(&self, name: &str) -> Result<serde_json::Value> {
        let base = self.api_base()?;
        let url = format!("{}/templates/{}", base, urlencoding::encode(name));
        tracing::info!("Fetching template from: {}", url);

        let response: ApiResponse<serde_json::Value> =
            self.http_client.get(&url).send().await?.json().await?;

        Ok(response.result)
    }

    /// Fetch driver list from marketplace API.
    pub async fn fetch_drivers(&self) -> Result<Vec<DriverMetadata>> {
        let base = self.api_base()?;
        let url = format!("{}/drivers", base);
        tracing::info!("Fetching drivers from: {}", url);

        let response: ApiResponse<PaginatedList<MarketplaceDriver>> =
            self.http_client.get(&url).send().await?.json().await?;

        let drivers = response
            .result
            .items
            .into_iter()
            .map(|d| DriverMetadata {
                id: d.id,
                name: d.name,
                version: d.version,
                protocol: d.protocol,
                description: d.description,
                tags: d.tags,
                author: AuthorInfo {
                    name: d.author_name,
                    email: d.author_email.unwrap_or_default(),
                },
                icon: d.icon,
                downloads: d.downloads as u64,
                rating: d.rating.unwrap_or(0.0) as f32,
                reviews: d.reviews.unwrap_or(0) as u32,
                license: d.license,
                homepage: d.homepage,
                documentation: d.documentation,
                platforms: d
                    .platforms
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default(),
                requirements: d
                    .requirements
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or(super::metadata::DriverRequirements {
                        min_version: "0.1.0".to_string(),
                    }),
                created_at: String::new(),
                updated_at: d.updated_at,
            })
            .collect();

        Ok(drivers)
    }

    /// Download resource file from a URL.
    pub async fn download_resource(&self, url: &str, dest: &Path) -> Result<()> {
        tracing::info!("Downloading resource from {} to {:?}", url, dest);

        let response = self.http_client.get(url).send().await?;
        let bytes = response.bytes().await?;

        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(dest, &bytes).await?;

        tracing::info!("Downloaded {} bytes to {:?}", bytes.len(), dest);
        Ok(())
    }

    /// Verify file checksum.
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

    /// Calculate file checksum.
    fn calculate_checksum(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        format!("sha256:{}", hex::encode(result))
    }

    /// Get current platform identifier.
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
