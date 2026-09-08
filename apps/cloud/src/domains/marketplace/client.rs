use std::{path::Path, time::Duration};

use reqwest::Client;
use sha2::{Digest, Sha256};
use tinyiothub_core::config::MarketplaceConfig;

use super::{
    dto::{Driver as MDriver, PaginatedList, Template as MTemplate},
    error::{MarketplaceError, Result},
    metadata::{AuthorInfo, DriverMetadata, TemplateMetadata},
};

/// Marketplace API response wrapper.
#[derive(Debug, serde::Deserialize)]
struct ApiResponse<T> {
    #[allow(dead_code)]
    code: i32,
    #[allow(dead_code)]
    msg: String,
    result: T,
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

        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.download_timeout_secs))
            .build()?;

        Ok(Self { http_client, config })
    }

    /// Get API base URL (includes /api/v1).
    fn api_base(&self) -> Result<&str> {
        self.config
            .api_url
            .as_deref()
            .ok_or_else(|| MarketplaceError::InvalidConfig("No marketplace API URL configured".to_string()))
    }

    /// 翻页拉取全量列表（缺省 20 条/页，不带参数会静默截断在第 1 页）。
    /// MAX_PAGES 是防御性上限：上游异常返回满页时避免无界请求。
    async fn fetch_all_pages<M, T, F>(&self, path: &str, map: F) -> Result<Vec<T>>
    where
        M: serde::de::DeserializeOwned,
        F: Fn(&str, M) -> T,
    {
        const PER_PAGE: usize = 100;
        const MAX_PAGES: usize = 100;

        let base = self.api_base()?;
        let mut out = Vec::new();
        let mut page = 1usize;
        loop {
            let url = format!("{}{}?page={}&per_page={}", base, path, page, PER_PAGE);
            tracing::info!("Fetching marketplace {} page {}: {}", path, page, url);

            let resp = self.http_client.get(&url).send().await?;
            // 冷缓存的空列表（200 + total=0 + X-Cache-Stale）不是真实的空目录——
            // 同步管线若在降级窗口运行会把空目录当好数据落库。显式拒绝。
            if resp.headers().contains_key("x-cache-stale") {
                return Err(MarketplaceError::Driver(format!(
                    "marketplace {} cache not loaded (X-Cache-Stale), retry later",
                    path
                )));
            }
            if !resp.status().is_success() {
                return Err(MarketplaceError::Driver(format!(
                    "marketplace {} returned HTTP {}",
                    path,
                    resp.status()
                )));
            }

            let response: ApiResponse<PaginatedList<M>> = resp.json().await?;

            let total = response.result.total;
            let items = response.result.items;
            // 末页判断：短页或空页（上游 total 撒谎时兜底）
            let is_last_page = items.len() < PER_PAGE;
            out.extend(items.into_iter().map(|m| map(base, m)));

            if is_last_page || out.len() >= total {
                break;
            }
            page += 1;
            if page > MAX_PAGES {
                tracing::warn!(
                    "marketplace {} exceeded {} pages, returning partial catalog",
                    path,
                    MAX_PAGES
                );
                break;
            }
        }

        Ok(out)
    }

    /// Fetch template list from marketplace API.
    pub async fn fetch_templates(&self) -> Result<Vec<TemplateMetadata>> {
        self.fetch_all_pages("/templates", |base, t: MTemplate| {
            let name = t.name;
            TemplateMetadata {
                id: name.clone(),
                file_url: format!("{}/templates/{}", base, urlencoding::encode(&name)),
                name,
                version: t.version,
                category: t.category,
                protocol: t.protocol_type,
                manufacturer: t.manufacturer.unwrap_or_default(),
                description: t.description.zh.or(t.description.en).unwrap_or_default(),
                tags: t.tags,
                author: AuthorInfo {
                    name: t.author,
                    email: String::new(),
                },
                icon: t.icon,
                downloads: t.downloads as u64,
                rating: t.rating.unwrap_or(0.0) as f32,
                reviews: t.reviews.unwrap_or(0) as u32,
                license: if t.license.is_empty() {
                    "MIT".to_string()
                } else {
                    t.license
                },
                checksum: String::new(),
                size: 0,
                created_at: t.created_at,
                updated_at: t.updated_at,
            }
        })
        .await
    }

    /// Fetch a single template definition from marketplace API.
    /// Returns the raw template JSON value (the `result` field).
    pub async fn fetch_template(&self, name: &str) -> Result<serde_json::Value> {
        let base = self.api_base()?;
        let url = format!("{}/templates/{}", base, urlencoding::encode(name));
        tracing::info!("Fetching template from: {}", url);

        let response: ApiResponse<serde_json::Value> = self.http_client.get(&url).send().await?.json().await?;

        Ok(response.result)
    }

    /// Fetch driver list from marketplace API.
    pub async fn fetch_drivers(&self) -> Result<Vec<DriverMetadata>> {
        self.fetch_all_pages("/drivers", |_base, d: MDriver| DriverMetadata {
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
            requirements: d.requirements.and_then(|v| serde_json::from_value(v).ok()).unwrap_or(
                super::metadata::DriverRequirements {
                    min_version: "0.1.0".to_string(),
                },
            ),
            created_at: d.created_at,
            updated_at: d.updated_at,
        })
        .await
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
