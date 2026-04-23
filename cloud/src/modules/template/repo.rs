use std::path::PathBuf;
use std::sync::Arc;

use sqlx::{QueryBuilder, Row};
use tracing::{debug, error, info, warn};

use tinyiothub_core::models::template_error::{TemplateError, ValidationError};

use crate::shared::persistence::Database;

use super::types::{
    CreateDeviceTemplateRequest, DeviceTemplate, TemplateCategory, TemplateQueryParams,
    UpdateDeviceTemplateRequest,
};

// ─── TemplateRepository ───────────────────────────────────────

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
            if let Some(existing_template) = existing
                && existing_template.id != id {
                    return Err(TemplateError::TemplateNameExists { name: new_name.clone() });
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
        DeviceTemplate::set_builtin(&self.database, &template.id).await?;

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

// ─── TemplateSearchService ────────────────────────────────────

/// 模板搜索服务 - 负责高级搜索和筛选功能
#[derive(Debug)]
pub struct TemplateSearchService {
    database: Arc<Database>,
}

impl TemplateSearchService {
    /// 创建新的搜索服务实例
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    /// 高级搜索模板
    pub async fn advanced_search(
        &self,
        params: &TemplateQueryParams,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        info!("执行高级模板搜索，参数: {:?}", params);

        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, device_type, protocol_type, driver_name, tags,
                   device_info, properties, commands, is_builtin, is_active,
                   created_at, updated_at
            FROM device_templates WHERE is_active = 1
            "#,
        );

        // 构建搜索条件
        self.build_search_conditions(&mut query, params);

        // 添加排序
        query.push(" ORDER BY ");
        query.push("is_builtin DESC, "); // 内置模板优先
        query.push("category, name");

        // 添加分页
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let templates =
            query.build_query_as::<DeviceTemplate>().fetch_all(self.database.pool()).await?;

        info!("高级搜索找到 {} 个模板", templates.len());
        Ok(templates)
    }

    /// 构建搜索条件
    fn build_search_conditions(
        &self,
        query: &mut QueryBuilder<sqlx::Sqlite>,
        params: &TemplateQueryParams,
    ) {
        // 分类筛选
        if let Some(category) = &params.category {
            query.push(" AND category = ").push_bind(category);
        }

        // 厂商筛选
        if let Some(manufacturer) = &params.manufacturer {
            query.push(" AND manufacturer = ").push_bind(manufacturer);
        }

        // 设备类型筛选
        if let Some(device_type) = &params.device_type {
            query.push(" AND device_type = ").push_bind(device_type);
        }

        // 协议类型筛选
        if let Some(protocol_type) = &params.protocol_type {
            query.push(" AND protocol_type = ").push_bind(protocol_type);
        }

        // 关键词搜索
        if let Some(keyword) = &params.keyword {
            let search_pattern = format!("%{}%", keyword);
            query
                .push(" AND (name LIKE ")
                .push_bind(search_pattern.clone())
                .push(" OR display_name LIKE ")
                .push_bind(search_pattern.clone())
                .push(" OR tags LIKE ")
                .push_bind(search_pattern.clone())
                .push(" OR description LIKE ")
                .push_bind(search_pattern)
                .push(")");
        }
    }

    /// 按分类搜索模板
    pub async fn search_by_category(
        &self,
        category: &str,
        limit: Option<u32>,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        info!("按分类搜索模板: {}", category);

        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, device_type, protocol_type, driver_name, tags,
                   device_info, properties, commands, is_builtin, is_active,
                   created_at, updated_at
            FROM device_templates
            WHERE is_active = 1 AND category =
            "#,
        );
        query.push_bind(category);
        query.push(" ORDER BY is_builtin DESC, name");

        if let Some(limit) = limit {
            query.push(" LIMIT ").push_bind(limit as i64);
        }

        let templates = query.build_query_as::<DeviceTemplate>().fetch_all(self.database.pool()).await?;

        info!("在分类 {} 中找到 {} 个模板", category, templates.len());
        Ok(templates)
    }

    /// 按厂商搜索模板
    pub async fn search_by_manufacturer(
        &self,
        manufacturer: &str,
        limit: Option<u32>,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        info!("按厂商搜索模板: {}", manufacturer);

        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, device_type, protocol_type, driver_name, tags,
                   device_info, properties, commands, is_builtin, is_active,
                   created_at, updated_at
            FROM device_templates
            WHERE is_active = 1 AND manufacturer =
            "#,
        );
        query.push_bind(manufacturer);
        query.push(" ORDER BY is_builtin DESC, category, name");

        if let Some(limit) = limit {
            query.push(" LIMIT ").push_bind(limit as i64);
        }

        let templates = query.build_query_as::<DeviceTemplate>().fetch_all(self.database.pool()).await?;

        info!("厂商 {} 的模板找到 {} 个", manufacturer, templates.len());
        Ok(templates)
    }

    /// 按协议类型搜索模板
    pub async fn search_by_protocol(
        &self,
        protocol_type: &str,
        limit: Option<u32>,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        info!("按协议类型搜索模板: {}", protocol_type);

        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, device_type, protocol_type, driver_name, tags,
                   device_info, properties, commands, is_builtin, is_active,
                   created_at, updated_at
            FROM device_templates
            WHERE is_active = 1 AND protocol_type =
            "#,
        );
        query.push_bind(protocol_type);
        query.push(" ORDER BY is_builtin DESC, category, name");

        if let Some(limit) = limit {
            query.push(" LIMIT ").push_bind(limit as i64);
        }

        let templates = query.build_query_as::<DeviceTemplate>().fetch_all(self.database.pool()).await?;

        info!("协议类型 {} 的模板找到 {} 个", protocol_type, templates.len());
        Ok(templates)
    }

    /// 多条件组合筛选
    pub async fn filter_templates(
        &self,
        filters: &TemplateFilters,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        info!("执行多条件组合筛选: {:?}", filters);

        let mut query = QueryBuilder::new(
            r#"
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, device_type, protocol_type, driver_name, tags,
                   device_info, properties, commands, is_builtin, is_active,
                   created_at, updated_at
            FROM device_templates WHERE is_active = 1
            "#,
        );

        // 分类筛选
        if !filters.categories.is_empty() {
            query.push(" AND category IN (");
            let mut separated = query.separated(", ");
            for category in &filters.categories {
                separated.push_bind(category);
            }
            separated.push_unseparated(")");
        }

        // 厂商筛选
        if !filters.manufacturers.is_empty() {
            query.push(" AND manufacturer IN (");
            let mut separated = query.separated(", ");
            for manufacturer in &filters.manufacturers {
                separated.push_bind(manufacturer);
            }
            separated.push_unseparated(")");
        }

        // 协议类型筛选
        if !filters.protocol_types.is_empty() {
            query.push(" AND protocol_type IN (");
            let mut separated = query.separated(", ");
            for protocol_type in &filters.protocol_types {
                separated.push_bind(protocol_type);
            }
            separated.push_unseparated(")");
        }

        // 设备类型筛选
        if !filters.device_types.is_empty() {
            query.push(" AND device_type IN (");
            let mut separated = query.separated(", ");
            for device_type in &filters.device_types {
                separated.push_bind(device_type);
            }
            separated.push_unseparated(")");
        }

        // 标签筛选
        if !filters.tags.is_empty() {
            for tag in &filters.tags {
                query.push(" AND tags LIKE ").push_bind(format!("%{}%", tag));
            }
        }

        // 内置模板筛选
        if let Some(is_builtin) = filters.is_builtin {
            let builtin_value = if is_builtin { 1 } else { 0 };
            query.push(" AND is_builtin = ").push_bind(builtin_value);
        }

        // 添加排序
        query.push(" ORDER BY is_builtin DESC, category, name");

        // 添加分页
        if let Some(limit) = filters.limit {
            query.push(" LIMIT ").push_bind(limit as i64);
            if let Some(offset) = filters.offset {
                query.push(" OFFSET ").push_bind(offset as i64);
            }
        }

        let templates =
            query.build_query_as::<DeviceTemplate>().fetch_all(self.database.pool()).await?;

        info!("组合筛选找到 {} 个模板", templates.len());
        Ok(templates)
    }

    /// 获取搜索建议
    pub async fn get_search_suggestions(
        &self,
        keyword: &str,
        limit: u32,
    ) -> Result<Vec<String>, TemplateError> {
        info!("获取搜索建议，关键词: {}", keyword);

        let search_pattern = format!("%{}%", keyword);

        let suggestions = sqlx::query(
            r#"
            SELECT DISTINCT name as suggestion FROM device_templates
            WHERE is_active = 1 AND name LIKE ?
            UNION
            SELECT DISTINCT category as suggestion FROM device_templates
            WHERE is_active = 1 AND category LIKE ?
            UNION
            SELECT DISTINCT manufacturer as suggestion FROM device_templates
            WHERE is_active = 1 AND manufacturer IS NOT NULL AND manufacturer LIKE ?
            UNION
            SELECT DISTINCT protocol_type as suggestion FROM device_templates
            WHERE is_active = 1 AND protocol_type IS NOT NULL AND protocol_type LIKE ?
            ORDER BY suggestion
            LIMIT ?
            "#,
        )
        .bind(&search_pattern)
        .bind(&search_pattern)
        .bind(&search_pattern)
        .bind(&search_pattern)
        .bind(limit as i64)
        .fetch_all(self.database.pool())
        .await?;

        let suggestions: Vec<String> =
            suggestions.into_iter().map(|row| row.get::<String, _>("suggestion")).collect();

        debug!("找到 {} 个搜索建议", suggestions.len());
        Ok(suggestions)
    }

    /// 获取热门搜索关键词
    pub async fn get_popular_keywords(&self, limit: u32) -> Result<Vec<String>, TemplateError> {
        info!("获取热门搜索关键词");

        let popular = sqlx::query(
            r#"
            SELECT category as keyword, COUNT(*) as count FROM device_templates
            WHERE is_active = 1
            GROUP BY category
            UNION
            SELECT manufacturer as keyword, COUNT(*) as count FROM device_templates
            WHERE is_active = 1 AND manufacturer IS NOT NULL
            GROUP BY manufacturer
            UNION
            SELECT protocol_type as keyword, COUNT(*) as count FROM device_templates
            WHERE is_active = 1 AND protocol_type IS NOT NULL
            GROUP BY protocol_type
            ORDER BY count DESC
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(self.database.pool())
        .await?;

        let keywords: Vec<String> =
            popular.into_iter().map(|row| row.get::<String, _>("keyword")).collect();

        debug!("找到 {} 个热门关键词", keywords.len());
        Ok(keywords)
    }

    /// 统计搜索结果数量
    pub async fn count_search_results(
        &self,
        params: &TemplateQueryParams,
    ) -> Result<i64, TemplateError> {
        let mut query =
            QueryBuilder::new("SELECT COUNT(*) as count FROM device_templates WHERE is_active = 1");

        self.build_search_conditions(&mut query, params);

        let row = query.build().fetch_one(self.database.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }
}

/// 模板筛选条件
#[derive(Debug, Clone, Default)]
pub struct TemplateFilters {
    pub categories: Vec<String>,
    pub manufacturers: Vec<String>,
    pub protocol_types: Vec<String>,
    pub device_types: Vec<String>,
    pub tags: Vec<String>,
    pub is_builtin: Option<bool>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

// ─── TemplateFileManager ──────────────────────────────────────

use std::{
    fs,
    path::Path,
};

/// 模板文件系统管理器 - 负责模板文件的加载和解析
#[derive(Debug)]
pub struct TemplateFileManager {
    templates_root: PathBuf,
}

impl TemplateFileManager {
    /// 创建新的模板文件管理器
    pub fn new<P: AsRef<Path>>(templates_root: P) -> Self {
        Self { templates_root: templates_root.as_ref().to_path_buf() }
    }

    /// 获取内置模板目录路径
    pub fn get_builtin_path(&self) -> PathBuf {
        self.templates_root.join("builtin")
    }

    /// 获取自定义模板目录路径
    pub fn get_custom_path(&self) -> PathBuf {
        self.templates_root.join("custom")
    }

    /// 获取模式定义目录路径
    pub fn get_schemas_path(&self) -> PathBuf {
        self.templates_root.join("schemas")
    }

    /// 确保模板目录结构存在
    pub fn ensure_directory_structure(&self) -> Result<(), TemplateError> {
        info!("确保模板目录结构存在");

        let directories = vec![
            self.get_builtin_path().join("sensors"),
            self.get_builtin_path().join("cameras"),
            self.get_builtin_path().join("controllers"),
            self.get_builtin_path().join("robots"),
            self.get_custom_path(),
            self.get_schemas_path(),
        ];

        for dir in directories {
            if !dir.exists() {
                fs::create_dir_all(&dir).map_err(|e| {
                    error!("创建目录失败: {:?}, 错误: {}", dir, e);
                    TemplateError::FileSystemError { source: e }
                })?;
                info!("创建目录: {:?}", dir);
            }
        }

        Ok(())
    }

    /// 加载内置模板文件
    pub fn load_builtin_templates(
        &self,
    ) -> Result<Vec<CreateDeviceTemplateRequest>, TemplateError> {
        info!("加载内置模板文件");

        let mut templates = Vec::new();
        let builtin_path = self.get_builtin_path();

        if !builtin_path.exists() {
            warn!("内置模板目录不存在: {:?}", builtin_path);
            return Ok(templates);
        }

        // 遍历所有子目录
        let categories = vec!["sensors", "cameras", "controllers", "robots"];

        for category in categories {
            let category_path = builtin_path.join(category);
            if !category_path.exists() {
                continue;
            }

            match self.load_templates_from_directory(&category_path) {
                Ok(mut category_templates) => {
                    info!("从分类 {} 加载了 {} 个模板", category, category_templates.len());
                    templates.append(&mut category_templates);
                }
                Err(e) => {
                    error!("加载分类 {} 的模板失败: {}", category, e);
                }
            }
        }

        info!("总共加载了 {} 个内置模板", templates.len());
        Ok(templates)
    }

    /// 从指定目录加载模板文件
    fn load_templates_from_directory(
        &self,
        dir_path: &Path,
    ) -> Result<Vec<CreateDeviceTemplateRequest>, TemplateError> {
        let mut templates = Vec::new();

        let entries = fs::read_dir(dir_path).map_err(|e| {
            error!("读取目录失败: {:?}, 错误: {}", dir_path, e);
            TemplateError::FileSystemError { source: e }
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| TemplateError::FileSystemError { source: e })?;
            let path = entry.path();

            // 只处理 .json 文件
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                match self.load_template_from_file(&path) {
                    Ok(template) => {
                        templates.push(template);
                    }
                    Err(e) => {
                        error!("加载模板文件失败: {:?}, 错误: {}", path, e);
                    }
                }
            }
        }

        Ok(templates)
    }

    /// 从文件加载单个模板
    fn load_template_from_file(
        &self,
        file_path: &Path,
    ) -> Result<CreateDeviceTemplateRequest, TemplateError> {
        let content = fs::read_to_string(file_path).map_err(|e| {
            error!("读取模板文件失败: {:?}, 错误: {}", file_path, e);
            TemplateError::FileSystemError { source: e }
        })?;

        let template: CreateDeviceTemplateRequest =
            serde_json::from_str(&content).map_err(|e| {
                error!("解析模板文件JSON失败: {:?}, 错误: {}", file_path, e);
                TemplateError::JsonFormatError {
                    message: format!("文件 {:?} JSON格式错误: {}", file_path, e),
                }
            })?;

        // 基本验证
        self.validate_template_basic(&template)?;

        Ok(template)
    }

    /// 基本模板验证
    fn validate_template_basic(
        &self,
        template: &CreateDeviceTemplateRequest,
    ) -> Result<(), TemplateError> {
        if template.name.is_empty() {
            return Err(TemplateError::ValidationFailed {
                errors: vec![ValidationError::required_field("name")],
            });
        }

        if template.category.is_empty() {
            return Err(TemplateError::ValidationFailed {
                errors: vec![ValidationError::required_field("category")],
            });
        }

        if template.device_type.is_empty() {
            return Err(TemplateError::ValidationFailed {
                errors: vec![ValidationError::required_field("device_type")],
            });
        }

        if template.display_name.is_empty() {
            return Err(TemplateError::ValidationFailed {
                errors: vec![ValidationError::required_field("display_name")],
            });
        }

        Ok(())
    }

    /// 保存模板到文件
    pub fn save_template_to_file(
        &self,
        template: &CreateDeviceTemplateRequest,
        file_path: &Path,
    ) -> Result<(), TemplateError> {
        info!("保存模板到文件: {:?}", file_path);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                error!("创建目录失败: {:?}, 错误: {}", parent, e);
                TemplateError::FileSystemError { source: e }
            })?;
        }

        let content = serde_json::to_string_pretty(template).map_err(|e| {
            error!("序列化模板失败: {}", e);
            TemplateError::SerializationError { source: e }
        })?;

        fs::write(file_path, content).map_err(|e| {
            error!("写入模板文件失败: {:?}, 错误: {}", file_path, e);
            TemplateError::FileSystemError { source: e }
        })?;

        info!("成功保存模板到文件: {:?}", file_path);
        Ok(())
    }

    /// 删除模板文件
    pub fn delete_template_file(&self, file_path: &Path) -> Result<(), TemplateError> {
        info!("删除模板文件: {:?}", file_path);

        if !file_path.exists() {
            warn!("模板文件不存在: {:?}", file_path);
            return Ok(());
        }

        fs::remove_file(file_path).map_err(|e| {
            error!("删除模板文件失败: {:?}, 错误: {}", file_path, e);
            TemplateError::FileSystemError { source: e }
        })?;

        info!("成功删除模板文件: {:?}", file_path);
        Ok(())
    }

    /// 获取模板文件路径
    pub fn get_template_file_path(&self, category: &str, template_name: &str) -> PathBuf {
        self.get_builtin_path().join(category).join(format!("{}.json", template_name))
    }

    /// 获取自定义模板文件路径
    pub fn get_custom_template_file_path(&self, template_name: &str) -> PathBuf {
        self.get_custom_path().join(format!("{}.json", template_name))
    }

    /// 列出指定分类的所有模板文件
    pub fn list_template_files(&self, category: &str) -> Result<Vec<PathBuf>, TemplateError> {
        let category_path = self.get_builtin_path().join(category);

        if !category_path.exists() {
            return Ok(Vec::new());
        }

        let mut template_files = Vec::new();
        let entries = fs::read_dir(&category_path).map_err(|e| {
            error!("读取分类目录失败: {:?}, 错误: {}", category_path, e);
            TemplateError::FileSystemError { source: e }
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| TemplateError::FileSystemError { source: e })?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                template_files.push(path);
            }
        }

        Ok(template_files)
    }

    /// 检查模板文件是否存在
    pub fn template_file_exists(&self, category: &str, template_name: &str) -> bool {
        let file_path = self.get_template_file_path(category, template_name);
        file_path.exists()
    }

    /// 获取模板根目录
    pub fn get_templates_root(&self) -> &PathBuf {
        &self.templates_root
    }
}
