// Thing service — business logic layer

use sqlx::SqlitePool;

use super::{
    errors::ThingError,
    repo::ThingRepo,
    types::{
        CreateThingRequest, ListThingsParams, ListThingsResult, ThingProfileResponse,
        ThingResource, ThingResponse, ThingRow, ThingTreeNode, ThingType, UpdateThingRequest,
    },
};

pub struct ThingService {
    repo: ThingRepo,
    pool: SqlitePool,
}

impl ThingService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            repo: ThingRepo::new(pool.clone()),
            pool,
        }
    }

    // ──────────────────────────────────────────
    // List
    // ──────────────────────────────────────────

    pub async fn list_things(
        &self,
        workspace_id: &str,
        params: &ListThingsParams,
    ) -> Result<ListThingsResult, ThingError> {
        let (rows, total) = self.repo.list(workspace_id, params).await?;
        let limit = params.limit();
        let offset = params.offset();

        let mut items: Vec<ThingResponse> = Vec::with_capacity(rows.len());
        for row in &rows {
            let breadcrumb = self
                .repo
                .get_breadcrumb(&row.id, 10)
                .await
                .unwrap_or_default();
            items.push(Self::row_to_response(row, breadcrumb));
        }

        let unassigned = self
            .repo
            .list_unassigned_resources(workspace_id)
            .await
            .unwrap_or_default()
            .len() as u64;

        Ok(ListThingsResult {
            items,
            total,
            limit,
            offset,
            unassigned_resource_count: unassigned,
        })
    }

    // ──────────────────────────────────────────
    // Get
    // ──────────────────────────────────────────

    pub async fn get_thing(&self, id: &str) -> Result<ThingResponse, ThingError> {
        let row = self
            .repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| ThingError::NotFound(id.to_string()))?;

        let breadcrumb = self
            .repo
            .get_breadcrumb(id, 10)
            .await
            .unwrap_or_default();

        Ok(Self::row_to_response(&row, breadcrumb))
    }

    /// Full profile: thing + properties + recent events + knowledge docs.
    pub async fn get_thing_profile(
        &self,
        id: &str,
    ) -> Result<ThingProfileResponse, ThingError> {
        let thing = self.get_thing(id).await?;

        let properties = self.load_properties(id).await;
        let recent_events = self.load_recent_events(id).await;
        let knowledge_docs = self.load_knowledge_docs(id).await;

        Ok(ThingProfileResponse {
            thing,
            properties,
            recent_events,
            knowledge_docs,
        })
    }

    // ──────────────────────────────────────────
    // Tree
    // ──────────────────────────────────────────

    pub async fn get_thing_tree(
        &self,
        workspace_id: &str,
        root_id: Option<&str>,
        depth: Option<u32>,
    ) -> Result<Vec<ThingTreeNode>, ThingError> {
        let max_depth = depth.unwrap_or(10);
        Ok(self.repo.get_tree(root_id, workspace_id, max_depth).await?)
    }

    // ──────────────────────────────────────────
    // Create
    // ──────────────────────────────────────────

    pub async fn create_thing(
        &self,
        req: &CreateThingRequest,
        workspace_id: Option<&str>,
    ) -> Result<ThingResponse, ThingError> {
        // Validate thing_type
        let thing_type = req
            .thing_type
            .as_deref()
            .unwrap_or("device")
            .parse::<ThingType>()
            .map_err(|e| ThingError::ActionNotSupported(e))?;

        // Name conflict check within workspace (only if workspace provided)
        if let Some(ws) = workspace_id {
            if let Some(_existing) = self.repo.find_by_name(ws, &req.name).await? {
                return Err(ThingError::NameConflict(req.name.clone()));
            }
        }

        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let row = ThingRow {
            id: uuid::Uuid::new_v4().to_string(),
            name: req.name.clone(),
            display_name: None,
            thing_type: thing_type.to_string(),
            device_type: req.device_type.clone(),
            address: None,
            description: req.description.clone(),
            position: None,
            driver_name: req.driver_name.clone(),
            device_model: None,
            protocol_type: req.protocol_type.clone(),
            factory_name: None,
            linked_data: None,
            driver_options: None,
            state: 0,
            parent_id: req.parent_id.clone(),
            organization_id: None,
            tenant_id: None,
            workspace_id: workspace_id.map(|s| s.to_string()),
            linked_gateway: None,
            fingerprint: None,
            template_id: req.template_id.clone(),
            ontology_summary: None,
            summary_status: None,
            created_at: now.clone(),
            updated_at: now,
        };

        let created = self.repo.create(&row).await?;
        Ok(Self::row_to_response(&created, vec![]))
    }

    // ──────────────────────────────────────────
    // Update
    // ──────────────────────────────────────────

    pub async fn update_thing(
        &self,
        id: &str,
        req: &UpdateThingRequest,
    ) -> Result<ThingResponse, ThingError> {
        // Verify exists
        let existing = self
            .repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| ThingError::NotFound(id.to_string()))?;

        // Cycle check when changing parent
        if let Some(ref new_parent_id) = req.parent_id {
            let is_cycle = self.repo.check_cycle(id, new_parent_id).await?;
            if is_cycle {
                return Err(ThingError::CycleDetected {
                    thing_id: id.to_string(),
                    parent_id: new_parent_id.clone(),
                });
            }
        }

        // If name change: check conflict in same workspace
        if let Some(ref new_name) = req.name {
            if new_name != &existing.name {
                if let Some(ref ws) = existing.workspace_id {
                    if let Some(_conflict) = self.repo.find_by_name(ws, new_name).await? {
                        return Err(ThingError::NameConflict(new_name.clone()));
                    }
                }
            }
        }

        let updated = self
            .repo
            .update(id, req)
            .await?
            .ok_or_else(|| ThingError::NotFound(id.to_string()))?;

        let breadcrumb = self
            .repo
            .get_breadcrumb(id, 10)
            .await
            .unwrap_or_default();

        Ok(Self::row_to_response(&updated, breadcrumb))
    }

    // ──────────────────────────────────────────
    // Delete
    // ──────────────────────────────────────────

    pub async fn delete_thing(&self, id: &str) -> Result<(), ThingError> {
        // Check children first
        let children = self.repo.count_children(id).await?;
        if children > 0 {
            return Err(ThingError::HasChildren(children as usize));
        }

        let affected = self.repo.delete(id).await?;
        if affected == 0 {
            return Err(ThingError::NotFound(id.to_string()));
        }
        Ok(())
    }

    // ──────────────────────────────────────────
    // Resources
    // ──────────────────────────────────────────

    pub async fn attach_resource(
        &self,
        thing_id: &str,
        resource_id: &str,
    ) -> Result<(), ThingError> {
        // Verify thing exists
        self.repo
            .get_by_id(thing_id)
            .await?
            .ok_or_else(|| ThingError::NotFound(thing_id.to_string()))?;

        let affected = self.repo.attach_resource(thing_id, resource_id).await?;
        if affected == 0 {
            return Err(ThingError::NotFound(format!(
                "resource {} not found",
                resource_id
            )));
        }
        Ok(())
    }

    pub async fn list_unassigned_resources(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<ThingResource>, ThingError> {
        Ok(self.repo.list_unassigned_resources(workspace_id).await?)
    }

    // ──────────────────────────────────────────
    // Helpers
    // ──────────────────────────────────────────

    fn row_to_response(
        row: &ThingRow,
        breadcrumb: Vec<super::types::BreadcrumbNode>,
    ) -> ThingResponse {
        ThingResponse {
            id: row.id.clone(),
            workspace_id: row.workspace_id.clone(),
            name: row.name.clone(),
            device_type: row.device_type.clone(),
            thing_type: row.thing_type.clone(),
            parent_id: row.parent_id.clone(),
            template_id: row.template_id.clone(),
            state: row.state,
            driver_name: row.driver_name.clone(),
            protocol_type: row.protocol_type.clone(),
            ontology_summary: row.ontology_summary.clone(),
            summary_status: row.summary_status.clone(),
            breadcrumb,
            created_at: row.created_at.clone(),
            updated_at: row.updated_at.clone(),
        }
    }

    async fn load_properties(&self, device_id: &str) -> Option<Vec<serde_json::Value>> {
        let rows: Vec<PropertyRow> =
            sqlx::query_as::<_, PropertyRow>(
                "SELECT id, device_id, name, display_name, description, data_type, unit, \
                 min_value, max_value, default_value, is_read_only, created_at, updated_at \
                 FROM device_properties WHERE device_id = ? ORDER BY name",
            )
            .bind(device_id)
            .fetch_all(&self.pool)
            .await
            .ok()?;

        if rows.is_empty() {
            return None;
        }
        let values: Vec<serde_json::Value> = rows
            .into_iter()
            .filter_map(|r| serde_json::to_value(r).ok())
            .collect();
        Some(values)
    }

    async fn load_recent_events(&self, device_id: &str) -> Option<Vec<serde_json::Value>> {
        let rows: Vec<EventRow> =
            sqlx::query_as::<_, EventRow>(
                "SELECT id, event_type, event_subtype, level, source, source_id, \
                 title, content, metadata, created_at \
                 FROM events WHERE device_id = ? ORDER BY created_at DESC LIMIT 20",
            )
            .bind(device_id)
            .fetch_all(&self.pool)
            .await
            .ok()?;

        if rows.is_empty() {
            return None;
        }
        let values: Vec<serde_json::Value> = rows
            .into_iter()
            .filter_map(|r| serde_json::to_value(r).ok())
            .collect();
        Some(values)
    }

    async fn load_knowledge_docs(&self, device_id: &str) -> Option<Vec<serde_json::Value>> {
        let rows: Vec<DocRow> =
            sqlx::query_as::<_, DocRow>(
                "SELECT id, name, type, file_path, content, tags, created_at, updated_at \
                 FROM resources WHERE device_id = ? ORDER BY created_at DESC LIMIT 10",
            )
            .bind(device_id)
            .fetch_all(&self.pool)
            .await
            .ok()?;

        if rows.is_empty() {
            return None;
        }
        let values: Vec<serde_json::Value> = rows
            .into_iter()
            .filter_map(|r| serde_json::to_value(r).ok())
            .collect();
        Some(values)
    }
}

// ──────────────────────────────────────────────
// Internal query rows
// ──────────────────────────────────────────────

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
#[allow(dead_code)]
struct PropertyRow {
    id: String,
    device_id: String,
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    data_type: String,
    unit: Option<String>,
    min_value: Option<f64>,
    max_value: Option<f64>,
    default_value: Option<String>,
    is_read_only: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
#[allow(dead_code)]
struct EventRow {
    id: String,
    event_type: String,
    event_subtype: Option<String>,
    level: String,
    source: String,
    source_id: Option<String>,
    title: Option<String>,
    content: String,
    metadata: Option<String>,
    created_at: String,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
#[allow(dead_code)]
struct DocRow {
    id: String,
    name: String,
    #[sqlx(rename = "type")]
    resource_type: String,
    file_path: String,
    content: Option<String>,
    tags: String,
    created_at: String,
    updated_at: String,
}
