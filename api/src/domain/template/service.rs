use crate::domain::template::repository::TemplateRepository;
use crate::dto::entity::template_error::TemplateError;
use std::sync::Arc;
use tracing::info;

/// 模板服务 - 提供高级模板操作功能
#[derive(Debug)]
pub struct TemplateService {
    repository: Arc<TemplateRepository>,
}

impl TemplateService {
    /// 创建新的模板服务实例
    pub fn new(repository: Arc<TemplateRepository>) -> Self {
        Self { repository }
    }

    /// 初始化模板系统
    pub async fn initialize(&self) -> Result<(), TemplateError> {
        info!("初始化模板系统");

        // 确保目录结构存在
        self.repository
            .get_file_manager()
            .ensure_directory_structure()?;

        // 加载内置模板
        let _templates = self.repository.load_builtin_templates().await?;

        info!("模板系统初始化完成");
        Ok(())
    }

    /// 获取仓库引用
    pub fn get_repository(&self) -> &TemplateRepository {
        &self.repository
    }
}
