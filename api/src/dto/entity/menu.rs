use crate::infrastructure::persistence::database::Database;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};
use std::collections::HashMap;

/// 菜单实体 - 使用现代化 SQLx 实现
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Menu {
    pub id: String,
    pub title: Option<String>,
    pub sub_title: Option<String>,
    pub path: Option<String>,
    pub target: Option<String>,
    pub divided: Option<String>,
    pub icon: Option<String>,
    pub custom: Option<String>,
    pub header: Option<String>,
    pub menu_type: Option<String>,
    pub order: i32,
    pub parent_id: Option<String>,
    pub created_at: Option<String>,
}

/// 菜单树结构（包含子菜单）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MenuTree {
    pub id: String,
    pub title: Option<String>,
    pub sub_title: Option<String>,
    pub path: Option<String>,
    pub target: Option<String>,
    pub divided: Option<String>,
    pub icon: Option<String>,
    pub custom: Option<String>,
    pub header: Option<String>,
    pub menu_type: Option<String>,
    pub order: i32,
    pub parent_id: Option<String>,
    pub created_at: Option<String>,
    pub children: Vec<MenuTree>,
}

/// 菜单查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct MenuQueryParams {
    pub title: Option<String>,
    pub path: Option<String>,
    pub menu_type: Option<String>,
    pub parent_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建菜单请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateMenuRequest {
    pub title: Option<String>,
    pub sub_title: Option<String>,
    pub path: Option<String>,
    pub target: Option<String>,
    pub divided: Option<String>,
    pub icon: Option<String>,
    pub custom: Option<String>,
    pub header: Option<String>,
    pub menu_type: Option<String>,
    pub order: Option<i32>,
    pub parent_id: Option<String>,
}

/// 更新菜单请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateMenuRequest {
    pub title: Option<String>,
    pub sub_title: Option<String>,
    pub path: Option<String>,
    pub target: Option<String>,
    pub divided: Option<String>,
    pub icon: Option<String>,
    pub custom: Option<String>,
    pub header: Option<String>,
    pub menu_type: Option<String>,
    pub order: Option<i32>,
    pub parent_id: Option<String>,
}

