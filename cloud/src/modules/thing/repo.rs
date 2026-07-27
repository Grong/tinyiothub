// Thing repository — database access layer

use sqlx::{QueryBuilder, SqlitePool};

use super::types::{ListThingsParams, ThingResource, ThingRow};

/// Single-query cycle check: walk UP from the candidate parent; if the
/// thing itself is on that chain, reparenting creates a cycle. Depth cap 10
/// matches the read-side breadcrumb/tree caps.
const CHECK_CYCLE_SQL: &str = "WITH RECURSIVE up AS ( \
    SELECT id, parent_id, 0 AS depth FROM devices WHERE id = ? \
    UNION ALL \
    SELECT d.id, d.parent_id, up.depth + 1 FROM devices d JOIN up ON d.id = up.parent_id \
    WHERE up.depth < 10 \
) SELECT EXISTS(SELECT 1 FROM up WHERE id = ?)";

pub struct ThingRepo {
    pool: SqlitePool,
}

impl ThingRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // ──────────────────────────────────────────
    // Query
    // ──────────────────────────────────────────

    /// Workspace-scoped lookup (eng-review T1): strict workspace match,
    /// same semantics as list().
    pub async fn get_by_id_scoped(
        &self,
        id: &str,
        workspace_id: &str,
    ) -> Result<Option<ThingRow>, sqlx::Error> {
        sqlx::query_as::<_, ThingRow>("SELECT * FROM devices WHERE id = ? AND workspace_id = ?")
            .bind(id)
            .bind(workspace_id)
            .fetch_optional(&self.pool)
            .await
    }

    /// Workspace-scoped delete (eng-review T1): refuses to delete another
    /// workspace's thing.
    pub async fn delete_scoped(&self, id: &str, workspace_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM devices WHERE id = ? AND workspace_id = ?")
            .bind(id)
            .bind(workspace_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Workspace-scoped name lookup.
    pub async fn find_by_name(
        &self,
        workspace_id: &str,
        name: &str,
    ) -> Result<Option<ThingRow>, sqlx::Error> {
        sqlx::query_as::<_, ThingRow>("SELECT * FROM devices WHERE workspace_id = ? AND name = ?")
            .bind(workspace_id)
            .bind(name)
            .fetch_optional(&self.pool)
            .await
    }

    /// List things with dynamic WHERE + pagination.
    pub async fn list(
        &self,
        workspace_id: &str,
        params: &ListThingsParams,
    ) -> Result<(Vec<ThingRow>, u64), sqlx::Error> {
        let limit = params.limit() as i64;
        let offset = params.offset() as i64;

        // Build COUNT query
        let mut count_builder =
            QueryBuilder::new("SELECT COUNT(*) FROM devices WHERE workspace_id = ");
        count_builder.push_bind(workspace_id);
        Self::push_where_clauses(&mut count_builder, params);

        let total: i64 = count_builder.build_query_scalar().fetch_one(&self.pool).await?;

        // Build SELECT query
        let mut select_builder = QueryBuilder::new("SELECT * FROM devices WHERE workspace_id = ");
        select_builder.push_bind(workspace_id);
        Self::push_where_clauses(&mut select_builder, params);
        select_builder.push(" ORDER BY created_at DESC LIMIT ");
        select_builder.push_bind(limit);
        select_builder.push(" OFFSET ");
        select_builder.push_bind(offset);

        let rows = select_builder.build_query_as::<ThingRow>().fetch_all(&self.pool).await?;

        Ok((rows, total as u64))
    }

    /// Push additional WHERE clauses into the builder (without leading WHERE).
    fn push_where_clauses(builder: &mut QueryBuilder<sqlx::Sqlite>, params: &ListThingsParams) {
        if let Some(ref tt) = params.thing_type {
            builder.push(" AND thing_type = ");
            builder.push_bind(tt);
        }
        if let Some(ref pid) = params.parent_id {
            builder.push(" AND parent_id = ");
            builder.push_bind(pid);
        }
        if let Some(ref q) = params.q {
            builder.push(" AND (name LIKE ");
            builder.push_bind(format!("%{}%", q));
            builder.push(" OR description LIKE ");
            builder.push_bind(format!("%{}%", q));
            builder.push(")");
        }
    }

    /// Single thing by id.
    pub async fn get_by_id(&self, id: &str) -> Result<Option<ThingRow>, sqlx::Error> {
        sqlx::query_as::<_, ThingRow>("SELECT * FROM devices WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    // ──────────────────────────────────────────
    // Mutations
    // ──────────────────────────────────────────

    /// INSERT — returns the newly created row.
    pub async fn create(&self, row: &ThingRow) -> Result<ThingRow, sqlx::Error> {
        sqlx::query(
            "INSERT INTO devices (id, name, display_name, thing_type, device_type, \
             description, parent_id, template_id, protocol_type, driver_name, \
             workspace_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.name)
        .bind(&row.display_name)
        .bind(&row.thing_type)
        .bind(&row.device_type)
        .bind(&row.description)
        .bind(&row.parent_id)
        .bind(&row.template_id)
        .bind(&row.protocol_type)
        .bind(&row.driver_name)
        .bind(&row.workspace_id)
        .bind(&row.created_at)
        .bind(&row.updated_at)
        .execute(&self.pool)
        .await?;

        self.get_by_id(&row.id)
            .await?
            .ok_or_else(|| sqlx::Error::Protocol("readback after insert failed".into()))
    }

    /// UPDATE — returns the updated row.
    pub async fn update(
        &self,
        id: &str,
        input: &super::types::UpdateThingRequest,
    ) -> Result<Option<ThingRow>, sqlx::Error> {
        let mut builder = QueryBuilder::new("UPDATE devices SET ");
        let mut separated = builder.separated(", ");
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        separated.push("updated_at = ").push_bind_unseparated(&now);

        if let Some(ref name) = input.name {
            separated.push("name = ").push_bind_unseparated(name);
        }
        if let Some(ref tt) = input.thing_type {
            separated.push("thing_type = ").push_bind_unseparated(tt);
        }
        if let Some(ref dt) = input.device_type {
            separated.push("device_type = ").push_bind_unseparated(dt);
        }
        if let Some(ref desc) = input.description {
            separated.push("description = ").push_bind_unseparated(desc);
        }
        if let Some(ref pid) = input.parent_id {
            separated.push("parent_id = ").push_bind_unseparated(pid);
        }
        if let Some(ref tid) = input.template_id {
            separated.push("template_id = ").push_bind_unseparated(tid);
        }
        if let Some(ref proto) = input.protocol_type {
            separated.push("protocol_type = ").push_bind_unseparated(proto);
        }
        if let Some(ref driver) = input.driver_name {
            separated.push("driver_name = ").push_bind_unseparated(driver);
        }

        builder.push(" WHERE id = ");
        builder.push_bind(id);

        builder.build().execute(&self.pool).await?;

        self.get_by_id(id).await
    }

    /// DELETE — checks children count first.
    /// Returns `Some(child_count)` if children exist, or rows_affected on success.
    pub async fn delete(&self, id: &str) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM devices WHERE id = ?").bind(id).execute(&self.pool).await?;

        Ok(result.rows_affected())
    }

    /// Count children of a thing.
    pub async fn count_children(&self, id: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE parent_id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
    }

    // ──────────────────────────────────────────
    // Tree / Hierarchy
    // ──────────────────────────────────────────

    /// Recursive CTE: all descendants of `root_id` (or full workspace tree if None).
    pub async fn get_tree(
        &self,
        root_id: Option<&str>,
        workspace_id: &str,
        max_depth: u32,
    ) -> Result<Vec<super::types::ThingTreeNode>, sqlx::Error> {
        let depth_val = max_depth.min(20) as i32;

        let root_prefix = "WITH RECURSIVE subtree AS ( \
            SELECT id, name, thing_type, parent_id, 0 AS depth FROM devices WHERE ";

        let union_part = " UNION ALL \
            SELECT d.id, d.name, d.thing_type, d.parent_id, s.depth + 1 \
            FROM devices d JOIN subtree s ON d.parent_id = s.id \
            WHERE s.depth < ";

        let select_part = ") SELECT id, name, thing_type, parent_id, depth FROM subtree";

        let mut builder = QueryBuilder::new(root_prefix);

        if let Some(rid) = root_id {
            builder.push("id = ");
            builder.push_bind(rid);
            builder.push(" AND workspace_id = ");
            builder.push_bind(workspace_id);
        } else {
            builder.push("workspace_id = ");
            builder.push_bind(workspace_id);
            builder.push(" AND parent_id IS NULL");
        }

        builder.push(union_part);
        builder.push_bind(depth_val);
        builder.push(select_part);

        let rows = builder.build_query_as::<TreeNodeRow>().fetch_all(&self.pool).await?;

        Ok(Self::build_tree(rows))
    }

    fn build_tree(rows: Vec<TreeNodeRow>) -> Vec<super::types::ThingTreeNode> {
        let mut nodes: std::collections::HashMap<String, super::types::ThingTreeNode> =
            std::collections::HashMap::new();

        for row in &rows {
            nodes.insert(
                row.id.clone(),
                super::types::ThingTreeNode {
                    id: row.id.clone(),
                    name: row.name.clone(),
                    thing_type: row.thing_type.clone(),
                    children: vec![],
                },
            );
        }

        let mut roots: Vec<super::types::ThingTreeNode> = vec![];
        for row in &rows {
            let Some(node) = nodes.remove(&row.id) else {
                tracing::warn!(id = %row.id, "Thing tree: node not found in map, skipping");
                continue;
            };
            if let Some(ref pid) = row.parent_id {
                if let Some(parent) = nodes.get_mut(pid) {
                    parent.children.push(node);
                } else {
                    roots.push(node);
                }
            } else {
                roots.push(node);
            }
        }

        fn sort_tree(nodes: &mut [super::types::ThingTreeNode]) {
            nodes.sort_by(|a, b| a.name.cmp(&b.name));
            for n in nodes.iter_mut() {
                sort_tree(&mut n.children);
            }
        }
        sort_tree(&mut roots);

        roots
    }

    /// Breadcrumb: walk parent chain up from `id`, max depth 10.
    pub async fn get_breadcrumb(
        &self,
        id: &str,
        max_depth: u32,
    ) -> Result<Vec<super::types::BreadcrumbNode>, sqlx::Error> {
        let depth_val = (max_depth.min(10) as i32).to_string();

        let mut builder = QueryBuilder::new(
            "WITH RECURSIVE ancestors AS ( \
             SELECT id, name, thing_type, parent_id, 0 AS depth FROM devices WHERE id = ",
        );
        builder.push_bind(id);
        builder.push(
            " UNION ALL \
             SELECT d.id, d.name, d.thing_type, d.parent_id, a.depth + 1 \
             FROM devices d JOIN ancestors a ON d.id = a.parent_id \
             WHERE a.depth < ",
        );
        builder.push(depth_val);
        builder.push(") SELECT id, name, thing_type FROM ancestors ORDER BY depth DESC");

        let rows: Vec<BreadcrumbRow> =
            builder.build_query_as::<BreadcrumbRow>().fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(|r| super::types::BreadcrumbNode {
                id: r.id,
                name: r.name,
                thing_type: r.thing_type,
            })
            .collect())
    }

    /// Cycle detection: single recursive-CTE walk UP from
    /// `candidate_parent_id` (eng-review T11 — was up to 50 sequential
    /// round-trips with a depth-50 cap that contradicted the design's 10).
    /// Returns `true` if `thing_id` is on the candidate's ancestor chain.
    pub async fn check_cycle(
        &self,
        thing_id: &str,
        candidate_parent_id: &str,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(CHECK_CYCLE_SQL)
            .bind(candidate_parent_id)
            .bind(thing_id)
            .fetch_one(&self.pool)
            .await
    }

    /// UPDATE with cycle check in ONE transaction (eng-review T11).
    /// The check's read holds a SHARED lock until commit, so a concurrent
    /// reparent cannot race a cycle past the guard (was: check-then-update
    /// TOCTOU).
    pub async fn update_guarded(
        &self,
        id: &str,
        input: &super::types::UpdateThingRequest,
        workspace_id: &str,
    ) -> Result<super::types::UpdateGuardedOutcome, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        if let Some(ref new_parent_id) = input.parent_id {
            let is_cycle: bool = sqlx::query_scalar(CHECK_CYCLE_SQL)
                .bind(new_parent_id)
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            if is_cycle {
                return Ok(super::types::UpdateGuardedOutcome::Cycle);
            }
        }

        let mut builder = QueryBuilder::new("UPDATE devices SET ");
        let mut separated = builder.separated(", ");
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        separated.push("updated_at = ").push_bind_unseparated(&now);
        if let Some(ref name) = input.name {
            separated.push("name = ").push_bind_unseparated(name);
        }
        if let Some(ref tt) = input.thing_type {
            separated.push("thing_type = ").push_bind_unseparated(tt);
        }
        if let Some(ref dt) = input.device_type {
            separated.push("device_type = ").push_bind_unseparated(dt);
        }
        if let Some(ref desc) = input.description {
            separated.push("description = ").push_bind_unseparated(desc);
        }
        if let Some(ref pid) = input.parent_id {
            separated.push("parent_id = ").push_bind_unseparated(pid);
        }
        if let Some(ref tid) = input.template_id {
            separated.push("template_id = ").push_bind_unseparated(tid);
        }
        if let Some(ref proto) = input.protocol_type {
            separated.push("protocol_type = ").push_bind_unseparated(proto);
        }
        if let Some(ref driver) = input.driver_name {
            separated.push("driver_name = ").push_bind_unseparated(driver);
        }
        builder.push(" WHERE id = ");
        builder.push_bind(id);
        builder.push(" AND workspace_id = ");
        builder.push_bind(workspace_id);
        builder.build().execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(super::types::UpdateGuardedOutcome::Updated(self.get_by_id(id).await?))
    }

    /// Breadcrumbs for MANY things in ONE recursive-CTE query
    /// (eng-review T11 — was one recursive CTE per row, up to 200
    /// round-trips per list page). Keys of the returned map are thing IDs.
    pub async fn get_breadcrumbs(
        &self,
        ids: &[String],
        max_depth: u32,
    ) -> Result<std::collections::HashMap<String, Vec<super::types::BreadcrumbNode>>, sqlx::Error>
    {
        if ids.is_empty() {
            return Ok(Default::default());
        }
        let mut qb = QueryBuilder::new(
            "WITH RECURSIVE ancestors AS (              SELECT id, name, thing_type, parent_id, id AS root, 0 AS depth FROM devices WHERE id IN (",
        );
        let mut sep = qb.separated(",");
        for id in ids {
            sep.push_bind(id);
        }
        sep.push_unseparated(
            ") UNION ALL              SELECT d.id, d.name, d.thing_type, d.parent_id, a.root, a.depth + 1              FROM devices d JOIN ancestors a ON d.id = a.parent_id              WHERE a.depth < ",
        );
        qb.push_bind(max_depth.min(10) as i32);
        qb.push(") SELECT root, id, name, thing_type FROM ancestors ORDER BY root, depth DESC");

        let rows =
            qb.build_query_as::<(String, String, String, String)>().fetch_all(&self.pool).await?;
        let mut map: std::collections::HashMap<String, Vec<super::types::BreadcrumbNode>> =
            std::collections::HashMap::new();
        for (root, id, name, thing_type) in rows {
            map.entry(root).or_default().push(super::types::BreadcrumbNode {
                id,
                name,
                thing_type,
            });
        }
        Ok(map)
    }

    /// Mark subtree summary_status='dirty' for all descendants of root_id.
    pub async fn mark_subtree_dirty(&self, root_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "WITH RECURSIVE subtree AS ( \
             SELECT id FROM devices WHERE id = ? \
             UNION ALL \
             SELECT d.id FROM devices d JOIN subtree s ON d.parent_id = s.id \
             ) \
             UPDATE devices SET summary_status = 'dirty' WHERE id IN (SELECT id FROM subtree)",
        )
        .bind(root_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    // ──────────────────────────────────────────
    // Resources
    // ──────────────────────────────────────────

    /// Detach resource from thing (set device_id = NULL).
    pub async fn detach_resource(
        &self,
        thing_id: &str,
        resource_id: &str,
        workspace_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE resources SET device_id = NULL WHERE id = ? AND device_id = ? AND workspace_id = ?",
        )
        .bind(resource_id)
        .bind(thing_id)
        .bind(workspace_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn attach_resource(
        &self,
        thing_id: &str,
        resource_id: &str,
        workspace_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("UPDATE resources SET device_id = ? WHERE id = ? AND workspace_id = ?")
                .bind(thing_id)
                .bind(resource_id)
                .bind(workspace_id)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected())
    }

    pub async fn list_unassigned_resources(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<ThingResource>, sqlx::Error> {
        sqlx::query_as::<_, ThingResource>(
            "SELECT * FROM resources WHERE workspace_id = ? AND device_id IS NULL",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
    }
}

// ──────────────────────────────────────────────
// Internal query rows
// ──────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct TreeNodeRow {
    id: String,
    name: String,
    thing_type: String,
    parent_id: Option<String>,
    #[allow(dead_code)]
    depth: i32,
}

#[derive(Debug, sqlx::FromRow)]
struct BreadcrumbRow {
    id: String,
    name: String,
    thing_type: String,
}

// ──────────────────────────────────────────────
// Tags / knowledge docs / events (moved from service — eng-review T9)
// ──────────────────────────────────────────────

impl ThingRepo {
    /// Batch-load tags for multiple thing IDs from tag_bindings.
    pub async fn load_tags_batch(
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
        let mut map: std::collections::HashMap<String, Vec<super::types::TagInfo>> =
            std::collections::HashMap::new();
        for (target_id, id, name, color) in rows {
            map.entry(target_id).or_default().push(super::types::TagInfo { id, name, color });
        }
        Ok(map)
    }

    /// Knowledge docs attached to a thing, newest first.
    pub async fn list_knowledge_docs(
        &self,
        device_id: &str,
        limit: i64,
    ) -> Result<Vec<super::types::DocRow>, sqlx::Error> {
        sqlx::query_as::<_, super::types::DocRow>(
            "SELECT id, name, resource_type, description, file_path, content, tags, created_at, updated_at \
                 FROM resources WHERE device_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(device_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    /// Recent events for a thing, newest first.
    pub async fn list_recent_events(
        &self,
        device_id: &str,
        limit: i64,
    ) -> Result<Vec<super::types::EventRow>, sqlx::Error> {
        sqlx::query_as::<_, super::types::EventRow>(
            "SELECT id, event_type, event_subtype, event_level, source_type, source_id, \
                 title, content, metadata, created_at \
                 FROM events WHERE device_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(device_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }
}
