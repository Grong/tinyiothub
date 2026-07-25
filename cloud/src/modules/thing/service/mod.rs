// Thing service — business logic layer

pub mod a2ui;
pub mod import_export;

use sqlx::SqlitePool;

use super::{
    errors::ThingError,
    repo::ThingRepo,
    summary::{self, StubLlmClient, SummaryComputer},
    types::{
        CreateThingRequest, ListThingsParams, ListThingsResult, TagInfo, ThingProfileResponse,
        ThingResource, ThingResponse, ThingRow, ThingTreeNode, ThingType, UpdateThingRequest,
    },
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

        // Load tags for all things in batch
        let thing_ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        let tags_map = self.load_tags_batch(&thing_ids).await.unwrap_or_default();

        let mut items: Vec<ThingResponse> = Vec::with_capacity(rows.len());
        for row in &rows {
            let breadcrumb = self.repo.get_breadcrumb(&row.id, 10).await.unwrap_or_default();
            let mut resp = Self::row_to_response(row, breadcrumb);
            resp.tags = tags_map.get(&row.id).cloned().unwrap_or_default();
            items.push(resp);
        }

        let unassigned =
            self.repo.list_unassigned_resources(workspace_id).await.unwrap_or_default().len()
                as u64;

        Ok(ListThingsResult { items, total, limit, offset, unassigned_resource_count: unassigned })
    }

    // ──────────────────────────────────────────
    // Get
    // ──────────────────────────────────────────

    pub async fn get_thing(&self, id: &str) -> Result<ThingResponse, ThingError> {
        let mut row =
            self.repo.get_by_id(id).await?.ok_or_else(|| ThingError::NotFound(id.to_string()))?;

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
        let tags_map = self.load_tags_batch(&[id]).await.unwrap_or_default();
        let mut resp = Self::row_to_response(&row, breadcrumb);
        resp.tags = tags_map.get(id).cloned().unwrap_or_default();

        Ok(resp)
    }

    /// Full profile: thing + properties + recent events + knowledge docs.
    pub async fn get_thing_profile(&self, id: &str) -> Result<ThingProfileResponse, ThingError> {
        let thing = self.get_thing(id).await?;

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
        let row: Option<(String,)> = sqlx::query_as("SELECT properties FROM thing_templates WHERE id=?")
            .bind(tid).fetch_optional(&self.pool).await?;
        let Some((json,)) = row else { return Ok(()); };
        for p in serde_json::from_str::<Vec<serde_json::Value>>(&json).unwrap_or_default() {
            let nm = p["name"].as_str().unwrap_or("");
            let dp = p.get("displayName").and_then(|v| v.as_str()).unwrap_or(nm);
            sqlx::query("INSERT INTO thing_properties (id,device_id,name,display_name,data_type,unit,is_read_only,created_at,updated_at) VALUES (?,?,?,?,?,?,?,datetime('now'),datetime('now'))")
                .bind(uuid::Uuid::new_v4().to_string()).bind(thing_id).bind(nm).bind(dp)
                .bind(p.get("dataType").and_then(|v| v.as_str()).unwrap_or("string"))
                .bind(p.get("unit").and_then(|v| v.as_str()).unwrap_or(""))
                .bind((p.get("isReadOnly").and_then(|v| v.as_bool()).unwrap_or(false)) as i32)
                .execute(&self.pool).await?;
        }
        Ok(())
    }

    async fn copy_template_acts(&self, thing_id: &str, tid: &str) -> Result<(), sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as("SELECT actions FROM thing_templates WHERE id=?")
            .bind(tid).fetch_optional(&self.pool).await?;
        let Some((json,)) = row else { return Ok(()); };
        for a in serde_json::from_str::<Vec<serde_json::Value>>(&json).unwrap_or_default() {
            let nm = a["name"].as_str().unwrap_or("");
            let dp = a.get("displayName").and_then(|v| v.as_str()).unwrap_or(nm);
            sqlx::query("INSERT INTO thing_actions (id,device_id,name,display_name,parameters,created_at) VALUES (?,?,?,?,?,datetime('now'))")
                .bind(uuid::Uuid::new_v4().to_string()).bind(thing_id).bind(nm).bind(dp)
                .bind(a.get("parameters").map(|v| v.to_string()).as_deref())
                .execute(&self.pool).await?;
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
    ) -> Result<ThingResponse, ThingError> {
        // Verify exists
        let existing =
            self.repo.get_by_id(id).await?.ok_or_else(|| ThingError::NotFound(id.to_string()))?;

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
        if let Some(ref new_name) = req.name
            && new_name != &existing.name
            && let Some(ref ws) = existing.workspace_id
            && let Some(_conflict) = self.repo.find_by_name(ws, new_name).await?
        {
            return Err(ThingError::NameConflict(new_name.clone()));
        }

        let updated =
            self.repo.update(id, req).await?.ok_or_else(|| ThingError::NotFound(id.to_string()))?;

        let breadcrumb = self.repo.get_breadcrumb(id, 10).await.unwrap_or_default();

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

    /// Detach a resource from a thing (set device_id = NULL).
    pub async fn detach_resource(
        &self,
        thing_id: &str,
        resource_id: &str,
    ) -> Result<(), ThingError> {
        let affected = self.repo.detach_resource(thing_id, resource_id).await?;
        if affected == 0 {
            return Err(ThingError::NotFound(format!("resource {} not found on thing {}", resource_id, thing_id)));
        }
        Ok(())
    }

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

    /// Batch-load tags for multiple thing IDs from tag_bindings.
    async fn load_tags_batch(
        &self,
        thing_ids: &[&str],
    ) -> Result<std::collections::HashMap<String, Vec<super::types::TagInfo>>, sqlx::Error> {
        if thing_ids.is_empty() {
            return Ok(Default::default());
        }
        let mut qb = sqlx::QueryBuilder::new(
            "SELECT tb.target_id, t.id, t.name, t.color FROM tag_bindings tb \
             JOIN tags t ON t.id = tb.tag_id \
             WHERE tb.target_type IN ('device','thing') AND tb.target_id IN (",
        );
        let mut separated = qb.separated(",");
        for id in thing_ids {
            separated.push_bind(*id);
        }
        separated.push_unseparated(")");
        let rows = qb
            .build_query_as::<(String, String, String, Option<String>)>()
            .fetch_all(&self.pool)
            .await?;
        let mut map: std::collections::HashMap<String, Vec<TagInfo>> =
            std::collections::HashMap::new();
        for (target_id, id, name, color) in rows {
            map.entry(target_id).or_default().push(TagInfo { id, name, color });
        }
        Ok(map)
    }

    async fn load_properties(&self, device_id: &str) -> Option<Vec<serde_json::Value>> {
        let rows: Vec<PropertyRow> = sqlx::query_as::<_, PropertyRow>(
            "SELECT id, device_id, name, display_name, description, data_type, unit, \
                 min_value, max_value, default_value, is_read_only, created_at, updated_at \
                 FROM thing_properties WHERE device_id = ? ORDER BY name",
        )
        .bind(device_id)
        .fetch_all(&self.pool)
        .await
        .ok()?;

        if rows.is_empty() {
            if let Ok(Some((Some(ref json),))) = sqlx::query_as::<_, (Option<String>,)>(
                "SELECT t.properties FROM thing_templates t JOIN devices d ON d.template_id = t.id WHERE d.id = ?"
            ).bind(device_id).fetch_optional(&self.pool).await {
                return serde_json::from_str(json).ok();
            }
            return None;
        }
        let values: Vec<serde_json::Value> =
            rows.into_iter().filter_map(|r| serde_json::to_value(r).ok()).collect();
        Some(values)
    }

    async fn load_recent_events(&self, device_id: &str) -> Option<Vec<serde_json::Value>> {
        let rows: Vec<EventRow> = sqlx::query_as::<_, EventRow>(
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
        let values: Vec<serde_json::Value> =
            rows.into_iter().filter_map(|r| serde_json::to_value(r).ok()).collect();
        Some(values)
    }

    /// Load actions: template first, then device_commands table.
        /// Load actions from per-thing device_commands table.
    async fn load_actions(&self, thing_id: &str) -> Option<Vec<serde_json::Value>> {
        #[derive(Debug, serde::Serialize, sqlx::FromRow)]
        #[serde(rename_all = "camelCase")]
        struct CmdRow { id: String, device_id: String, name: String, display_name: Option<String>, description: Option<String>, parameters: Option<String>, created_at: String }
        let rows: Vec<CmdRow> = sqlx::query_as::<_, CmdRow>(
            "SELECT id,device_id,name,display_name,description,parameters,created_at FROM thing_actions WHERE device_id=? ORDER BY name",
        ).bind(thing_id).fetch_all(&self.pool).await.ok()?;
        if rows.is_empty() { return None; }
        Some(rows.into_iter().filter_map(|r| serde_json::to_value(r).ok()).collect())
    }

    async fn load_knowledge_docs(&self, device_id: &str) -> Option<Vec<serde_json::Value>> {
        let rows: Vec<DocRow> = sqlx::query_as::<_, DocRow>(
            "SELECT id, name, resource_type, description, file_path, content, tags, created_at, updated_at \
                 FROM resources WHERE device_id = ? ORDER BY created_at DESC LIMIT 10",
        )
        .bind(device_id)
        .fetch_all(&self.pool)
        .await
        .ok()?;

        if rows.is_empty() {
            return None;
        }
        let values: Vec<serde_json::Value> =
            rows.into_iter().filter_map(|r| serde_json::to_value(r).ok()).collect();
        Some(values)
    }
}

// ──────────────────────────────────────────────
// Internal query rows
// ──────────────────────────────────────────────

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct DocRow {
    id: String,
    name: String,
    resource_type: String,
    description: Option<String>,
    file_path: String,
    content: Option<String>,
    tags: String,
    created_at: String,
    updated_at: String,
}
