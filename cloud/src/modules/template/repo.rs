use std::sync::Arc;

use sqlx::{QueryBuilder, Row};
use tinyiothub_core::models::template_error::TemplateError;
use tracing::{debug, info, warn};

use super::types::{
    CreateDeviceTemplateRequest, DeviceTemplate, TemplateCategory, TemplateQueryParams,
    UpdateDeviceTemplateRequest,
};
use crate::shared::persistence::Database;

// ─── TemplateRepository ───────────────────────────────────────

/// 模板仓库 - 所有模板（内置和自定义）都存储在数据库中。
/// 内置模板通过 migration seed 写入 DB，无需文件系统加载。
#[derive(Debug)]
pub struct TemplateRepository {
    database: Arc<Database>,
    search_service: TemplateSearchService,
}

impl TemplateRepository {
    pub fn new(database: Arc<Database>) -> Self {
        let search_service = TemplateSearchService::new(database.clone());
        Self { database, search_service }
    }

    /// 查找所有模板（支持分页和筛选）
    pub async fn find_all(
        &self,
        params: &TemplateQueryParams,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        Ok(DeviceTemplate::find_all(&self.database, params, "").await?)
    }

    /// 根据ID查找模板
    pub async fn find_by_id(&self, id: &str) -> Result<Option<DeviceTemplate>, TemplateError> {
        Ok(DeviceTemplate::find_by_id(&self.database, id, "").await?)
    }

    /// 根据分类查找模板
    pub async fn find_by_category(
        &self,
        category: &str,
    ) -> Result<Vec<DeviceTemplate>, TemplateError> {
        Ok(DeviceTemplate::find_by_category(&self.database, category, "").await?)
    }

    /// 搜索模板
    pub async fn search(&self, keyword: &str) -> Result<Vec<DeviceTemplate>, TemplateError> {
        Ok(DeviceTemplate::search(&self.database, keyword, "", None).await?)
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

        if DeviceTemplate::exists_by_name(&self.database, &request.name).await? {
            return Err(TemplateError::TemplateNameExists { name: request.name.clone() });
        }

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

        if DeviceTemplate::find_by_id(&self.database, id, "").await?.is_none() {
            return Err(TemplateError::TemplateNotFound { id: id.to_string() });
        }

        if let Some(new_name) = &request.name {
            let existing = DeviceTemplate::find_by_name(&self.database, new_name, "").await?;
            if let Some(existing_template) = existing
                && existing_template.id != id
            {
                return Err(TemplateError::TemplateNameExists { name: new_name.clone() });
            }
        }

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

        if DeviceTemplate::find_by_id(&self.database, id, "").await?.is_none() {
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

    /// 获取模板分类
    pub async fn get_categories(&self) -> Result<Vec<TemplateCategory>, TemplateError> {
        Ok(TemplateCategory::get_categories(&self.database).await?)
    }

    /// 统计模板数量
    pub async fn count(&self, params: &TemplateQueryParams) -> Result<i64, TemplateError> {
        Ok(DeviceTemplate::count(&self.database, params, "").await?)
    }

    /// 检查模板名称是否存在
    pub async fn exists_by_name(&self, name: &str) -> Result<bool, TemplateError> {
        Ok(DeviceTemplate::exists_by_name(&self.database, name).await?)
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

        let templates =
            query.build_query_as::<DeviceTemplate>().fetch_all(self.database.pool()).await?;

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

        let templates =
            query.build_query_as::<DeviceTemplate>().fetch_all(self.database.pool()).await?;

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

        let templates =
            query.build_query_as::<DeviceTemplate>().fetch_all(self.database.pool()).await?;

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
