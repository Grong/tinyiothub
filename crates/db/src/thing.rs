//! Thing 持久化：devices 表的 Thing 视图 + resources/tag_bindings/events 侧查询
//!（自 cloud domains/thing/repo.rs 迁入，Task 12）。
//!
//! 类型随 repo 住 db：ThingRow/ThingResource/TagInfo 等行类型，
//! cloud 侧 types 模块直接引用本模块路径。

use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool, Transaction};

use crate::database::Db;

// ──────────────────────────────────────────────
// 持久化类型（DB 行 + 查询参数）— 自 cloud thing/types.rs 迁入
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagInfo {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Maps to the `devices` table after the Thing Ontology migration.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ThingRow {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub thing_type: String,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub description: Option<String>,
    pub position: Option<String>,
    pub driver_name: Option<String>,
    pub device_model: Option<String>,
    pub protocol_type: Option<String>,
    pub factory_name: Option<String>,
    pub linked_data: Option<String>,
    pub driver_options: Option<String>,
    pub state: i32,
    pub parent_id: Option<String>,
    pub organization_id: Option<String>,
    pub tenant_id: Option<String>,
    pub workspace_id: Option<String>,
    pub linked_gateway: Option<String>,
    pub fingerprint: Option<String>,
    pub template_id: Option<String>,
    pub ontology_summary: Option<String>,
    pub summary_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListThingsParams {
    pub thing_type: Option<String>,
    pub parent_id: Option<String>,
    pub tags: Option<String>,
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl ListThingsParams {
    /// Clamp `limit` to 1..=200, default 50.
    pub fn limit(&self) -> u32 {
        let raw = self.limit.unwrap_or(50);
        raw.clamp(1, 200)
    }

    /// Default offset to 0.
    pub fn offset(&self) -> u32 {
        self.offset.unwrap_or(0)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BreadcrumbNode {
    pub id: String,
    pub name: String,
    pub thing_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingTreeNode {
    pub id: String,
    pub name: String,
    pub thing_type: String,
    pub children: Vec<ThingTreeNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateThingRequest {
    pub name: Option<String>,
    pub thing_type: Option<String>,
    pub device_type: Option<String>,
    pub description: Option<String>,
    pub parent_id: Option<String>,
    pub template_id: Option<String>,
    pub protocol_type: Option<String>,
    pub driver_name: Option<String>,
    pub tags: Option<String>,
}

/// Maps to the `resources` table (formerly `thing_resources`).
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingResource {
    pub id: String,
    pub workspace_id: String,
    pub device_id: Option<String>,
    #[sqlx(rename = "type")]
    pub resource_type: String,
    pub name: String,
    pub file_path: String,
    pub content: Option<String>,
    pub tags: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Recent event row for the thing profile (real events-table columns).
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct EventRow {
    pub id: String,
    pub event_type: String,
    pub event_subtype: Option<String>,
    pub event_level: i64,
    pub source_type: String,
    pub source_id: Option<String>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub metadata: Option<String>,
    pub created_at: String,
}

/// Knowledge document row attached to a thing.
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DocRow {
    pub id: String,
    pub name: String,
    pub resource_type: String,
    pub description: Option<String>,
    pub file_path: String,
    pub content: Option<String>,
    pub tags: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Outcome of a transaction-guarded update (cycle check + write in one tx).
pub enum UpdateGuardedOutcome {
    Cycle,
    Updated(Option<Box<ThingRow>>),
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

/// Single-query cycle check: walk UP from the candidate parent; if the
/// thing itself is on that chain, reparenting creates a cycle. Depth cap 10
/// matches the read-side breadcrumb/tree caps.
const CHECK_CYCLE_SQL: &str = "WITH RECURSIVE up AS ( \
    SELECT id, parent_id, 0 AS depth FROM devices WHERE id = ? \
    UNION ALL \
    SELECT d.id, d.parent_id, up.depth + 1 FROM devices d JOIN up ON d.id = up.parent_id \
    WHERE up.depth < 10 \
) SELECT EXISTS(SELECT 1 FROM up WHERE id = ?)";

// ──────────────────────────────────────────────
// 持久化函数（SQLite）
// ──────────────────────────────────────────────

/// Workspace-scoped lookup (eng-review T1): strict workspace match,
/// same semantics as list().
pub(crate) async fn find_thing_by_id_scoped(
    pool: &SqlitePool,
    id: &str,
    workspace_id: &str,
) -> Result<Option<ThingRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingRow>("SELECT * FROM devices WHERE id = ? AND workspace_id = ?")
        .bind(id)
        .bind(workspace_id)
        .fetch_optional(pool)
        .await
}

/// Workspace-scoped delete (eng-review T1): refuses to delete another
/// workspace's thing.
pub(crate) async fn delete_thing_scoped(pool: &SqlitePool, id: &str, workspace_id: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM devices WHERE id = ? AND workspace_id = ?")
        .bind(id)
        .bind(workspace_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Workspace-scoped name lookup.
pub(crate) async fn find_thing_by_name(
    pool: &SqlitePool,
    workspace_id: &str,
    name: &str,
) -> Result<Option<ThingRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingRow>("SELECT * FROM devices WHERE workspace_id = ? AND name = ?")
        .bind(workspace_id)
        .bind(name)
        .fetch_optional(pool)
        .await
}

/// List things with dynamic WHERE + pagination.
pub(crate) async fn list_things(
    pool: &SqlitePool,
    workspace_id: &str,
    params: &ListThingsParams,
) -> Result<(Vec<ThingRow>, u64), sqlx::Error> {
    let limit = params.limit() as i64;
    let offset = params.offset() as i64;

    // Build COUNT query
    let mut count_builder = QueryBuilder::new("SELECT COUNT(*) FROM devices WHERE workspace_id = ");
    count_builder.push_bind(workspace_id);
    push_where_clauses(&mut count_builder, params);

    let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

    // Build SELECT query
    let mut select_builder = QueryBuilder::new("SELECT * FROM devices WHERE workspace_id = ");
    select_builder.push_bind(workspace_id);
    push_where_clauses(&mut select_builder, params);
    select_builder.push(" ORDER BY created_at DESC LIMIT ");
    select_builder.push_bind(limit);
    select_builder.push(" OFFSET ");
    select_builder.push_bind(offset);

    let rows = select_builder.build_query_as::<ThingRow>().fetch_all(pool).await?;

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
pub(crate) async fn find_thing_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ThingRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingRow>("SELECT * FROM devices WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// INSERT — returns the newly created row.
pub(crate) async fn create_thing(pool: &SqlitePool, row: &ThingRow) -> Result<ThingRow, sqlx::Error> {
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
    .execute(pool)
    .await?;

    find_thing_by_id(pool, &row.id)
        .await?
        .ok_or_else(|| sqlx::Error::Protocol("readback after insert failed".into()))
}

/// UPDATE — returns the updated row.
pub(crate) async fn update_thing(
    pool: &SqlitePool,
    id: &str,
    input: &UpdateThingRequest,
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

    builder.build().execute(pool).await?;

    find_thing_by_id(pool, id).await
}

/// DELETE — checks children count first.
/// Returns rows_affected on success.
pub(crate) async fn delete_thing(pool: &SqlitePool, id: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// Count children of a thing.
pub(crate) async fn count_thing_children(pool: &SqlitePool, id: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE parent_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
}

/// Recursive CTE: all descendants of `root_id` (or full workspace tree if None).
pub(crate) async fn get_thing_tree(
    pool: &SqlitePool,
    root_id: Option<&str>,
    workspace_id: &str,
    max_depth: u32,
) -> Result<Vec<ThingTreeNode>, sqlx::Error> {
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

    let rows = builder.build_query_as::<TreeNodeRow>().fetch_all(pool).await?;

    Ok(build_tree(rows))
}

fn build_tree(rows: Vec<TreeNodeRow>) -> Vec<ThingTreeNode> {
    let mut nodes: std::collections::HashMap<String, ThingTreeNode> = std::collections::HashMap::new();

    for row in &rows {
        nodes.insert(
            row.id.clone(),
            ThingTreeNode {
                id: row.id.clone(),
                name: row.name.clone(),
                thing_type: row.thing_type.clone(),
                children: vec![],
            },
        );
    }

    let mut roots: Vec<ThingTreeNode> = vec![];
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

    fn sort_tree(nodes: &mut [ThingTreeNode]) {
        nodes.sort_by(|a, b| a.name.cmp(&b.name));
        for n in nodes.iter_mut() {
            sort_tree(&mut n.children);
        }
    }
    sort_tree(&mut roots);

    roots
}

/// Breadcrumb: walk parent chain up from `id`, max depth 10.
pub(crate) async fn get_thing_breadcrumb(
    pool: &SqlitePool,
    id: &str,
    max_depth: u32,
) -> Result<Vec<BreadcrumbNode>, sqlx::Error> {
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

    let rows: Vec<BreadcrumbRow> = builder.build_query_as::<BreadcrumbRow>().fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|r| BreadcrumbNode {
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
pub(crate) async fn check_thing_cycle(
    pool: &SqlitePool,
    thing_id: &str,
    candidate_parent_id: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(CHECK_CYCLE_SQL)
        .bind(candidate_parent_id)
        .bind(thing_id)
        .fetch_one(pool)
        .await
}

/// Guarded-update 事务体（eng-review T11）：cycle check + UPDATE 在同一事务内。
/// 返回 `true` 表示检测到环（未执行任何写入，由调用方丢弃事务）；
/// 返回 `false` 表示 UPDATE 已执行，调用方负责 commit 与 readback。
pub(crate) async fn update_thing_guarded_tx(
    tx: &mut Transaction<'static, Sqlite>,
    id: &str,
    input: &UpdateThingRequest,
    workspace_id: &str,
) -> Result<bool, sqlx::Error> {
    if let Some(ref new_parent_id) = input.parent_id {
        let is_cycle: bool = sqlx::query_scalar(CHECK_CYCLE_SQL)
            .bind(new_parent_id)
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
        if is_cycle {
            return Ok(true);
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
    builder.build().execute(&mut **tx).await?;

    Ok(false)
}

/// Breadcrumbs for MANY things in ONE recursive-CTE query
/// (eng-review T11 — was one recursive CTE per row, up to 200
/// round-trips per list page). Keys of the returned map are thing IDs.
pub(crate) async fn get_thing_breadcrumbs(
    pool: &SqlitePool,
    ids: &[String],
    max_depth: u32,
) -> Result<std::collections::HashMap<String, Vec<BreadcrumbNode>>, sqlx::Error> {
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

    let rows = qb
        .build_query_as::<(String, String, String, String)>()
        .fetch_all(pool)
        .await?;
    let mut map: std::collections::HashMap<String, Vec<BreadcrumbNode>> = std::collections::HashMap::new();
    for (root, id, name, thing_type) in rows {
        map.entry(root)
            .or_default()
            .push(BreadcrumbNode { id, name, thing_type });
    }
    Ok(map)
}

/// Mark subtree summary_status='dirty' for all descendants of root_id.
pub(crate) async fn mark_thing_subtree_dirty(pool: &SqlitePool, root_id: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "WITH RECURSIVE subtree AS ( \
             SELECT id FROM devices WHERE id = ? \
             UNION ALL \
             SELECT d.id FROM devices d JOIN subtree s ON d.parent_id = s.id \
             ) \
             UPDATE devices SET summary_status = 'dirty' WHERE id IN (SELECT id FROM subtree)",
    )
    .bind(root_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Detach resource from thing (set device_id = NULL).
pub(crate) async fn detach_thing_resource(
    pool: &SqlitePool,
    thing_id: &str,
    resource_id: &str,
    workspace_id: &str,
) -> Result<u64, sqlx::Error> {
    let result =
        sqlx::query("UPDATE resources SET device_id = NULL WHERE id = ? AND device_id = ? AND workspace_id = ?")
            .bind(resource_id)
            .bind(thing_id)
            .bind(workspace_id)
            .execute(pool)
            .await?;
    Ok(result.rows_affected())
}

/// Attach resource to thing.
pub(crate) async fn attach_thing_resource(
    pool: &SqlitePool,
    thing_id: &str,
    resource_id: &str,
    workspace_id: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("UPDATE resources SET device_id = ? WHERE id = ? AND workspace_id = ?")
        .bind(thing_id)
        .bind(resource_id)
        .bind(workspace_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// List resources not yet attached to any thing.
pub(crate) async fn list_unassigned_thing_resources(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<ThingResource>, sqlx::Error> {
    sqlx::query_as::<_, ThingResource>("SELECT * FROM resources WHERE workspace_id = ? AND device_id IS NULL")
        .bind(workspace_id)
        .fetch_all(pool)
        .await
}

/// Batch-load tags for multiple thing IDs from tag_bindings.
pub(crate) async fn load_thing_tags_batch(
    pool: &SqlitePool,
    thing_ids: &[&str],
) -> Result<std::collections::HashMap<String, Vec<TagInfo>>, sqlx::Error> {
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
        .fetch_all(pool)
        .await?;
    let mut map: std::collections::HashMap<String, Vec<TagInfo>> = std::collections::HashMap::new();
    for (target_id, id, name, color) in rows {
        map.entry(target_id).or_default().push(TagInfo { id, name, color });
    }
    Ok(map)
}

/// Knowledge docs attached to a thing, newest first.
pub(crate) async fn list_thing_knowledge_docs(
    pool: &SqlitePool,
    device_id: &str,
    limit: i64,
) -> Result<Vec<DocRow>, sqlx::Error> {
    sqlx::query_as::<_, DocRow>(
        "SELECT id, name, resource_type, description, file_path, content, tags, created_at, updated_at \
                 FROM resources WHERE device_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(device_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Recent events for a thing, newest first.
pub(crate) async fn list_thing_recent_events(
    pool: &SqlitePool,
    device_id: &str,
    limit: i64,
) -> Result<Vec<EventRow>, sqlx::Error> {
    sqlx::query_as::<_, EventRow>(
        "SELECT id, event_type, event_subtype, event_level, source_type, source_id, \
                 title, content, metadata, created_at \
                 FROM events WHERE device_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(device_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

// ──────────────────────────────────────────────
// Summary 侧查询（自 cloud thing/summary.rs 迁入）
// ──────────────────────────────────────────────

/// Mark a thing's summary dirty (resource attach/detach/update trigger).
pub(crate) async fn mark_thing_summary_dirty(pool: &SqlitePool, thing_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE devices SET summary_status = 'dirty' WHERE id = ?")
        .bind(thing_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Read (ontology_summary, summary_status) for a thing.
pub(crate) async fn get_thing_summary_state(
    pool: &SqlitePool,
    thing_id: &str,
) -> Result<Option<(Option<String>, Option<String>)>, sqlx::Error> {
    sqlx::query_as("SELECT ontology_summary, summary_status FROM devices WHERE id = ?")
        .bind(thing_id)
        .fetch_optional(pool)
        .await
}

/// Read the cached ontology summary for a thing.
pub(crate) async fn get_thing_summary(pool: &SqlitePool, thing_id: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(Option<String>,)> = sqlx::query_as("SELECT ontology_summary FROM devices WHERE id = ?")
        .bind(thing_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|(s,)| s))
}

/// Persist a computed summary and mark status 'ok'.
pub(crate) async fn save_thing_summary(pool: &SqlitePool, thing_id: &str, text: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE devices SET ontology_summary = ?, summary_status = 'ok', \
                     updated_at = datetime('now') WHERE id = ?",
    )
    .bind(text)
    .bind(thing_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark summary status 'failed' (keep cached summary).
pub(crate) async fn mark_thing_summary_failed(pool: &SqlitePool, thing_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE devices SET summary_status = 'failed', \
                     updated_at = datetime('now') WHERE id = ?",
    )
    .bind(thing_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch (name, thing_type) for a thing.
pub(crate) async fn find_thing_name_and_type(
    pool: &SqlitePool,
    thing_id: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as("SELECT name, thing_type FROM devices WHERE id = ?")
        .bind(thing_id)
        .fetch_optional(pool)
        .await
}

/// Breadcrumb names from root to this thing (recursive CTE, depth cap 10).
pub(crate) async fn get_thing_breadcrumb_names(pool: &SqlitePool, thing_id: &str) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "WITH RECURSIVE ancestors AS (
            SELECT id, name, parent_id, 0 AS depth FROM devices WHERE id = ?
            UNION ALL
            SELECT d.id, d.name, d.parent_id, a.depth + 1
            FROM devices d JOIN ancestors a ON d.id = a.parent_id
            WHERE a.depth < 10
        ) SELECT name FROM ancestors ORDER BY depth DESC",
    )
    .bind(thing_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(name,)| name).collect())
}

/// Property names of a thing (model definition input).
pub(crate) async fn list_thing_property_names(pool: &SqlitePool, thing_id: &str) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT name FROM thing_properties WHERE device_id = ? ORDER BY name")
        .bind(thing_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

/// Action names of a thing (model definition input).
pub(crate) async fn list_thing_action_names(pool: &SqlitePool, thing_id: &str) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT name FROM thing_actions WHERE device_id = ? ORDER BY name")
        .bind(thing_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

/// Knowledge doc (name, content) snippets for a thing, newest first, max 5.
pub(crate) async fn list_thing_knowledge_doc_snippets(
    pool: &SqlitePool,
    thing_id: &str,
) -> Result<Vec<(String, Option<String>)>, sqlx::Error> {
    sqlx::query_as("SELECT name, content FROM resources WHERE device_id = ? ORDER BY created_at DESC LIMIT 5")
        .bind(thing_id)
        .fetch_all(pool)
        .await
}

// ──────────────────────────────────────────────
// thing_actions / thing_properties / resources 单行查询
//（自 cloud thing/handler/actions.rs、agent tools/thing 迁入）
// ──────────────────────────────────────────────

/// Thing 属性定义行（read_property 工具用）。
#[derive(Debug, sqlx::FromRow)]
pub struct ThingPropertyRow {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub data_type: Option<String>,
    pub unit: Option<String>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub default_value: Option<String>,
    pub is_read_only: bool,
}

/// Thing 文档完整行（read_document 工具用）。
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct ThingDocumentRow {
    pub id: String,
    pub name: String,
    #[sqlx(rename = "type")]
    pub doc_type: String,
    pub file_path: String,
    pub content: Option<String>,
    pub tags: String,
    pub device_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Count thing_actions matching device + name.
pub(crate) async fn count_thing_action_by_name(
    pool: &SqlitePool,
    device_id: &str,
    name: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM thing_actions WHERE device_id = ? AND name = ?")
        .bind(device_id)
        .bind(name)
        .fetch_one(pool)
        .await
}

/// Action 参数 schema（无行或 NULL 均为 None）。
pub(crate) async fn find_thing_action_parameters(
    pool: &SqlitePool,
    device_id: &str,
    name: &str,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<Option<String>> =
        sqlx::query_scalar("SELECT parameters FROM thing_actions WHERE device_id = ? AND name = ?")
            .bind(device_id)
            .bind(name)
            .fetch_optional(pool)
            .await?;
    Ok(row.flatten())
}

/// 查询属性定义行。
pub(crate) async fn find_thing_property(
    pool: &SqlitePool,
    device_id: &str,
    name: &str,
) -> Result<Option<ThingPropertyRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingPropertyRow>(
        "SELECT name, display_name, description, data_type, unit, \
             min_value, max_value, default_value, is_read_only \
             FROM thing_properties WHERE device_id = ? AND name = ?",
    )
    .bind(device_id)
    .bind(name)
    .fetch_optional(pool)
    .await
}

/// 查询文档完整行（workspace 作用域）。
pub(crate) async fn find_thing_document(
    pool: &SqlitePool,
    resource_id: &str,
    workspace_id: &str,
) -> Result<Option<ThingDocumentRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingDocumentRow>(
        "SELECT id, name, resource_type AS type, file_path, content, tags, device_id, \
             created_at, updated_at FROM resources WHERE id = ? AND workspace_id = ?",
    )
    .bind(resource_id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
}

/// Thing 知识文档搜索行（search_knowledge 工具用，自 cloud agent tools/thing 迁入）。
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct ThingKnowledgeDocRow {
    pub id: String,
    pub name: String,
    #[sqlx(rename = "type")]
    pub doc_type: String,
    pub file_path: String,
    pub tags: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 知识文档搜索（workspace 作用域；name/tags LIKE，可选 thing/tags 过滤）。
/// SQL 与原 search_knowledge 工具内联 QueryBuilder 逐字一致。
pub(crate) async fn search_thing_knowledge_docs(
    pool: &SqlitePool,
    workspace_id: &str,
    thing_id: Option<&str>,
    q: &str,
    tags: Option<&str>,
    limit: i64,
) -> Result<Vec<ThingKnowledgeDocRow>, sqlx::Error> {
    let like_pattern = format!("%{}%", q);

    // Build dynamic query with QueryBuilder
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT id, name, resource_type AS type, file_path, tags, created_at, updated_at \
         FROM resources WHERE workspace_id = ",
    );
    builder.push_bind(workspace_id);

    if let Some(tid) = thing_id {
        builder.push(" AND device_id = ");
        builder.push_bind(tid);
    }

    // LIKE search on name and tags (FTS5 deferred per TODOS)
    builder.push(" AND (name LIKE ");
    builder.push_bind(&like_pattern);
    builder.push(" OR tags LIKE ");
    builder.push_bind(&like_pattern);
    builder.push(")");

    if let Some(t) = tags {
        let tag_pattern = format!("%{}%", t);
        builder.push(" AND tags LIKE ");
        builder.push_bind(tag_pattern);
    }

    builder.push(" ORDER BY created_at DESC LIMIT ");
    builder.push_bind(limit);

    builder.build_query_as::<ThingKnowledgeDocRow>().fetch_all(pool).await
}

// ──────────────────────────────────────────────
// Db 委托
// ──────────────────────────────────────────────

impl Db {
    /// Workspace-scoped thing 查询（严格 workspace 匹配）。
    pub async fn find_thing_by_id_scoped(&self, id: &str, workspace_id: &str) -> Result<Option<ThingRow>, sqlx::Error> {
        find_thing_by_id_scoped(self.pool(), id, workspace_id).await
    }

    /// Workspace-scoped thing 删除。
    pub async fn delete_thing_scoped(&self, id: &str, workspace_id: &str) -> Result<u64, sqlx::Error> {
        delete_thing_scoped(self.pool(), id, workspace_id).await
    }

    /// Workspace-scoped 按名查询。
    pub async fn find_thing_by_name(&self, workspace_id: &str, name: &str) -> Result<Option<ThingRow>, sqlx::Error> {
        find_thing_by_name(self.pool(), workspace_id, name).await
    }

    /// 分页列出 things（动态 WHERE）。
    pub async fn list_things(
        &self,
        workspace_id: &str,
        params: &ListThingsParams,
    ) -> Result<(Vec<ThingRow>, u64), sqlx::Error> {
        list_things(self.pool(), workspace_id, params).await
    }

    /// 按 id 查单个 thing。
    pub async fn find_thing_by_id(&self, id: &str) -> Result<Option<ThingRow>, sqlx::Error> {
        find_thing_by_id(self.pool(), id).await
    }

    /// 插入 thing 并返回新行。
    pub async fn create_thing(&self, row: &ThingRow) -> Result<ThingRow, sqlx::Error> {
        create_thing(self.pool(), row).await
    }

    /// 更新 thing 并返回更新后的行。
    pub async fn update_thing(&self, id: &str, input: &UpdateThingRequest) -> Result<Option<ThingRow>, sqlx::Error> {
        update_thing(self.pool(), id, input).await
    }

    /// 删除 thing，返回受影响行数。
    pub async fn delete_thing(&self, id: &str) -> Result<u64, sqlx::Error> {
        delete_thing(self.pool(), id).await
    }

    /// 统计 thing 的子节点数。
    pub async fn count_thing_children(&self, id: &str) -> Result<i64, sqlx::Error> {
        count_thing_children(self.pool(), id).await
    }

    /// 递归 CTE 查询 thing 树。
    pub async fn get_thing_tree(
        &self,
        root_id: Option<&str>,
        workspace_id: &str,
        max_depth: u32,
    ) -> Result<Vec<ThingTreeNode>, sqlx::Error> {
        get_thing_tree(self.pool(), root_id, workspace_id, max_depth).await
    }

    /// 查询 thing 的面包屑（向上父链，最大深度 10）。
    pub async fn get_thing_breadcrumb(&self, id: &str, max_depth: u32) -> Result<Vec<BreadcrumbNode>, sqlx::Error> {
        get_thing_breadcrumb(self.pool(), id, max_depth).await
    }

    /// 环检测：candidate_parent 的祖先链上是否含 thing。
    pub async fn check_thing_cycle(&self, thing_id: &str, candidate_parent_id: &str) -> Result<bool, sqlx::Error> {
        check_thing_cycle(self.pool(), thing_id, candidate_parent_id).await
    }

    /// 事务内 cycle check + UPDATE（TOCTOU 安全，eng-review T11）。
    /// 唯一允许 Db 方法内起事务的形态：事务体在 `update_thing_guarded_tx`。
    pub async fn update_thing_guarded(
        &self,
        id: &str,
        input: &UpdateThingRequest,
        workspace_id: &str,
    ) -> Result<UpdateGuardedOutcome, sqlx::Error> {
        let mut tx = self.pool().begin().await?;
        if update_thing_guarded_tx(&mut tx, id, input, workspace_id).await? {
            // 检测到环：未执行写入，丢弃事务（rollback）。
            return Ok(UpdateGuardedOutcome::Cycle);
        }
        tx.commit().await?;
        Ok(UpdateGuardedOutcome::Updated(
            find_thing_by_id(self.pool(), id).await?.map(Box::new),
        ))
    }

    /// 单查询批量面包屑（key 为 thing ID）。
    pub async fn get_thing_breadcrumbs(
        &self,
        ids: &[String],
        max_depth: u32,
    ) -> Result<std::collections::HashMap<String, Vec<BreadcrumbNode>>, sqlx::Error> {
        get_thing_breadcrumbs(self.pool(), ids, max_depth).await
    }

    /// 将子树全部标记 summary_status='dirty'。
    pub async fn mark_thing_subtree_dirty(&self, root_id: &str) -> Result<u64, sqlx::Error> {
        mark_thing_subtree_dirty(self.pool(), root_id).await
    }

    /// 解除 resource 与 thing 的挂载（device_id = NULL）。
    pub async fn detach_thing_resource(
        &self,
        thing_id: &str,
        resource_id: &str,
        workspace_id: &str,
    ) -> Result<u64, sqlx::Error> {
        detach_thing_resource(self.pool(), thing_id, resource_id, workspace_id).await
    }

    /// 挂载 resource 到 thing。
    pub async fn attach_thing_resource(
        &self,
        thing_id: &str,
        resource_id: &str,
        workspace_id: &str,
    ) -> Result<u64, sqlx::Error> {
        attach_thing_resource(self.pool(), thing_id, resource_id, workspace_id).await
    }

    /// 列出未挂载的 resources。
    pub async fn list_unassigned_thing_resources(&self, workspace_id: &str) -> Result<Vec<ThingResource>, sqlx::Error> {
        list_unassigned_thing_resources(self.pool(), workspace_id).await
    }

    /// 批量加载 thing 标签（tag_bindings）。
    pub async fn load_thing_tags_batch(
        &self,
        thing_ids: &[&str],
    ) -> Result<std::collections::HashMap<String, Vec<TagInfo>>, sqlx::Error> {
        load_thing_tags_batch(self.pool(), thing_ids).await
    }

    /// Thing 挂载的知识文档（新的在前）。
    pub async fn list_thing_knowledge_docs(&self, device_id: &str, limit: i64) -> Result<Vec<DocRow>, sqlx::Error> {
        list_thing_knowledge_docs(self.pool(), device_id, limit).await
    }

    /// Thing 的最近事件（新的在前）。
    pub async fn list_thing_recent_events(&self, device_id: &str, limit: i64) -> Result<Vec<EventRow>, sqlx::Error> {
        list_thing_recent_events(self.pool(), device_id, limit).await
    }

    /// 标记 thing 摘要 dirty（resource 变更触发）。
    pub async fn mark_thing_summary_dirty(&self, thing_id: &str) -> Result<(), sqlx::Error> {
        mark_thing_summary_dirty(self.pool(), thing_id).await
    }

    /// 读取 thing 的 (ontology_summary, summary_status)。
    pub async fn get_thing_summary_state(
        &self,
        thing_id: &str,
    ) -> Result<Option<(Option<String>, Option<String>)>, sqlx::Error> {
        get_thing_summary_state(self.pool(), thing_id).await
    }

    /// 读取 thing 的缓存摘要。
    pub async fn get_thing_summary(&self, thing_id: &str) -> Result<Option<String>, sqlx::Error> {
        get_thing_summary(self.pool(), thing_id).await
    }

    /// 持久化摘要并标记 'ok'。
    pub async fn save_thing_summary(&self, thing_id: &str, text: &str) -> Result<(), sqlx::Error> {
        save_thing_summary(self.pool(), thing_id, text).await
    }

    /// 标记摘要 'failed'（保留缓存）。
    pub async fn mark_thing_summary_failed(&self, thing_id: &str) -> Result<(), sqlx::Error> {
        mark_thing_summary_failed(self.pool(), thing_id).await
    }

    /// 读取 thing 的 (name, thing_type)。
    pub async fn find_thing_name_and_type(&self, thing_id: &str) -> Result<Option<(String, String)>, sqlx::Error> {
        find_thing_name_and_type(self.pool(), thing_id).await
    }

    /// 面包屑名称链（根到本节点）。
    pub async fn get_thing_breadcrumb_names(&self, thing_id: &str) -> Result<Vec<String>, sqlx::Error> {
        get_thing_breadcrumb_names(self.pool(), thing_id).await
    }

    /// Thing 的属性名列表。
    pub async fn list_thing_property_names(&self, thing_id: &str) -> Result<Vec<String>, sqlx::Error> {
        list_thing_property_names(self.pool(), thing_id).await
    }

    /// Thing 的动作名列表。
    pub async fn list_thing_action_names(&self, thing_id: &str) -> Result<Vec<String>, sqlx::Error> {
        list_thing_action_names(self.pool(), thing_id).await
    }

    /// Thing 的知识文档摘要（新的在前，最多 5 条）。
    pub async fn list_thing_knowledge_doc_snippets(
        &self,
        thing_id: &str,
    ) -> Result<Vec<(String, Option<String>)>, sqlx::Error> {
        list_thing_knowledge_doc_snippets(self.pool(), thing_id).await
    }

    /// 统计 thing_actions 中 device + name 匹配数。
    pub async fn count_thing_action_by_name(&self, device_id: &str, name: &str) -> Result<i64, sqlx::Error> {
        count_thing_action_by_name(self.pool(), device_id, name).await
    }

    /// Action 参数 schema（无行或 NULL 均为 None）。
    pub async fn find_thing_action_parameters(
        &self,
        device_id: &str,
        name: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        find_thing_action_parameters(self.pool(), device_id, name).await
    }

    /// 查询 thing 属性定义行。
    pub async fn find_thing_property(
        &self,
        device_id: &str,
        name: &str,
    ) -> Result<Option<ThingPropertyRow>, sqlx::Error> {
        find_thing_property(self.pool(), device_id, name).await
    }

    /// 查询 thing 文档完整行（workspace 作用域）。
    pub async fn find_thing_document(
        &self,
        resource_id: &str,
        workspace_id: &str,
    ) -> Result<Option<ThingDocumentRow>, sqlx::Error> {
        find_thing_document(self.pool(), resource_id, workspace_id).await
    }

    /// 知识文档搜索（search_knowledge 工具用，workspace 作用域）。
    pub async fn search_thing_knowledge_docs(
        &self,
        workspace_id: &str,
        thing_id: Option<&str>,
        q: &str,
        tags: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ThingKnowledgeDocRow>, sqlx::Error> {
        search_thing_knowledge_docs(self.pool(), workspace_id, thing_id, q, tags, limit).await
    }
}

// ──────────────────────────────────────────────
// Open API 投影查询（自 cloud admin/open 迁入，Task 12）
// ──────────────────────────────────────────────

/// Open API thing 属性行。
#[derive(Debug)]
pub struct OpenThingPropertyRow {
    pub name: String,
    pub display_name: Option<String>,
    pub data_type: String,
    pub value: Option<String>,
    pub unit: Option<String>,
    pub updated_at: String,
}

/// Open API thing 命令行。
#[derive(Debug)]
pub struct OpenThingCommandRow {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub parameters: Option<String>,
}

/// Open API：列出 thing 属性（workspace 作用域子查询）。
pub(crate) async fn list_open_thing_properties(
    pool: &SqlitePool,
    device_id: &str,
    workspace_id: &str,
) -> Result<Vec<OpenThingPropertyRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT name, display_name, data_type, value, unit, updated_at FROM thing_properties          WHERE device_id = ? AND device_id IN (SELECT id FROM devices WHERE workspace_id = ?)          ORDER BY created_at DESC",
    )
    .bind(device_id)
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| OpenThingPropertyRow {
            name: row.try_get::<String, _>("name").unwrap_or_default(),
            display_name: row.try_get::<Option<String>, _>("display_name").unwrap_or_default(),
            data_type: row.try_get::<String, _>("data_type").unwrap_or_default(),
            value: row.try_get::<Option<String>, _>("value").unwrap_or_default(),
            unit: row.try_get::<Option<String>, _>("unit").unwrap_or_default(),
            updated_at: row.try_get::<String, _>("updated_at").unwrap_or_default(),
        })
        .collect())
}

/// Open API：列出 thing 命令（workspace 作用域子查询）。
pub(crate) async fn list_open_thing_commands(
    pool: &SqlitePool,
    device_id: &str,
    workspace_id: &str,
) -> Result<Vec<OpenThingCommandRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, name, display_name, description, parameters FROM thing_actions          WHERE device_id = ? AND device_id IN (SELECT id FROM devices WHERE workspace_id = ?) ORDER BY name",
    )
    .bind(device_id)
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| OpenThingCommandRow {
            id: row.try_get::<String, _>("id").unwrap_or_default(),
            name: row.try_get::<String, _>("name").unwrap_or_default(),
            display_name: row.try_get::<Option<String>, _>("display_name").unwrap_or_default(),
            description: row.try_get::<Option<String>, _>("description").unwrap_or_default(),
            parameters: row.try_get::<Option<String>, _>("parameters").unwrap_or_default(),
        })
        .collect())
}

impl Db {
    /// Open API：列出 thing 属性（workspace 作用域子查询）。
    pub async fn list_open_thing_properties(
        &self,
        device_id: &str,
        workspace_id: &str,
    ) -> Result<Vec<OpenThingPropertyRow>, sqlx::Error> {
        list_open_thing_properties(self.pool(), device_id, workspace_id).await
    }

    /// Open API：列出 thing 命令（workspace 作用域子查询）。
    pub async fn list_open_thing_commands(
        &self,
        device_id: &str,
        workspace_id: &str,
    ) -> Result<Vec<OpenThingCommandRow>, sqlx::Error> {
        list_open_thing_commands(self.pool(), device_id, workspace_id).await
    }
}
