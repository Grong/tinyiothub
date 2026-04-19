use std::{path::PathBuf, sync::Arc};

use sqlx;
use tracing::{info, warn};

use crate::{
    domain::template::{
        file_manager::TemplateFileManager,
        search_service::{TemplateFilters, TemplateSearchService},
    },
    dto::entity::{
        device_template::{
            CreateDeviceTemplateRequest, DeviceTemplate, TemplateCategory, TemplateQueryParams,
            UpdateDeviceTemplateRequest,
        },
        template_error::TemplateError,
    },
    infrastructure::persistence::database::Database,
};

/// 模板仓库 - 负责设备模板的存储和检索
#[derive(Debug)]
pub struct TemplateRepository {
    database: Arc<Database>,
    file_manager: TemplateFileManager,
    search_service: TemplateSearchService,
}

impl TemplateRepository {
    /// 创建新的模板仓库实例
    pub fn new(database: Arc<Database>, file_system_path: PathBuf) -> Self {
        let file_manager = TemplateFileManager::new(file_system_path);
        let search_service = TemplateSearchService::new(database.clone());
        Self { database, file_manager, search_service }
    }

    /// 查找所有模板（支持分页和筛选）
    pub async fn find_all(
        &self,
        params: &TemplateQueryParams,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        info!("查询设备模板列表，参数: {:?}", params);

        let templates = DeviceTemplate::find_all(&self.database, params).await?;

        info!("找到 {} 个设备模板", templates.len());
        Ok(templates)
    }

    /// 根据ID查找模板
    pub async fn find_by_id(&self, id: &str) -> Result<Option<DeviceTemplate>, TemplateError> {
        info!("根据ID查找设备模板: {}", id);

        let template = DeviceTemplate::find_by_id(&self.database, id).await?;

        if template.is_some() {
            info!("找到设备模板: {}", id);
        } else {
            warn!("设备模板不存在: {}", id);
        }

        Ok(template)
    }

    /// 根据分类查找模板
    pub async fn find_by_category(
        &self,
        category: &str,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        info!("根据分类查找设备模板: {}", category);

        let templates = DeviceTemplate::find_by_category(&self.database, category).await?;

        info!("在分类 {} 中找到 {} 个设备模板", category, templates.len());
        Ok(templates)
    }
    /// 搜索模板
    pub async fn search(&self, keyword: &str) -> Result<Vec<DeviceTemplate>, TemplateError> {
        info!("搜索设备模板，关键词: {}", keyword);

        let templates = DeviceTemplate::search(&self.database, keyword, None).await?;

        info!("搜索到 {} 个匹配的设备模板", templates.len());
        Ok(templates)
    }

    /// 高级搜索模板
    pub async fn advanced_search(
        &self,
        params: &TemplateQueryParams,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        self.search_service.advanced_search(params).await
    }

    /// 按分类搜索模板
    pub async fn search_by_category(
        &self,
        category: &str,
        limit: Option<u32>,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        self.search_service.search_by_category(category, limit).await
    }

    /// 按厂商搜索模板
    pub async fn search_by_manufacturer(
        &self,
        manufacturer: &str,
        limit: Option<u32>,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        self.search_service.search_by_manufacturer(manufacturer, limit).await
    }

    /// 按协议类型搜索模板
    pub async fn search_by_protocol(
        &self,
        protocol_type: &str,
        limit: Option<u32>,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        self.search_service.search_by_protocol(protocol_type, limit).await
    }

    /// 多条件组合筛选
    pub async fn filter_templates(
        &self,
        filters: &TemplateFilters,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        self.search_service.filter_templates(filters).await
    }

    /// 获取搜索建议
    pub async fn get_search_suggestions(
        &self,
        keyword: &str,
        limit: u32,
    ) -> Result<Vec<String>, TemplateError> {
        self.search_service.get_search_suggestions(keyword, limit).await
    }

    /// 获取热门搜索关键词
    pub async fn get_popular_keywords(&self, limit: u32) -> Result<Vec<String>, TemplateError> {
        self.search_service.get_popular_keywords(limit).await
    }

    /// 统计搜索结果数量
    pub async fn count_search_results(
        &self,
        params: &TemplateQueryParams,
    ) -> Result<i64, TemplateError> {
        self.search_service.count_search_results(params).await
    }

    /// 创建新模板
    pub async fn create(
        &self,
        request: &CreateDeviceTemplateRequest,
    ) -> Result<DeviceTemplate, TemplateError> {
        info!("创建新设备模板: {}", request.name);

        // 检查模板名称是否已存在
        if DeviceTemplate::exists_by_name(&self.database, &request.name).await? {
            return Err(TemplateError::TemplateNameExists { name: request.name.clone() });
        }

        // 验证分类是否存在
        let categories = TemplateCategory::get_categories(&self.database).await?;
        if !categories.iter().any(|c| c.name == request.category) {
            return Err(TemplateError::CategoryNotFound { category: request.category.clone() });
        }

        let template = DeviceTemplate::create(&self.database, request).await?;

        info!("成功创建设备模板: {} (ID: {})", template.name, template.id);
        Ok(template)
    }