impl Menu {
    /// 根据 ID 查找菜单
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<Menu>, sqlx::Error> {
        let menu = sqlx::query_as::<_, Menu>(
            r#"
            SELECT id as id, Title as title, sub_title as sub_title, Path as path, 
                   Target as target, Divided as divided, Icon as icon, Custom as custom, 
                   Header as header, type as menu_type, `Order` as `order`, 
                   parent_id as parent_id, created_at as created_at
            FROM Menus WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(menu)
    }

    /// 创建新菜单
    pub async fn create(db: &Database, request: &CreateMenuRequest) -> Result<Menu, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let order_value = request.order.unwrap_or(1);

        sqlx::query(
            r#"
            INSERT INTO Menus (
                id, Title, sub_title, Path, Target, Divided, Icon, Custom, Header,
                type, `Order`, parent_id, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.title)
        .bind(&request.sub_title)
        .bind(&request.path)
        .bind(&request.target)
        .bind(&request.divided)
        .bind(&request.icon)
        .bind(&request.custom)
        .bind(&request.header)
        .bind(&request.menu_type)
        .bind(order_value)
        .bind(&request.parent_id)
        .bind(&now)
        .execute(db.pool())
        .await?;

        // 返回创建的菜单
        Self::find_by_id(db, &id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新菜单信息
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateMenuRequest,
    ) -> Result<Menu, sqlx::Error> {
        let mut query = QueryBuilder::new("UPDATE Menus SET ");
        let mut has_updates = false;

        if let Some(title) = &request.title {
            if has_updates {
                query.push(", ");
            }
            query.push("Title = ").push_bind(title);
            has_updates = true;
        }

        if let Some(sub_title) = &request.sub_title {
            if has_updates {
                query.push(", ");
            }
            query.push("sub_title = ").push_bind(sub_title);
            has_updates = true;
        }

        if let Some(path) = &request.path {
            if has_updates {
                query.push(", ");
            }
            query.push("Path = ").push_bind(path);
            has_updates = true;
        }

        if let Some(target) = &request.target {
            if has_updates {
                query.push(", ");
            }
            query.push("Target = ").push_bind(target);
            has_updates = true;
        }

        if let Some(divided) = &request.divided {
            if has_updates {
                query.push(", ");
            }
            query.push("Divided = ").push_bind(divided);
            has_updates = true;
        }

        if let Some(icon) = &request.icon {
            if has_updates {
                query.push(", ");
            }
            query.push("Icon = ").push_bind(icon);
            has_updates = true;
        }

        if let Some(custom) = &request.custom {
            if has_updates {
                query.push(", ");
            }
            query.push("Custom = ").push_bind(custom);
            has_updates = true;
        }

        if let Some(header) = &request.header {
            if has_updates {
                query.push(", ");
            }
            query.push("Header = ").push_bind(header);
            has_updates = true;
        }

        if let Some(menu_type) = &request.menu_type {
            if has_updates {
                query.push(", ");
            }
            query.push("type = ").push_bind(menu_type);
            has_updates = true;
        }

        if let Some(order) = request.order {
            if has_updates {
                query.push(", ");
            }
            query.push("`Order` = ").push_bind(order);
            has_updates = true;
        }

        if let Some(parent_id) = &request.parent_id {
            if has_updates {
                query.push(", ");
            }
            query.push("parent_id = ").push_bind(parent_id);
            has_updates = true;
        }

        if !has_updates {
            return Self::find_by_id(db, id)
                .await?
                .ok_or(sqlx::Error::RowNotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(db.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Self::find_by_id(db, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除菜单
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM Menus WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// 批量删除菜单
    pub async fn delete_by_ids(db: &Database, ids: &[String]) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut query = QueryBuilder::new("DELETE FROM Menus WHERE id IN (");
        let mut separated = query.separated(", ");

        for id in ids {
            separated.push_bind(id);
        }

        separated.push_unseparated(")");

        let result = query.build().execute(db.pool()).await?;
        Ok(result.rows_affected())
    }

    /// 查询菜单列表（支持分页和筛选）
    pub async fn find_all(
        db: &Database,
        params: &MenuQueryParams,
    ) -> Result<Vec<Menu>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, Title, sub_title, Path, Target, Divided, Icon, Custom, Header,
                   type, `Order`, parent_id, created_at
            FROM Menus WHERE 1=1
            "#,
        );

        // 动态添加查询条件
        if let Some(title) = &params.title {
            query
                .push(" AND Title LIKE ")
                .push_bind(format!("%{}%", title));
        }

        if let Some(path) = &params.path {
            query
                .push(" AND Path LIKE ")
                .push_bind(format!("%{}%", path));
        }

        if let Some(menu_type) = &params.menu_type {
            query.push(" AND type = ").push_bind(menu_type);
        }

        if let Some(parent_id) = &params.parent_id {
            query.push(" AND parent_id = ").push_bind(parent_id);
        }

        // 添加排序
        query.push(" ORDER BY `Order`, Title");

        // 添加分页
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let menus = query.build_query_as::<Menu>().fetch_all(db.pool()).await?;

        Ok(menus)
    }

    /// 统计菜单数量
    pub async fn count(db: &Database, params: &MenuQueryParams) -> Result<i64, sqlx::Error> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM Menus WHERE 1=1");

        if let Some(title) = &params.title {
            query
                .push(" AND Title LIKE ")
                .push_bind(format!("%{}%", title));
        }

        if let Some(path) = &params.path {
            query
                .push(" AND Path LIKE ")
                .push_bind(format!("%{}%", path));
        }

        if let Some(menu_type) = &params.menu_type {
            query.push(" AND type = ").push_bind(menu_type);
        }

        if let Some(parent_id) = &params.parent_id {
            query.push(" AND parent_id = ").push_bind(parent_id);
        }

        let row = query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// 获取根菜单（无父菜单）
    pub async fn find_root_menus(db: &Database) -> Result<Vec<Menu>, sqlx::Error> {
        let menus = sqlx::query_as::<_, Menu>(
            r#"
            SELECT id as id, Title as title, sub_title as sub_title, Path as path, 
                   Target as target, Divided as divided, Icon as icon, Custom as custom, 
                   Header as header, type as menu_type, `Order` as `order`, 
                   parent_id as parent_id, created_at as created_at
            FROM Menus 
            WHERE parent_id IS NULL OR parent_id = ''
            ORDER BY `Order`, Title
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        Ok(menus)
    }

    /// 根据父菜单 ID 查询子菜单
    pub async fn find_by_parent_id(
        db: &Database,
        parent_id: &str,
    ) -> Result<Vec<Menu>, sqlx::Error> {
        let menus = sqlx::query_as::<_, Menu>(
            r#"
            SELECT id as id, Title as title, sub_title as sub_title, Path as path, 
                   Target as target, Divided as divided, Icon as icon, Custom as custom, 
                   Header as header, type as menu_type, `Order` as `order`, 
                   parent_id as parent_id, created_at as created_at
            FROM Menus WHERE parent_id = ?
            ORDER BY `Order`, Title
            "#,
        )
        .bind(parent_id)
        .fetch_all(db.pool())
        .await?;

        Ok(menus)
    }

    /// 获取菜单树结构
    pub async fn get_menu_tree(db: &Database) -> Result<Vec<MenuTree>, sqlx::Error> {
        // 获取所有菜单
        let all_menus = sqlx::query_as::<_, Menu>(
            r#"
            SELECT id as id, Title as title, sub_title as sub_title, Path as path, 
                   Target as target, Divided as divided, Icon as icon, Custom as custom, 
                   Header as header, type as menu_type, `Order` as `order`, 
                   parent_id as parent_id, created_at as created_at
            FROM Menus ORDER BY `Order`, Title
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        // 构建菜单映射
        let mut menu_map: HashMap<String, MenuTree> = HashMap::new();
        let mut root_menus = Vec::new();

        // 第一遍：创建所有菜单节点
        for menu in all_menus {
            let menu_tree = MenuTree {
                id: menu.id.clone(),
                title: menu.title,
                sub_title: menu.sub_title,
                path: menu.path,
                target: menu.target,
                divided: menu.divided,
                icon: menu.icon,
                custom: menu.custom,
                header: menu.header,
                menu_type: menu.menu_type,
                order: menu.order,
                parent_id: menu.parent_id.clone(),
                created_at: menu.created_at,
                children: Vec::new(),
            };

            let is_root = menu.parent_id.is_none()
                || menu
                    .parent_id
                    .as_ref()
                    .map(|s| s.is_empty())
                    .unwrap_or(true);

            if is_root {
                root_menus.push(menu_tree.clone());
            }

            menu_map.insert(menu.id, menu_tree);
        }

        // 第二遍：构建树结构
        Self::build_menu_tree_recursive(&mut root_menus, &menu_map);

        // 排序根菜单
        root_menus.sort_by(|a, b| a.order.cmp(&b.order));

        Ok(root_menus)
    }

    /// 递归构建菜单树
    fn build_menu_tree_recursive(menus: &mut Vec<MenuTree>, menu_map: &HashMap<String, MenuTree>) {
        for menu in menus {
            // 查找当前菜单的子菜单
            for child_menu in menu_map.values() {
                if let Some(parent_id) = &child_menu.parent_id {
                    if parent_id == &menu.id {
                        menu.children.push(child_menu.clone());
                    }
                }
            }

            // 排序子菜单
            menu.children.sort_by(|a, b| a.order.cmp(&b.order));

            // 递归处理子菜单
            Self::build_menu_tree_recursive(&mut menu.children, menu_map);
        }
    }

    /// 检查菜单路径是否存在
    pub async fn exists_by_path(db: &Database, path: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM Menus WHERE Path = ?")
            .bind(path)
            .fetch_one(db.pool())
            .await?;

        Ok(count > 0)
    }

    /// 检查菜单路径是否存在（排除指定 ID）
    pub async fn exists_by_path_exclude_id(
        db: &Database,
        path: &str,
        exclude_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM Menus WHERE Path = ? AND id != ?")
                .bind(path)
                .bind(exclude_id)
                .fetch_one(db.pool())
                .await?;

        Ok(count > 0)
    }

    /// 获取菜单的最大排序值
    pub async fn get_max_order(db: &Database, parent_id: Option<&str>) -> Result<i32, sqlx::Error> {
        let max_order: i32 = if let Some(parent_id) = parent_id {
            sqlx::query_scalar("SELECT COALESCE(MAX(`Order`), 0) FROM Menus WHERE parent_id = ?")
                .bind(parent_id)
                .fetch_one(db.pool())
                .await?
        } else {
            sqlx::query_scalar(
                "SELECT COALESCE(MAX(`Order`), 0) FROM Menus WHERE parent_id IS NULL OR parent_id = ''"
            )
            .fetch_one(db.pool())
            .await?
        };

        Ok(max_order)
    }

    /// 根据 ID 列表查询菜单
    pub async fn find_by_ids(db: &Database, ids: &[String]) -> Result<Vec<Menu>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut query = QueryBuilder::new(
            r#"
            SELECT id, Title, sub_title, Path, Target, Divided, Icon, Custom, Header,
                   type, `Order`, parent_id, created_at
            FROM Menus WHERE id IN (
            "#,
        );

        let mut separated = query.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let menus = query.build_query_as::<Menu>().fetch_all(db.pool()).await?;

        Ok(menus)
    }
}

impl Default for Menu {
    fn default() -> Self {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: None,
            sub_title: None,
            path: None,
            target: None,
            divided: None,
            icon: None,
            custom: None,
            header: None,
            menu_type: None,
            order: 1,
            parent_id: None,
            created_at: Some(now),
        }
    }
}

impl Default for MenuTree {
    fn default() -> Self {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: None,
            sub_title: None,
            path: None,
            target: None,
            divided: None,
            icon: None,
            custom: None,
            header: None,
            menu_type: None,
            order: 1,
            parent_id: None,
            created_at: Some(now),
            children: Vec::new(),
        }
    }
}

// 为了向后兼容，保留旧的 DTO 别名
pub type MenuDto = MenuTree;
