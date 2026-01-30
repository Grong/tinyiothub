use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row, Sqlite};
use std::collections::HashMap;

use crate::infrastructure::persistence::database::Database;

/// Organization entity - 组织实体
///
/// 使用 SQLx 最佳实践:
/// - 使用 snake_case 字段名映射到 PascalCase 数据库列
/// - 使用类型安全的查询构建
/// - 使用事务确保数据一致性
/// - 支持层级组织结构和树形操作
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<String>,
    pub created_at: String,
}

/// Query parameters for organization search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct OrganizationQueryParams {
    pub name: Option<String>,
    pub parent_id: Option<String>,
    pub include_children: Option<bool>,
    pub max_depth: Option<u32>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new organization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<String>,
}

/// Request for updating an organization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateOrganizationRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub parent_id: Option<String>,
}

/// Organization tree node
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OrganizationNode {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub children: Vec<OrganizationNode>,
    pub depth: u32,
    pub path: String, // Full path from root
}

/// Organization statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OrganizationStatistics {
    pub total_organizations: i64,
    pub root_organizations: i64,
    pub max_depth: u32,
    pub organizations_by_depth: Vec<DepthCount>,
}

/// Depth count for statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DepthCount {
    pub depth: u32,
    pub count: i64,
}