    /// 更新模板
    pub async fn update(
        &self,
        id: &str,
        request: &UpdateDeviceTemplateRequest,
    ) -> Result<DeviceTemplate, TemplateError> {
        info!("更新设备模板: {}", id);

        // 检查模板是否存在
        if self.find_by_id(id).await?.is_none() {
            return Err(TemplateError::TemplateNotFound { id: id.to_string() });
        }

        // 如果更新名称，检查新名称是否已存在
        if let Some(new_name) = &request.name {
            let existing = DeviceTemplate::find_by_name(&self.database, new_name).await?;
            if let Some(existing_template) = existing {
                if existing_template.id != id {
                    return Err(TemplateError::TemplateNameExists { name: new_name.clone() });
                }
            }
        }

        // 如果更新分类，验证分类是否存在
        if let Some(new_category) = &request.category {
            let categories = TemplateCategory::get_categories(&self.database).await?;
            if !categories.iter().any(|c| c.name == *new_category) {
                return Err(TemplateError::CategoryNotFound { category: new_category.clone() });
            }
        }

        let template = DeviceTemplate::update(&self.database, id, request).await?;

        info!("成功更新设备模板: {} (ID: {})", template.name, template.id);
        Ok(template)
    }
    /// 删除模板
    pub async fn delete(&self, id: &str) -> Result<bool, TemplateError> {
        info!("删除设备模板: {}", id);

        // 检查模板是否存在
        let template = self.find_by_id(id).await?;
        if template.is_none() {
            return Err(TemplateError::TemplateNotFound { id: id.to_string() });
        }

        // 检查是否有设备正在使用该模板
        // TODO: 实现设备依赖检查
        // if self.check_template_dependencies(id).await? {
        //     return Err(TemplateError::TemplateInUse {
        //         template_id: id.to_string()
        //     });
        // }

        let rows_affected = DeviceTemplate::delete(&self.database, id).await?;
        let success = rows_affected > 0;

        if success {
            info!("成功删除设备模板: {}", id);
        } else {
            warn!("删除设备模板失败，可能已被删除: {}", id);
        }

        Ok(success)
    }

    /// 加载内置模板
    pub async fn load_builtin_templates(&self) -> Result<Vec<DeviceTemplate>, TemplateError> {
        info!("加载内置设备模板");

        // 确保目录结构存在
        self.file_manager.ensure_directory_structure()?;

        // 从文件系统加载模板
        let template_requests = self.file_manager.load_builtin_templates()?;

        // 将文件模板同步到数据库
        let mut templates = Vec::new();
        for request in template_requests {
            match self.sync_template_to_database(&request).await {
                Ok(template) => templates.push(template),
                Err(e) => {
                    warn!("同步模板到数据库失败: {}, 错误: {}", request.name, e);
                    // 继续处理其他模板
                }
            }
        }

        // 同时从数据库加载已存在的内置模板
        let db_templates = DeviceTemplate::load_builtin_templates(&self.database).await?;

        // 合并结果，去重
        for db_template in db_templates {
            if !templates.iter().any(|t| t.id == db_template.id) {
                templates.push(db_template);
            }
        }

        info!("加载了 {} 个内置设备模板", templates.len());
        Ok(templates)
    }

    /// 同步模板到数据库
    async fn sync_template_to_database(
        &self,
        request: &CreateDeviceTemplateRequest,
    ) -> Result<DeviceTemplate, TemplateError> {
        // 检查模板是否已存在
        if let Some(existing) = DeviceTemplate::find_by_name(&self.database, &request.name).await? {
            // 如果是内置模板且版本相同，跳过
            if existing.is_builtin() && existing.version == request.version {
                return Ok(existing);
            }
        }

        // 创建或更新模板
        let template = DeviceTemplate::create(&self.database, request).await?;

        // 标记为内置模板
        let _update_request = UpdateDeviceTemplateRequest {
            name: None,
            display_name: None,
            description: None,
            version: None,
            author: None,
            category: None,
            manufacturer: None,
            device_type: None,
            protocol_type: None,
            driver_name: None,
            tags: None,
            device_info: None,
            properties: None,
            commands: None,
        };

        // 直接更新数据库标记为内置模板
        sqlx::query("UPDATE device_templates SET is_builtin = 1 WHERE id = ?")
            .bind(&template.id)
            .execute(self.database.pool())
            .await?;

        // 重新获取更新后的模板
        DeviceTemplate::find_by_id(&self.database, &template.id)
            .await?
            .ok_or(TemplateError::TemplateNotFound { id: template.id })
    }

    /// 获取模板分类
    pub async fn get_categories(&self) -> Result<Vec<TemplateCategory>, TemplateError> {
        info!("获取模板分类列表");

        let categories = TemplateCategory::get_categories(&self.database).await?;

        info!("找到 {} 个模板分类", categories.len());
        Ok(categories)
    }

    /// 统计模板数量
    pub async fn count(&self, params: &TemplateQueryParams) -> Result<i64, TemplateError> {
        let count = DeviceTemplate::count(&self.database, params).await?;
        Ok(count)
    }

    /// 检查模板名称是否存在
    pub async fn exists_by_name(&self, name: &str) -> Result<bool, TemplateError> {
        let exists = DeviceTemplate::exists_by_name(&self.database, name).await?;
        Ok(exists)
    }

    /// 获取文件系统路径
    pub fn get_file_system_path(&self) -> &PathBuf {
        self.file_manager.get_templates_root()
    }

    /// 获取文件管理器
    pub fn get_file_manager(&self) -> &TemplateFileManager {
        &self.file_manager
    }

    /// 获取搜索服务
    pub fn get_search_service(&self) -> &TemplateSearchService {
        &self.search_service
    }
}
