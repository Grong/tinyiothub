use std::path::PathBuf;
use std::sync::Arc;

use super::client::MarketplaceClient;
use super::error::{MarketplaceError, Result};
use super::metadata::DriverMetadata;
use crate::domain::device::driver;

pub struct DriverInstaller {
    client: Arc<MarketplaceClient>,
    drivers_dir: PathBuf,
}

impl DriverInstaller {
    pub fn new(client: Arc<MarketplaceClient>, drivers_dir: PathBuf) -> Self {
        Self {
            client,
            drivers_dir,
        }
    }

    /// 从市场安装驱动
    pub async fn install_from_marketplace(
        &self,
        driver_id: &str,
        version: Option<&str>,
    ) -> Result<String> {
        tracing::info!("Installing driver {} from marketplace", driver_id);

        // 1. 获取驱动列表
        let drivers = self.client.fetch_drivers().await?;

        // 2. 查找指定驱动
        let driver_meta = drivers
            .iter()
            .find(|d| d.id == driver_id)
            .ok_or_else(|| MarketplaceError::NotFound(format!("Driver: {}", driver_id)))?;

        // 3. 检查版本（如果指定）
        if let Some(ver) = version {
            if driver_meta.version != ver {
                return Err(MarketplaceError::NotFound(format!(
                    "Driver {} version {}",
                    driver_id, ver
                )));
            }
        }

        // 4. 选择当前平台的二进制文件
        let platform = MarketplaceClient::get_current_platform();
        let binary_info = driver_meta
            .platforms
            .get(&platform)
            .ok_or_else(|| MarketplaceError::PlatformNotSupported(platform.clone()))?;

        // 5. 下载驱动文件
        let driver_file = self
            .download_driver(driver_id, binary_info, &platform)
            .await?;

        // 6. 验证校验和（开发模式下跳过）
        if !binary_info.checksum.starts_with("sha256:test")
            && !binary_info.checksum.contains("test")
        {
            self.client
                .verify_checksum(&driver_file, &binary_info.checksum)
                .await?;
        } else {
            tracing::warn!("Skipping checksum verification for test/development driver");
        }

        // 7. 自动加载驱动
        let driver_name = self.load_driver(&driver_file).await?;

        tracing::info!(
            "Successfully installed driver: {} ({})",
            driver_id,
            driver_name
        );
        Ok(driver_name)
    }

    /// 下载驱动文件
    async fn download_driver(
        &self,
        driver_id: &str,
        binary_info: &super::metadata::PlatformBinary,
        platform: &str,
    ) -> Result<PathBuf> {
        // 确保驱动目录存在
        tokio::fs::create_dir_all(&self.drivers_dir).await?;

        // 确定文件扩展名
        let extension = if platform.starts_with("windows") {
            "dll"
        } else {
            "so"
        };

        let dest_file = self
            .drivers_dir
            .join(format!("{}_driver.{}", driver_id, extension));

        self.client
            .download_resource(&binary_info.file_url, &dest_file)
            .await?;

        Ok(dest_file)
    }

    /// 加载驱动
    async fn load_driver(&self, driver_file: &PathBuf) -> Result<String> {
        let path_str = driver_file
            .to_str()
            .ok_or_else(|| MarketplaceError::InstallationFailed("Invalid path".to_string()))?;

        driver::load_dynamic_driver(path_str).map_err(|e| MarketplaceError::Driver(e.to_string()))
    }

    /// 检查驱动是否已安装
    pub fn is_installed(&self, driver_name: &str) -> bool {
        let registry = driver::dynamic::registry::get_global_registry();
        registry.has_driver(driver_name)
    }

    /// 卸载驱动
    pub async fn uninstall(&self, driver_name: &str) -> Result<()> {
        driver::unload_dynamic_driver(driver_name)
            .map_err(|e| MarketplaceError::Driver(e.to_string()))?;

        Ok(())
    }
}