impl Organization {
    /// Find an organization by ID
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<Organization>, sqlx::Error> {
        let organization = sqlx::query_as::<_, Organization>(
            r#"
            SELECT id, name, description, parent_id, created_at
            FROM Organizations WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(organization)
    }

    /// Create a new organization
    pub async fn create(
        db: &Database,
        request: &CreateOrganizationRequest,
    ) -> Result<Organization, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // Validate parent exists if specified
        if let Some(parent_id) = &request.parent_id {
            if Self::find_by_id(db, parent_id).await?.is_none() {
                return Err(sqlx::Error::RowNotFound);
            }
        }

        // Use transaction for data consistency
        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO Organizations (id, name, description, parent_id, CreatedAt)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(&request.parent_id)
        .bind(&created_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // Return the created organization
        Self::find_by_id(db, &id)
            .await?
            .ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Update an organization
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateOrganizationRequest,
    ) -> Result<Organization, sqlx::Error> {
        // Validate parent exists if specified and prevent circular references
        if let Some(parent_id) = &request.parent_id {
            if parent_id == id {
                return Err(sqlx::Error::RowNotFound); // Cannot be parent of itself
            }

            if Self::find_by_id(db, parent_id).await?.is_none() {
                return Err(sqlx::Error::RowNotFound);
            }

            // Check for circular reference
            if Self::would_create_cycle(db, id, parent_id).await? {
                return Err(sqlx::Error::RowNotFound); // Would create cycle
            }
        }

        let mut query_builder = QueryBuilder::<Sqlite>::new("UPDATE Organizations SET ");
        let mut has_updates = false;

        if let Some(name) = &request.name {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("name = ").push_bind(name);
            has_updates = true;
        }

        if let Some(description) = &request.description {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("description = ").push_bind(description);
            has_updates = true;
        }

        if let Some(parent_id) = &request.parent_id {
            if has_updates {
                query_builder.push(", ");
            }
            query_builder.push("parent_id = ").push_bind(parent_id);
            has_updates = true;
        }

        if !has_updates {
            return Err(sqlx::Error::RowNotFound);
        }

        query_builder.push(" WHERE id = ").push_bind(id);

        let mut tx = db.pool().begin().await?;
        query_builder.build().execute(&mut *tx).await?;
        tx.commit().await?;

        // Return the updated organization
        Self::find_by_id(db, id)
            .await?
            .ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Delete an organization (and optionally its children)
    pub async fn delete(
        db: &Database,
        id: &str,
        delete_children: bool,
    ) -> Result<u64, sqlx::Error> {
        let mut tx = db.pool().begin().await?;
        let mut total_deleted = 0;

        if delete_children {
            // Recursively delete all children first
            let children = Self::find_children(db, id).await?;
            for child in children {
                total_deleted += Box::pin(Self::delete(db, &child.id, true)).await?;
            }
        } else {
            // Set parent_id to NULL for direct children
            sqlx::query("UPDATE Organizations SET parent_id = NULL WHERE parent_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }

        // Delete the organization itself
        let result = sqlx::query("DELETE FROM Organizations WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        total_deleted += result.rows_affected();

        Ok(total_deleted)
    }

    /// Find all organizations with optional filtering
    pub async fn find_all(
        db: &Database,
        params: &OrganizationQueryParams,
    ) -> Result<Vec<Organization>, sqlx::Error> {
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, name, description, parent_id, created_at
            FROM Organizations WHERE 1=1
            "#,
        );

        if let Some(name) = &params.name {
            query_builder
                .push(" AND name LIKE ")
                .push_bind(format!("%{}%", name));
        }

        if let Some(parent_id) = &params.parent_id {
            if params.include_children.unwrap_or(false) {
                // Include all descendants
                let descendants = Self::find_all_descendants(db, parent_id).await?;
                if !descendants.is_empty() {
                    query_builder
                        .push(" AND (parent_id = ")
                        .push_bind(parent_id);
                    query_builder.push(" OR id IN (");
                    let mut separated = query_builder.separated(", ");
                    for desc in descendants {
                        separated.push_bind(desc.id);
                    }
                    query_builder.push("))");
                } else {
                    query_builder.push(" AND parent_id = ").push_bind(parent_id);
                }
            } else {
                query_builder.push(" AND parent_id = ").push_bind(parent_id);
            }
        }

        query_builder.push(" ORDER BY name ASC");

        // Handle pagination
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query_builder.push(" LIMIT ").push_bind(page_size);
            query_builder.push(" OFFSET ").push_bind(offset);
        }

        let organizations = query_builder
            .build_query_as::<Organization>()
            .fetch_all(db.pool())
            .await?;

        Ok(organizations)
    }

    /// Count organizations with optional filtering
    pub async fn count(
        db: &Database,
        params: &OrganizationQueryParams,
    ) -> Result<i64, sqlx::Error> {
        let mut query_builder =
            QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM Organizations WHERE 1=1");

        if let Some(name) = &params.name {
            query_builder
                .push(" AND name LIKE ")
                .push_bind(format!("%{}%", name));
        }

        if let Some(parent_id) = &params.parent_id {
            if params.include_children.unwrap_or(false) {
                let descendants = Self::find_all_descendants(db, parent_id).await?;
                if !descendants.is_empty() {
                    query_builder
                        .push(" AND (parent_id = ")
                        .push_bind(parent_id);
                    query_builder.push(" OR id IN (");
                    let mut separated = query_builder.separated(", ");
                    for desc in descendants {
                        separated.push_bind(desc.id);
                    }
                    query_builder.push("))");
                } else {
                    query_builder.push(" AND parent_id = ").push_bind(parent_id);
                }
            } else {
                query_builder.push(" AND parent_id = ").push_bind(parent_id);
            }
        }

        let row = query_builder.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get(0);

        Ok(count)
    }

    /// Find root organizations (no parent)
    pub async fn find_root_organizations(db: &Database) -> Result<Vec<Organization>, sqlx::Error> {
        let organizations = sqlx::query_as::<_, Organization>(
            r#"
            SELECT id, name, description, parent_id, created_at
            FROM Organizations WHERE parent_id IS NULL
            ORDER BY name ASC
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        Ok(organizations)
    }

    /// Find direct child organizations
    pub async fn find_children(
        db: &Database,
        parent_id: &str,
    ) -> Result<Vec<Organization>, sqlx::Error> {
        let organizations = sqlx::query_as::<_, Organization>(
            r#"
            SELECT id, name, description, parent_id, created_at
            FROM Organizations WHERE parent_id = ?
            ORDER BY name ASC
            "#,
        )
        .bind(parent_id)
        .fetch_all(db.pool())
        .await?;

        Ok(organizations)
    }

    /// Find all descendants (recursive)
    pub async fn find_all_descendants(
        db: &Database,
        parent_id: &str,
    ) -> Result<Vec<Organization>, sqlx::Error> {
        let mut all_descendants = Vec::new();
        let mut to_process = vec![parent_id.to_string()];

        while let Some(current_id) = to_process.pop() {
            let children = Self::find_children(db, &current_id).await?;
            for child in children {
                to_process.push(child.id.clone());
                all_descendants.push(child);
            }
        }

        Ok(all_descendants)
    }

    /// Find ancestors (path to root)
    pub async fn find_ancestors(
        db: &Database,
        org_id: &str,
    ) -> Result<Vec<Organization>, sqlx::Error> {
        let mut ancestors = Vec::new();
        let mut current_id = org_id.to_string();

        while let Some(org) = Self::find_by_id(db, &current_id).await? {
            if let Some(parent_id) = &org.parent_id {
                if let Some(parent) = Self::find_by_id(db, parent_id).await? {
                    ancestors.push(parent.clone());
                    current_id = parent.id;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        ancestors.reverse(); // Root first
        Ok(ancestors)
    }

    /// Build organization tree
    pub async fn build_tree(db: &Database) -> Result<Vec<OrganizationNode>, sqlx::Error> {
        let all_orgs = Self::find_all(db, &OrganizationQueryParams::default()).await?;
        let mut org_map: HashMap<String, OrganizationNode> = HashMap::new();

        // Create nodes
        for org in all_orgs {
            let node = OrganizationNode {
                id: org.id.clone(),
                name: org.name.clone(),
                description: org.description.clone(),
                parent_id: org.parent_id.clone(),
                created_at: org.created_at.clone(),
                children: Vec::new(),
                depth: 0,
                path: org.name.clone(),
            };
            org_map.insert(org.id.clone(), node);
        }

        // Build tree structure and calculate depths/paths
        let mut roots = Vec::new();
        let mut children_map: HashMap<String, Vec<OrganizationNode>> = HashMap::new();

        for (_, node) in org_map {
            if let Some(parent_id) = &node.parent_id {
                children_map
                    .entry(parent_id.clone())
                    .or_default()
                    .push(node);
            } else {
                roots.push(node);
            }
        }

        // Recursively attach children and calculate depths/paths
        fn attach_children(
            node: &mut OrganizationNode,
            children_map: &mut HashMap<String, Vec<OrganizationNode>>,
            depth: u32,
            parent_path: &str,
        ) {
            node.depth = depth;
            node.path = if parent_path.is_empty() {
                node.name.clone()
            } else {
                format!("{} > {}", parent_path, node.name)
            };

            if let Some(mut children) = children_map.remove(&node.id) {
                children.sort_by(|a, b| a.name.cmp(&b.name));
                for child in &mut children {
                    attach_children(child, children_map, depth + 1, &node.path);
                }
                node.children = children;
            }
        }

        for root in &mut roots {
            attach_children(root, &mut children_map, 0, "");
        }

        roots.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(roots)
    }

    /// Check if organization has children
    pub async fn has_children(db: &Database, id: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT COUNT(*) FROM Organizations WHERE parent_id = ?")
            .bind(id)
            .fetch_one(db.pool())
            .await?;

        let count: i64 = row.get(0);
        Ok(count > 0)
    }

    /// Check if moving an organization would create a cycle
    pub async fn would_create_cycle(
        db: &Database,
        org_id: &str,
        new_parent_id: &str,
    ) -> Result<bool, sqlx::Error> {
        // Check if new_parent_id is a descendant of org_id
        let descendants = Self::find_all_descendants(db, org_id).await?;
        Ok(descendants.iter().any(|desc| desc.id == new_parent_id))
    }

    /// Get organization depth (distance from root)
    pub async fn get_depth(db: &Database, org_id: &str) -> Result<u32, sqlx::Error> {
        let ancestors = Self::find_ancestors(db, org_id).await?;
        Ok(ancestors.len() as u32)
    }

    /// Get organization statistics
    pub async fn get_statistics(db: &Database) -> Result<OrganizationStatistics, sqlx::Error> {
        let total_row = sqlx::query("SELECT COUNT(*) FROM Organizations")
            .fetch_one(db.pool())
            .await?;

        let root_row = sqlx::query("SELECT COUNT(*) FROM Organizations WHERE parent_id IS NULL")
            .fetch_one(db.pool())
            .await?;

        // Calculate depth statistics
        let all_orgs = Self::find_all(db, &OrganizationQueryParams::default()).await?;
        let mut depth_counts: HashMap<u32, i64> = HashMap::new();
        let mut max_depth = 0;

        for org in all_orgs {
            let depth = Self::get_depth(db, &org.id).await?;
            *depth_counts.entry(depth).or_insert(0) += 1;
            max_depth = max_depth.max(depth);
        }

        let mut organizations_by_depth: Vec<DepthCount> = depth_counts
            .into_iter()
            .map(|(depth, count)| DepthCount { depth, count })
            .collect();
        organizations_by_depth.sort_by_key(|dc| dc.depth);

        Ok(OrganizationStatistics {
            total_organizations: total_row.get(0),
            root_organizations: root_row.get(0),
            max_depth,
            organizations_by_depth,
        })
    }

    /// Find organizations with pagination and sorting
    pub async fn find_paginated(
        db: &Database,
        params: &OrganizationQueryParams,
        sort_by: Option<&str>,
        sort_order: Option<&str>,
    ) -> Result<(Vec<Organization>, i64), sqlx::Error> {
        // Get total count first
        let total_count = Self::count(db, params).await?;

        // Build the main query
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, name, description, parent_id, created_at
            FROM Organizations WHERE 1=1
            "#,
        );

        if let Some(name) = &params.name {
            query_builder
                .push(" AND name LIKE ")
                .push_bind(format!("%{}%", name));
        }

        if let Some(parent_id) = &params.parent_id {
            query_builder.push(" AND parent_id = ").push_bind(parent_id);
        }

        // Add sorting
        let sort_column = match sort_by {
            Some("name") => "Name",
            Some("createdAt") => "CreatedAt",
            _ => "Name",
        };

        let sort_direction = match sort_order {
            Some("desc") => "DESC",
            _ => "ASC",
        };

        query_builder.push(format!(" ORDER BY {} {}", sort_column, sort_direction));

        // Handle pagination
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query_builder.push(" LIMIT ").push_bind(page_size);
            query_builder.push(" OFFSET ").push_bind(offset);
        }

        let organizations = query_builder
            .build_query_as::<Organization>()
            .fetch_all(db.pool())
            .await?;

        Ok((organizations, total_count))
    }

    /// Check if organization name exists
    pub async fn exists_by_name(
        db: &Database,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let row = if let Some(parent_id) = parent_id {
            sqlx::query("SELECT COUNT(*) FROM Organizations WHERE name = ? AND parent_id = ?")
                .bind(name)
                .bind(parent_id)
                .fetch_one(db.pool())
                .await?
        } else {
            sqlx::query("SELECT COUNT(*) FROM Organizations WHERE name = ? AND parent_id IS NULL")
                .bind(name)
                .fetch_one(db.pool())
                .await?
        };

        let count: i64 = row.get(0);
        Ok(count > 0)
    }

    /// Move organization to new parent
    pub async fn move_to_parent(
        db: &Database,
        org_id: &str,
        new_parent_id: Option<&str>,
    ) -> Result<Organization, sqlx::Error> {
        // Validate new parent exists and no cycle would be created
        if let Some(parent_id) = new_parent_id {
            if Self::find_by_id(db, parent_id).await?.is_none() {
                return Err(sqlx::Error::RowNotFound);
            }

            if Self::would_create_cycle(db, org_id, parent_id).await? {
                return Err(sqlx::Error::RowNotFound);
            }
        }

        let mut tx = db.pool().begin().await?;

        sqlx::query("UPDATE Organizations SET parent_id = ? WHERE id = ?")
            .bind(new_parent_id)
            .bind(org_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Self::find_by_id(db, org_id)
            .await?
            .ok_or_else(|| sqlx::Error::RowNotFound)
    }

    // Helper methods for business logic

    /// Check if organization is root
    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }

    /// Get organization level (0 for root, 1 for first level, etc.)
    pub async fn get_level(&self, db: &Database) -> Result<u32, sqlx::Error> {
        Self::get_depth(db, &self.id).await
    }

    /// Validate organization data
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Organization name cannot be empty".to_string());
        }

        if self.name.len() > 255 {
            return Err("Organization name cannot exceed 255 characters".to_string());
        }

        if let Some(desc) = &self.description {
            if desc.len() > 1000 {
                return Err("Organization description cannot exceed 1000 characters".to_string());
            }
        }

        Ok(())
    }
}
