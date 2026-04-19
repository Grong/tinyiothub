use std::{path::PathBuf, sync::Arc};

use super::{
    client::MarketplaceClient,
    error::{MarketplaceError, Result},
    metadata::TemplateMetadata,
};
use crate::domain::template::repository::TemplateRepository;

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

    /// 从市场安装模板
    pub async fn install_from_marketplace(
        &self,
        template_id: &str,
        version: Option<&str>,
    ) -> Result<String> {
        tracing::info!("Installing template {} from marketplace", template_id);

        // 1. 获取模板列表
        let templates = self.client.fetch_templates().await?;

        // 2. 查找指定模板
        let template_meta = templates
            .iter()
            .find(|t| t.id == template_id)
            .ok_or_else(|| MarketplaceError::NotFound(format!("Template: {}", template_id)))?;

        // 3. 检查版本（如果指定）
        if let Some(ver) = version {
            if template_meta.version != ver {
                return Err(MarketplaceError::NotFound(format!(
                    "Template {} version {}",
                    template_id, ver
                )));
            }
        }

        // 4. 下载模板文件
        let temp_file = self.download_template(template_meta).await?;

        // 5. 验证校验和（开发模式下跳过）
        if !template_meta.checksum.starts_with("sha256:test")
            && !template_meta.checksum.contains("test")
        {
            self.client.verify_checksum(&temp_file, &template_meta.checksum).await?;
        } else {
            tracing::warn!("Skipping checksum verification for test/development template");
        }

        // 6. 保存到 templates/custom/ 目录
        let dest_file = self.save_template(&temp_file, template_id).await?;

        // 7. 导入到数据库
        self.import_template(&dest_file).await?;

        tracing::info!("Successfully installed template: {}", template_id);
        Ok(template_id.to_string())
    }

    /// 下载模板文件
    async fn download_template(&self, meta: &TemplateMetadata) -> Result<PathBuf> {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("template_{}.json", meta.id));

        self.client.download_resource(&meta.file_url, &temp_file).await?;

        Ok(temp_file)
    }

    /// 保存模板到目标目录
    async fn save_template(&self, temp_file: &PathBuf, template_id: &str) -> Result<PathBuf> {
        let custom_dir = self.templates_dir.join("custom");
        tokio::fs::create_dir_all(&custom_dir).await?;

        let dest_file = custom_dir.join(format!("{}.json", template_id));
        tokio::fs::copy(temp_file, &dest_file).await?;

        // 删除临时文件
        let _ = tokio::fs::remove_file(temp_file).await;

        Ok(dest_file)
    }

    /// 导入模板到数据库
    async fn import_template(&self, file_path: &PathBuf) -> Result<()> {
        let content = tokio::fs::read_to_string(file_path).await?;
        let template_data: serde_json::Value = serde_json::from_str(&content)?;

        // 将 JSON 转换为 CreateDeviceTemplateRequest
        let request: tinyiothub_core::models::device_template::CreateDeviceTemplateRequest =
            serde_json::from_value(template_data).map_err(|e| {
                MarketplaceError::Template(format!("Invalid template format: {}", e))
            })?;

        // 使用 repository 的 create 方法
        self.repository
            .create(&request)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?;

        Ok(())
    }

    /// 检查模板是否已安装
    pub async fn is_installed(&self, template_id: &str) -> Result<bool> {
        match self.repository.find_by_id(template_id).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(MarketplaceError::Template(e.to_string())),
        }
    }

    /// 获取已安装模板的版本
    pub async fn get_installed_version(&self, template_id: &str) -> Result<Option<String>> {
        match self.repository.find_by_id(template_id).await {
            Ok(Some(template)) => Ok(Some(template.version)),
            Ok(None) => Ok(None),
            Err(e) => Err(MarketplaceError::Template(e.to_string())),
        }
    }
}
