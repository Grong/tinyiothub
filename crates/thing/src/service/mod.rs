// Thing service — business logic layer

pub mod import_export;

use sqlx::SqlitePool;
use tinyiothub_core::models::{
    device_command::CreateDeviceCommandRequest, device_property::CreateDevicePropertyRequest,
};

use super::{
    errors::ThingError,
    repo::ThingRepo,
    summary::{self, StubLlmClient, SummaryComputer},
    types::{
        CreateThingRequest, ListThingsParams, ListThingsResult, ThingProfileResponse,
        ThingResource, ThingResponse, ThingRow, ThingTreeNode, ThingType, UpdateThingRequest,
    },
};
use tinyiothub_storage::{
    Database, create_device_command, create_device_properties_batch,
    find_device_commands_by_device_id, find_device_properties_by_device_id,
};

pub struct ThingService {
    repo: ThingRepo,
    pool: SqlitePool,
    summary_computer: SummaryComputer,
}

impl ThingService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { repo: ThingRepo::new(pool.clone()), pool, summary_computer: SummaryComputer::new() }
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

        // Tags + breadcrumbs for the whole page in ONE query each (T11 — was
        // one recursive CTE per row); DB errors surface instead of being
        // swallowed into empty breadcrumbs/tags.
        let thing_ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        let tags_map = self.repo.load_tags_batch(&thing_ids).await?;
        let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
        let mut breadcrumbs = self.repo.get_breadcrumbs(&ids, 10).await?;

        let mut items: Vec<ThingResponse> = Vec::with_capacity(rows.len());
        for row in &rows {
            let breadcrumb = breadcrumbs.remove(&row.id).unwrap_or_default();
            let mut resp = Self::row_to_response(row, breadcrumb);
            resp.tags = tags_map.get(&row.id).cloned().unwrap_or_default();
            items.push(resp);
        }

        let unassigned = self.repo.list_unassigned_resources(workspace_id).await?.len() as u64;

        Ok(ListThingsResult { items, total, limit, offset, unassigned_resource_count: unassigned })
    }

    // ──────────────────────────────────────────
    // Get
    // ──────────────────────────────────────────

    pub async fn get_thing(
        &self,
        id: &str,
        workspace_id: &str,
    ) -> Result<ThingResponse, ThingError> {
        let mut row = self
            .repo
            .get_by_id_scoped(id, workspace_id)
            .await?
            .ok_or_else(|| ThingError::NotFound(id.to_string()))?;

        // Lazy summary compute: trigger if status is not 'ok'
        if row.summary_status.as_deref() != Some("ok") {
            let llm = StubLlmClient;
            match self.summary_computer.get_or_compute(id, &self.pool, &llm).await {
                Ok(Some(summary)) => {
                    row.ontology_summary = Some(summary);
                    row.summary_status = Some("ok".to_string());
                }
                Ok(None) => { /* thing not found (should not happen) */ }
                Err(e) => {
                    tracing::warn!(?e, thing_id = %id, "Failed to compute ontology summary");
                }
            }
        }

        let breadcrumb = self.repo.get_breadcrumb(id, 10).await.unwrap_or_default();

        // Load tags for the single thing
        let tags_map = self.repo.load_tags_batch(&[id]).await?;
        let mut resp = Self::row_to_response(&row, breadcrumb);
        resp.tags = tags_map.get(id).cloned().unwrap_or_default();

        Ok(resp)
    }

    /// Full profile: thing + properties + recent events + knowledge docs.
    pub async fn get_thing_profile(
        &self,
        id: &str,
        workspace_id: &str,
    ) -> Result<ThingProfileResponse, ThingError> {
        let thing = self.get_thing(id, workspace_id).await?;

        let properties = self.load_properties(id).await.unwrap_or_default();
        let actions = self.load_actions(id).await.unwrap_or_default();
        let recent_events = self.load_recent_events(id).await.unwrap_or_default();
        let knowledge_docs = self.load_knowledge_docs(id).await.unwrap_or_default();

        Ok(ThingProfileResponse {
            thing,
            properties: Some(properties),
            actions: Some(actions),
            recent_events: Some(recent_events),
            knowledge_docs: Some(knowledge_docs),
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
        // Validate name (regression: the old device API rejected empty names)
        if req.name.trim().is_empty() {
            return Err(ThingError::Validation("name must not be empty".to_string()));
        }

        // Validate thing_type
        let thing_type = req
            .thing_type
            .as_deref()
            .unwrap_or("device")
            .parse::<ThingType>()
            .map_err(ThingError::ActionNotSupported)?;

        // Name conflict check within workspace (only if workspace provided)
        if let Some(ws) = workspace_id
            && let Some(_existing) = self.repo.find_by_name(ws, &req.name).await?
        {
            return Err(ThingError::NameConflict(req.name.clone()));
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
        if let Some(ref tid) = req.template_id {
            let _ = self.copy_template_props(&created.id, tid).await;
            let _ = self.copy_template_acts(&created.id, tid).await;
        }
        Ok(Self::row_to_response(&created, vec![]))
    }

    async fn copy_template_props(&self, thing_id: &str, tid: &str) -> Result<(), sqlx::Error> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT properties FROM thing_templates WHERE id=?")
                .bind(tid)
                .fetch_optional(&self.pool)
                .await?;
        let Some((json,)) = row else {
            return Ok(());
        };
        // Inserts go through the storage layer (single source of SQL for
        // thing_properties — eng-review T9)
        let requests: Vec<CreateDevicePropertyRequest> = serde_json::from_str::<
            Vec<serde_json::Value>,
        >(&json)
        .unwrap_or_default()
        .into_iter()
        .map(|p| CreateDevicePropertyRequest {
            device_id: thing_id.to_string(),
            name: p["name"].as_str().unwrap_or("").to_string(),
            display_name: p.get("displayName").and_then(|v| v.as_str()).map(|s| s.to_string()),
            description: None,
            data_type: Some(
                p.get("dataType").and_then(|v| v.as_str()).unwrap_or("string").to_string(),
            ),
            unit: Some(p.get("unit").and_then(|v| v.as_str()).unwrap_or("").to_string()),
            min_value: p.get("minValue").and_then(|v| v.as_f64()),
            max_value: p.get("maxValue").and_then(|v| v.as_f64()),
            default_value: p.get("defaultValue").and_then(|v| v.as_str()).map(|s| s.to_string()),
            is_read_only: Some(
                p.get("isReadOnly").and_then(|v| v.as_bool()).unwrap_or(false) as i32
            ),
        })
        .collect();
        let db = Database::new(self.pool.clone());
        create_device_properties_batch(&db, &requests).await?;
        Ok(())
    }

    async fn copy_template_acts(&self, thing_id: &str, tid: &str) -> Result<(), sqlx::Error> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT actions FROM thing_templates WHERE id=?")
                .bind(tid)
                .fetch_optional(&self.pool)
                .await?;
        let Some((json,)) = row else {
            return Ok(());
        };
        let db = Database::new(self.pool.clone());
        for a in serde_json::from_str::<Vec<serde_json::Value>>(&json).unwrap_or_default() {
            let req = CreateDeviceCommandRequest {
                device_id: thing_id.to_string(),
                name: a["name"].as_str().unwrap_or("").to_string(),
                display_name: a.get("displayName").and_then(|v| v.as_str()).map(|s| s.to_string()),
                description: None,
                parameters: a.get("parameters").map(|v| v.to_string()),
            };
            create_device_command(&db, &req).await?;
        }
        Ok(())
    }

    // ──────────────────────────────────────────
    // Update
    // ──────────────────────────────────────────

    pub async fn update_thing(
        &self,
        id: &str,
        req: &UpdateThingRequest,
        workspace_id: &str,
    ) -> Result<ThingResponse, ThingError> {
        // Verify exists in this workspace
        let existing = self
            .repo
            .get_by_id_scoped(id, workspace_id)
            .await?
            .ok_or_else(|| ThingError::NotFound(id.to_string()))?;

        // If name change: check conflict in same workspace
        if let Some(ref new_name) = req.name
            && new_name != &existing.name
            && let Some(ref ws) = existing.workspace_id
            && let Some(_conflict) = self.repo.find_by_name(ws, new_name).await?
        {
            return Err(ThingError::NameConflict(new_name.clone()));
        }

        // Cycle check + update in ONE transaction (TOCTOU-safe — T11)
        let updated = match self.repo.update_guarded(id, req, workspace_id).await? {
            super::types::UpdateGuardedOutcome::Cycle => {
                return Err(ThingError::CycleDetected {
                    thing_id: id.to_string(),
                    parent_id: req.parent_id.clone().unwrap_or_default(),
                });
            }
            super::types::UpdateGuardedOutcome::Updated(row) => {
                row.ok_or_else(|| ThingError::NotFound(id.to_string()))?
            }
        };

        // Dirty the summary subtree when the name or parent changed — the
        // breadcrumb is part of the summary input, so every descendant's
        // summary is now stale (design 二·③; eng-review T5).
        let name_changed = req.name.as_ref().is_some_and(|n| n != &existing.name);
        let parent_changed = req.parent_id.is_some() && req.parent_id != existing.parent_id;
        if (name_changed || parent_changed)
            && let Err(e) = summary::mark_dirty_for_name_or_parent_change(&self.pool, id).await
        {
            tracing::warn!(?e, thing_id = %id, "Failed to mark summary dirty after rename/reparent");
        }

        let breadcrumb = self.repo.get_breadcrumb(id, 10).await.unwrap_or_default();

        Ok(Self::row_to_response(&updated, breadcrumb))
    }

    // ──────────────────────────────────────────
    // Delete
    // ──────────────────────────────────────────

    pub async fn delete_thing(&self, id: &str, workspace_id: &str) -> Result<(), ThingError> {
        // Check children first
        let children = self.repo.count_children(id).await?;
        if children > 0 {
            return Err(ThingError::HasChildren(children as usize));
        }

        let affected = self.repo.delete_scoped(id, workspace_id).await?;
        if affected == 0 {
            return Err(ThingError::NotFound(id.to_string()));
        }
        Ok(())
    }

    // ──────────────────────────────────────────
    // Resources
    // ──────────────────────────────────────────

    /// Detach a resource from a thing (set device_id = NULL).
    pub async fn detach_resource(
        &self,
        thing_id: &str,
        resource_id: &str,
        workspace_id: &str,
    ) -> Result<(), ThingError> {
        let affected = self.repo.detach_resource(thing_id, resource_id, workspace_id).await?;
        if affected == 0 {
            return Err(ThingError::NotFound(format!(
                "resource {} not found on thing {}",
                resource_id, thing_id
            )));
        }

        // Symmetric with attach: removing a document changes the summary
        // input, so the cached summary is now stale (eng-review T5).
        if let Err(e) = summary::mark_dirty_for_resource_change(&self.pool, thing_id).await {
            tracing::warn!(?e, thing_id = %thing_id, "Failed to mark summary dirty after resource detach");
        }

        Ok(())
    }

    pub async fn attach_resource(
        &self,
        thing_id: &str,
        resource_id: &str,
        workspace_id: &str,
    ) -> Result<(), ThingError> {
        // Verify thing exists in this workspace; the repo update also
        // requires the resource to belong to the same workspace (T1).
        self.repo
            .get_by_id_scoped(thing_id, workspace_id)
            .await?
            .ok_or_else(|| ThingError::NotFound(thing_id.to_string()))?;

        let affected = self.repo.attach_resource(thing_id, resource_id, workspace_id).await?;
        if affected == 0 {
            return Err(ThingError::NotFound(format!("resource {} not found", resource_id)));
        }

        // Mark summary dirty so it will be recomputed on next read
        if let Err(e) = summary::mark_dirty_for_resource_change(&self.pool, thing_id).await {
            tracing::warn!(?e, thing_id = %thing_id, "Failed to mark summary dirty after resource attach");
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
            display_name: row.display_name.clone(),
            device_type: row.device_type.clone(),
            thing_type: row.thing_type.clone(),
            parent_id: row.parent_id.clone(),
            template_id: row.template_id.clone(),
            state: row.state,
            driver_name: row.driver_name.clone(),
            protocol_type: row.protocol_type.clone(),
            address: row.address.clone(),
            ontology_summary: row.ontology_summary.clone(),
            summary_status: row.summary_status.clone(),
            tags: vec![],
            breadcrumb,
            created_at: row.created_at.clone(),
            updated_at: row.updated_at.clone(),
        }
    }

    /// Load properties from the storage layer (single source of SQL for
    /// thing_properties — eng-review T9). No template fallback: the blueprint
    /// model means a thing with no instances has no properties (D6).
    async fn load_properties(&self, device_id: &str) -> Option<Vec<serde_json::Value>> {
        let db = Database::new(self.pool.clone());
        let props = find_device_properties_by_device_id(&db, device_id).await.ok()?;
        if props.is_empty() {
            return None;
        }
        // camelCase to match the existing profile API shape
        Some(
            props
                .into_iter()
                .map(|p| {
                    serde_json::json!({
                        "id": p.id,
                        "deviceId": p.device_id,
                        "name": p.name,
                        "displayName": p.display_name,
                        "description": p.description,
                        "dataType": p.data_type,
                        "unit": p.unit,
                        "minValue": p.min_value,
                        "maxValue": p.max_value,
                        "defaultValue": p.default_value,
                        "isReadOnly": p.is_read_only != 0,
                        "createdAt": p.created_at,
                        "updatedAt": p.updated_at,
                    })
                })
                .collect(),
        )
    }

    /// Load recent events (repo query — T9), mapped to the frontend's
    /// ThingEvent shape. Level int → name per the 4-level enum.
    async fn load_recent_events(&self, device_id: &str) -> Option<Vec<serde_json::Value>> {
        let rows = self.repo.list_recent_events(device_id, 20).await.ok()?;
        if rows.is_empty() {
            return None;
        }
        Some(
            rows.into_iter()
                .map(|r| {
                    let level = match r.event_level {
                        5 => "critical",
                        4 => "error",
                        3 => "warning",
                        _ => "info",
                    };
                    let content = r.content.unwrap_or_default();
                    serde_json::json!({
                        "id": r.id,
                        "title": r.title.unwrap_or_default(),
                        "message": r.event_subtype.clone().unwrap_or_default(),
                        "level": level,
                        "eventType": r.event_type,
                        "createdAt": r.created_at,
                        "contentPreview": content.chars().take(100).collect::<String>(),
                    })
                })
                .collect(),
        )
    }

    /// Load actions from the storage layer (single source of SQL for
    /// thing_actions — eng-review T9).
    async fn load_actions(&self, thing_id: &str) -> Option<Vec<serde_json::Value>> {
        let db = Database::new(self.pool.clone());
        let cmds = find_device_commands_by_device_id(&db, thing_id).await.ok()?;
        if cmds.is_empty() {
            return None;
        }
        Some(
            cmds.into_iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "deviceId": c.device_id,
                        "name": c.name,
                        "displayName": c.display_name,
                        "description": c.description,
                        "parameters": c.parameters,
                        "createdAt": c.created_at,
                    })
                })
                .collect(),
        )
    }

    async fn load_knowledge_docs(&self, device_id: &str) -> Option<Vec<serde_json::Value>> {
        let rows = self.repo.list_knowledge_docs(device_id, 10).await.ok()?;
        if rows.is_empty() {
            return None;
        }
        Some(rows.into_iter().filter_map(|r| serde_json::to_value(r).ok()).collect())
    }
}
