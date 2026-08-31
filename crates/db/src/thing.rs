//! Thing 持久化：things 表的唯一入口模块（Task 5 收敛）。
//!
//! 两段来源：
//! - Thing 视图 + resources/tag_bindings/events 侧查询（自 cloud domains/thing/repo.rs 迁入，Task 12）；
//! - 原 device.rs 全部内容（Thing/ThingCriteria 等类型名保持不变，PR-2 再改类型名）。
//!
//! 类型随 repo 住 db：ThingRow/ThingResource/TagInfo/ThingCriteria 等行类型，
//! cloud 侧 types 模块直接引用本模块路径。

use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool, Transaction};

use crate::database::Db;
use crate::thing_row_mapper;
use tinyiothub_core::error::{Error, Result};
use tinyiothub_core::models::thing::{CreateThingRequest, Thing, ThingStats, ThingStatusUpdate, UpdateThingRequest};
use tinyiothub_core::{generate_id, now_string};

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

/// Maps to the `things` table after the Thing Ontology migration.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ThingRow {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub thing_type: String,
    pub category: Option<String>,
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
pub struct UpdateThingRowRequest {
    pub name: Option<String>,
    pub thing_type: Option<String>,
    pub category: Option<String>,
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
    pub thing_id: Option<String>,
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
    SELECT id, parent_id, 0 AS depth FROM things WHERE id = ? \
    UNION ALL \
    SELECT d.id, d.parent_id, up.depth + 1 FROM things d JOIN up ON d.id = up.parent_id \
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
) -> std::result::Result<Option<ThingRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingRow>("SELECT * FROM things WHERE id = ? AND workspace_id = ?")
        .bind(id)
        .bind(workspace_id)
        .fetch_optional(pool)
        .await
}

/// Workspace-scoped delete (eng-review T1): refuses to delete another
/// workspace's thing.
pub(crate) async fn delete_thing_scoped(
    pool: &SqlitePool,
    id: &str,
    workspace_id: &str,
) -> std::result::Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM things WHERE id = ? AND workspace_id = ?")
        .bind(id)
        .bind(workspace_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Workspace-scoped name lookup.
pub(crate) async fn find_thing_row_by_name(
    pool: &SqlitePool,
    workspace_id: &str,
    name: &str,
) -> std::result::Result<Option<ThingRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingRow>("SELECT * FROM things WHERE workspace_id = ? AND name = ?")
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
) -> std::result::Result<(Vec<ThingRow>, u64), sqlx::Error> {
    let limit = params.limit() as i64;
    let offset = params.offset() as i64;

    // Build COUNT query
    let mut count_builder = QueryBuilder::new("SELECT COUNT(*) FROM things WHERE workspace_id = ");
    count_builder.push_bind(workspace_id);
    push_where_clauses(&mut count_builder, params);

    let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

    // Build SELECT query
    let mut select_builder = QueryBuilder::new("SELECT * FROM things WHERE workspace_id = ");
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
pub(crate) async fn find_thing_row_by_id(
    pool: &SqlitePool,
    id: &str,
) -> std::result::Result<Option<ThingRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingRow>("SELECT * FROM things WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// INSERT — returns the newly created row.
pub(crate) async fn create_thing_row(pool: &SqlitePool, row: &ThingRow) -> std::result::Result<ThingRow, sqlx::Error> {
    sqlx::query(
        "INSERT INTO things (id, name, display_name, thing_type, category, \
             description, parent_id, template_id, protocol_type, driver_name, \
             workspace_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.name)
    .bind(&row.display_name)
    .bind(&row.thing_type)
    .bind(&row.category)
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

    find_thing_row_by_id(pool, &row.id)
        .await?
        .ok_or_else(|| sqlx::Error::Protocol("readback after insert failed".into()))
}

/// UPDATE — returns the updated row.
pub(crate) async fn update_thing_row(
    pool: &SqlitePool,
    id: &str,
    input: &UpdateThingRowRequest,
) -> std::result::Result<Option<ThingRow>, sqlx::Error> {
    let mut builder = QueryBuilder::new("UPDATE things SET ");
    let mut separated = builder.separated(", ");
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    separated.push("updated_at = ").push_bind_unseparated(&now);

    if let Some(ref name) = input.name {
        separated.push("name = ").push_bind_unseparated(name);
    }
    if let Some(ref tt) = input.thing_type {
        separated.push("thing_type = ").push_bind_unseparated(tt);
    }
    if let Some(ref dt) = input.category {
        separated.push("category = ").push_bind_unseparated(dt);
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

    find_thing_row_by_id(pool, id).await
}

/// DELETE — checks children count first.
/// Returns rows_affected on success.
pub(crate) async fn delete_thing_row(pool: &SqlitePool, id: &str) -> std::result::Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM things WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// Count children of a thing.
pub(crate) async fn count_thing_children(pool: &SqlitePool, id: &str) -> std::result::Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE parent_id = ?")
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
) -> std::result::Result<Vec<ThingTreeNode>, sqlx::Error> {
    let depth_val = max_depth.min(20) as i32;

    let root_prefix = "WITH RECURSIVE subtree AS ( \
            SELECT id, name, thing_type, parent_id, 0 AS depth FROM things WHERE ";

    let union_part = " UNION ALL \
            SELECT d.id, d.name, d.thing_type, d.parent_id, s.depth + 1 \
            FROM things d JOIN subtree s ON d.parent_id = s.id \
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
) -> std::result::Result<Vec<BreadcrumbNode>, sqlx::Error> {
    let depth_val = (max_depth.min(10) as i32).to_string();

    let mut builder = QueryBuilder::new(
        "WITH RECURSIVE ancestors AS ( \
             SELECT id, name, thing_type, parent_id, 0 AS depth FROM things WHERE id = ",
    );
    builder.push_bind(id);
    builder.push(
        " UNION ALL \
             SELECT d.id, d.name, d.thing_type, d.parent_id, a.depth + 1 \
             FROM things d JOIN ancestors a ON d.id = a.parent_id \
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
) -> std::result::Result<bool, sqlx::Error> {
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
    input: &UpdateThingRowRequest,
    workspace_id: &str,
) -> std::result::Result<bool, sqlx::Error> {
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

    let mut builder = QueryBuilder::new("UPDATE things SET ");
    let mut separated = builder.separated(", ");
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    separated.push("updated_at = ").push_bind_unseparated(&now);
    if let Some(ref name) = input.name {
        separated.push("name = ").push_bind_unseparated(name);
    }
    if let Some(ref tt) = input.thing_type {
        separated.push("thing_type = ").push_bind_unseparated(tt);
    }
    if let Some(ref dt) = input.category {
        separated.push("category = ").push_bind_unseparated(dt);
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
) -> std::result::Result<std::collections::HashMap<String, Vec<BreadcrumbNode>>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Default::default());
    }
    let mut qb = QueryBuilder::new(
        "WITH RECURSIVE ancestors AS (              SELECT id, name, thing_type, parent_id, id AS root, 0 AS depth FROM things WHERE id IN (",
    );
    let mut sep = qb.separated(",");
    for id in ids {
        sep.push_bind(id);
    }
    sep.push_unseparated(
        ") UNION ALL              SELECT d.id, d.name, d.thing_type, d.parent_id, a.root, a.depth + 1              FROM things d JOIN ancestors a ON d.id = a.parent_id              WHERE a.depth < ",
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
pub(crate) async fn mark_thing_subtree_dirty(
    pool: &SqlitePool,
    root_id: &str,
) -> std::result::Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "WITH RECURSIVE subtree AS ( \
             SELECT id FROM things WHERE id = ? \
             UNION ALL \
             SELECT d.id FROM things d JOIN subtree s ON d.parent_id = s.id \
             ) \
             UPDATE things SET summary_status = 'dirty' WHERE id IN (SELECT id FROM subtree)",
    )
    .bind(root_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Detach resource from thing (set thing_id = NULL).
pub(crate) async fn detach_thing_resource(
    pool: &SqlitePool,
    thing_id: &str,
    resource_id: &str,
    workspace_id: &str,
) -> std::result::Result<u64, sqlx::Error> {
    let result = sqlx::query("UPDATE resources SET thing_id = NULL WHERE id = ? AND thing_id = ? AND workspace_id = ?")
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
) -> std::result::Result<u64, sqlx::Error> {
    let result = sqlx::query("UPDATE resources SET thing_id = ? WHERE id = ? AND workspace_id = ?")
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
) -> std::result::Result<Vec<ThingResource>, sqlx::Error> {
    sqlx::query_as::<_, ThingResource>("SELECT * FROM resources WHERE workspace_id = ? AND thing_id IS NULL")
        .bind(workspace_id)
        .fetch_all(pool)
        .await
}

/// Batch-load tags for multiple thing IDs from tag_bindings.
pub(crate) async fn load_thing_tags_batch(
    pool: &SqlitePool,
    thing_ids: &[&str],
) -> std::result::Result<std::collections::HashMap<String, Vec<TagInfo>>, sqlx::Error> {
    if thing_ids.is_empty() {
        return Ok(Default::default());
    }
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT tb.target_id, t.id, t.name, t.color FROM tag_bindings tb \
             JOIN tags t ON t.id = tb.tag_id \
             WHERE tb.target_type = 'thing' AND tb.target_id IN (",
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
    thing_id: &str,
    limit: i64,
) -> std::result::Result<Vec<DocRow>, sqlx::Error> {
    sqlx::query_as::<_, DocRow>(
        "SELECT id, name, resource_type, description, file_path, content, tags, created_at, updated_at \
                 FROM resources WHERE thing_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(thing_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Recent events for a thing, newest first.
pub(crate) async fn list_thing_recent_events(
    pool: &SqlitePool,
    thing_id: &str,
    limit: i64,
) -> std::result::Result<Vec<EventRow>, sqlx::Error> {
    sqlx::query_as::<_, EventRow>(
        "SELECT id, event_type, event_subtype, event_level, source_type, source_id, \
                 title, content, metadata, created_at \
                 FROM events WHERE thing_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(thing_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

// ──────────────────────────────────────────────
// Summary 侧查询（自 cloud thing/summary.rs 迁入）
// ──────────────────────────────────────────────

/// Mark a thing's summary dirty (resource attach/detach/update trigger).
pub(crate) async fn mark_thing_summary_dirty(
    pool: &SqlitePool,
    thing_id: &str,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("UPDATE things SET summary_status = 'dirty' WHERE id = ?")
        .bind(thing_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Read (ontology_summary, summary_status) for a thing.
pub(crate) async fn get_thing_summary_state(
    pool: &SqlitePool,
    thing_id: &str,
) -> std::result::Result<Option<(Option<String>, Option<String>)>, sqlx::Error> {
    sqlx::query_as("SELECT ontology_summary, summary_status FROM things WHERE id = ?")
        .bind(thing_id)
        .fetch_optional(pool)
        .await
}

/// Read the cached ontology summary for a thing.
pub(crate) async fn get_thing_summary(
    pool: &SqlitePool,
    thing_id: &str,
) -> std::result::Result<Option<String>, sqlx::Error> {
    let row: Option<(Option<String>,)> = sqlx::query_as("SELECT ontology_summary FROM things WHERE id = ?")
        .bind(thing_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|(s,)| s))
}

/// Persist a computed summary and mark status 'ok'.
pub(crate) async fn save_thing_summary(
    pool: &SqlitePool,
    thing_id: &str,
    text: &str,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE things SET ontology_summary = ?, summary_status = 'ok', \
                     updated_at = datetime('now') WHERE id = ?",
    )
    .bind(text)
    .bind(thing_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark summary status 'failed' (keep cached summary).
pub(crate) async fn mark_thing_summary_failed(
    pool: &SqlitePool,
    thing_id: &str,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE things SET summary_status = 'failed', \
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
) -> std::result::Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as("SELECT name, thing_type FROM things WHERE id = ?")
        .bind(thing_id)
        .fetch_optional(pool)
        .await
}

/// Breadcrumb names from root to this thing (recursive CTE, depth cap 10).
pub(crate) async fn get_thing_breadcrumb_names(
    pool: &SqlitePool,
    thing_id: &str,
) -> std::result::Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "WITH RECURSIVE ancestors AS (
            SELECT id, name, parent_id, 0 AS depth FROM things WHERE id = ?
            UNION ALL
            SELECT d.id, d.name, d.parent_id, a.depth + 1
            FROM things d JOIN ancestors a ON d.id = a.parent_id
            WHERE a.depth < 10
        ) SELECT name FROM ancestors ORDER BY depth DESC",
    )
    .bind(thing_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(name,)| name).collect())
}

/// Property names of a thing (model definition input).
pub(crate) async fn list_thing_property_names(
    pool: &SqlitePool,
    thing_id: &str,
) -> std::result::Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT name FROM thing_properties WHERE thing_id = ? ORDER BY name")
        .bind(thing_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

/// Action names of a thing (model definition input).
pub(crate) async fn list_thing_action_names(
    pool: &SqlitePool,
    thing_id: &str,
) -> std::result::Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT name FROM thing_actions WHERE thing_id = ? ORDER BY name")
        .bind(thing_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

/// Knowledge doc (name, content) snippets for a thing, newest first, max 5.
pub(crate) async fn list_thing_knowledge_doc_snippets(
    pool: &SqlitePool,
    thing_id: &str,
) -> std::result::Result<Vec<(String, Option<String>)>, sqlx::Error> {
    sqlx::query_as("SELECT name, content FROM resources WHERE thing_id = ? ORDER BY created_at DESC LIMIT 5")
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
    pub thing_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Count thing_actions matching device + name.
pub(crate) async fn count_thing_action_by_name(
    pool: &SqlitePool,
    thing_id: &str,
    name: &str,
) -> std::result::Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM thing_actions WHERE thing_id = ? AND name = ?")
        .bind(thing_id)
        .bind(name)
        .fetch_one(pool)
        .await
}

/// Action 参数 schema（无行或 NULL 均为 None）。
pub(crate) async fn find_thing_action_parameters(
    pool: &SqlitePool,
    thing_id: &str,
    name: &str,
) -> std::result::Result<Option<String>, sqlx::Error> {
    let row: Option<Option<String>> =
        sqlx::query_scalar("SELECT parameters FROM thing_actions WHERE thing_id = ? AND name = ?")
            .bind(thing_id)
            .bind(name)
            .fetch_optional(pool)
            .await?;
    Ok(row.flatten())
}

/// 查询属性定义行。
pub(crate) async fn find_thing_property(
    pool: &SqlitePool,
    thing_id: &str,
    name: &str,
) -> std::result::Result<Option<ThingPropertyRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingPropertyRow>(
        "SELECT name, display_name, description, data_type, unit, \
             min_value, max_value, default_value, is_read_only \
             FROM thing_properties WHERE thing_id = ? AND name = ?",
    )
    .bind(thing_id)
    .bind(name)
    .fetch_optional(pool)
    .await
}

/// 查询文档完整行（workspace 作用域）。
pub(crate) async fn find_thing_document(
    pool: &SqlitePool,
    resource_id: &str,
    workspace_id: &str,
) -> std::result::Result<Option<ThingDocumentRow>, sqlx::Error> {
    sqlx::query_as::<_, ThingDocumentRow>(
        "SELECT id, name, resource_type AS type, file_path, content, tags, thing_id, \
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
) -> std::result::Result<Vec<ThingKnowledgeDocRow>, sqlx::Error> {
    let like_pattern = format!("%{}%", q);

    // Build dynamic query with QueryBuilder
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT id, name, resource_type AS type, file_path, tags, created_at, updated_at \
         FROM resources WHERE workspace_id = ",
    );
    builder.push_bind(workspace_id);

    if let Some(tid) = thing_id {
        builder.push(" AND thing_id = ");
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
    pub async fn find_thing_by_id_scoped(
        &self,
        id: &str,
        workspace_id: &str,
    ) -> std::result::Result<Option<ThingRow>, sqlx::Error> {
        find_thing_by_id_scoped(self.pool(), id, workspace_id).await
    }

    /// Workspace-scoped thing 删除。
    pub async fn delete_thing_scoped(&self, id: &str, workspace_id: &str) -> std::result::Result<u64, sqlx::Error> {
        delete_thing_scoped(self.pool(), id, workspace_id).await
    }

    /// Workspace-scoped 按名查询。
    pub async fn find_thing_row_by_name(
        &self,
        workspace_id: &str,
        name: &str,
    ) -> std::result::Result<Option<ThingRow>, sqlx::Error> {
        find_thing_row_by_name(self.pool(), workspace_id, name).await
    }

    /// 分页列出 things（动态 WHERE）。
    pub async fn list_things(
        &self,
        workspace_id: &str,
        params: &ListThingsParams,
    ) -> std::result::Result<(Vec<ThingRow>, u64), sqlx::Error> {
        list_things(self.pool(), workspace_id, params).await
    }

    /// 按 id 查单个 thing。
    pub async fn find_thing_row_by_id(&self, id: &str) -> std::result::Result<Option<ThingRow>, sqlx::Error> {
        find_thing_row_by_id(self.pool(), id).await
    }

    /// 插入 thing 并返回新行。
    pub async fn create_thing_row(&self, row: &ThingRow) -> std::result::Result<ThingRow, sqlx::Error> {
        create_thing_row(self.pool(), row).await
    }

    /// 更新 thing 并返回更新后的行。
    pub async fn update_thing_row(
        &self,
        id: &str,
        input: &UpdateThingRowRequest,
    ) -> std::result::Result<Option<ThingRow>, sqlx::Error> {
        update_thing_row(self.pool(), id, input).await
    }

    /// 删除 thing，返回受影响行数。
    pub async fn delete_thing_row(&self, id: &str) -> std::result::Result<u64, sqlx::Error> {
        delete_thing_row(self.pool(), id).await
    }

    /// 统计 thing 的子节点数。
    pub async fn count_thing_children(&self, id: &str) -> std::result::Result<i64, sqlx::Error> {
        count_thing_children(self.pool(), id).await
    }

    /// 递归 CTE 查询 thing 树。
    pub async fn get_thing_tree(
        &self,
        root_id: Option<&str>,
        workspace_id: &str,
        max_depth: u32,
    ) -> std::result::Result<Vec<ThingTreeNode>, sqlx::Error> {
        get_thing_tree(self.pool(), root_id, workspace_id, max_depth).await
    }

    /// 查询 thing 的面包屑（向上父链，最大深度 10）。
    pub async fn get_thing_breadcrumb(
        &self,
        id: &str,
        max_depth: u32,
    ) -> std::result::Result<Vec<BreadcrumbNode>, sqlx::Error> {
        get_thing_breadcrumb(self.pool(), id, max_depth).await
    }

    /// 环检测：candidate_parent 的祖先链上是否含 thing。
    pub async fn check_thing_cycle(
        &self,
        thing_id: &str,
        candidate_parent_id: &str,
    ) -> std::result::Result<bool, sqlx::Error> {
        check_thing_cycle(self.pool(), thing_id, candidate_parent_id).await
    }

    /// 事务内 cycle check + UPDATE（TOCTOU 安全，eng-review T11）。
    /// 唯一允许 Db 方法内起事务的形态：事务体在 `update_thing_guarded_tx`。
    pub async fn update_thing_guarded(
        &self,
        id: &str,
        input: &UpdateThingRowRequest,
        workspace_id: &str,
    ) -> std::result::Result<UpdateGuardedOutcome, sqlx::Error> {
        let mut tx = self.pool().begin().await?;
        if update_thing_guarded_tx(&mut tx, id, input, workspace_id).await? {
            // 检测到环：未执行写入，丢弃事务（rollback）。
            return Ok(UpdateGuardedOutcome::Cycle);
        }
        tx.commit().await?;
        Ok(UpdateGuardedOutcome::Updated(
            find_thing_row_by_id(self.pool(), id).await?.map(Box::new),
        ))
    }

    /// 单查询批量面包屑（key 为 thing ID）。
    pub async fn get_thing_breadcrumbs(
        &self,
        ids: &[String],
        max_depth: u32,
    ) -> std::result::Result<std::collections::HashMap<String, Vec<BreadcrumbNode>>, sqlx::Error> {
        get_thing_breadcrumbs(self.pool(), ids, max_depth).await
    }

    /// 将子树全部标记 summary_status='dirty'。
    pub async fn mark_thing_subtree_dirty(&self, root_id: &str) -> std::result::Result<u64, sqlx::Error> {
        mark_thing_subtree_dirty(self.pool(), root_id).await
    }

    /// 解除 resource 与 thing 的挂载（thing_id = NULL）。
    pub async fn detach_thing_resource(
        &self,
        thing_id: &str,
        resource_id: &str,
        workspace_id: &str,
    ) -> std::result::Result<u64, sqlx::Error> {
        detach_thing_resource(self.pool(), thing_id, resource_id, workspace_id).await
    }

    /// 挂载 resource 到 thing。
    pub async fn attach_thing_resource(
        &self,
        thing_id: &str,
        resource_id: &str,
        workspace_id: &str,
    ) -> std::result::Result<u64, sqlx::Error> {
        attach_thing_resource(self.pool(), thing_id, resource_id, workspace_id).await
    }

    /// 列出未挂载的 resources。
    pub async fn list_unassigned_thing_resources(
        &self,
        workspace_id: &str,
    ) -> std::result::Result<Vec<ThingResource>, sqlx::Error> {
        list_unassigned_thing_resources(self.pool(), workspace_id).await
    }

    /// 批量加载 thing 标签（tag_bindings）。
    pub async fn load_thing_tags_batch(
        &self,
        thing_ids: &[&str],
    ) -> std::result::Result<std::collections::HashMap<String, Vec<TagInfo>>, sqlx::Error> {
        load_thing_tags_batch(self.pool(), thing_ids).await
    }

    /// Thing 挂载的知识文档（新的在前）。
    pub async fn list_thing_knowledge_docs(
        &self,
        thing_id: &str,
        limit: i64,
    ) -> std::result::Result<Vec<DocRow>, sqlx::Error> {
        list_thing_knowledge_docs(self.pool(), thing_id, limit).await
    }

    /// Thing 的最近事件（新的在前）。
    pub async fn list_thing_recent_events(
        &self,
        thing_id: &str,
        limit: i64,
    ) -> std::result::Result<Vec<EventRow>, sqlx::Error> {
        list_thing_recent_events(self.pool(), thing_id, limit).await
    }

    /// 标记 thing 摘要 dirty（resource 变更触发）。
    pub async fn mark_thing_summary_dirty(&self, thing_id: &str) -> std::result::Result<(), sqlx::Error> {
        mark_thing_summary_dirty(self.pool(), thing_id).await
    }

    /// 读取 thing 的 (ontology_summary, summary_status)。
    pub async fn get_thing_summary_state(
        &self,
        thing_id: &str,
    ) -> std::result::Result<Option<(Option<String>, Option<String>)>, sqlx::Error> {
        get_thing_summary_state(self.pool(), thing_id).await
    }

    /// 读取 thing 的缓存摘要。
    pub async fn get_thing_summary(&self, thing_id: &str) -> std::result::Result<Option<String>, sqlx::Error> {
        get_thing_summary(self.pool(), thing_id).await
    }

    /// 持久化摘要并标记 'ok'。
    pub async fn save_thing_summary(&self, thing_id: &str, text: &str) -> std::result::Result<(), sqlx::Error> {
        save_thing_summary(self.pool(), thing_id, text).await
    }

    /// 标记摘要 'failed'（保留缓存）。
    pub async fn mark_thing_summary_failed(&self, thing_id: &str) -> std::result::Result<(), sqlx::Error> {
        mark_thing_summary_failed(self.pool(), thing_id).await
    }

    /// 读取 thing 的 (name, thing_type)。
    pub async fn find_thing_name_and_type(
        &self,
        thing_id: &str,
    ) -> std::result::Result<Option<(String, String)>, sqlx::Error> {
        find_thing_name_and_type(self.pool(), thing_id).await
    }

    /// 面包屑名称链（根到本节点）。
    pub async fn get_thing_breadcrumb_names(&self, thing_id: &str) -> std::result::Result<Vec<String>, sqlx::Error> {
        get_thing_breadcrumb_names(self.pool(), thing_id).await
    }

    /// Thing 的属性名列表。
    pub async fn list_thing_property_names(&self, thing_id: &str) -> std::result::Result<Vec<String>, sqlx::Error> {
        list_thing_property_names(self.pool(), thing_id).await
    }

    /// Thing 的动作名列表。
    pub async fn list_thing_action_names(&self, thing_id: &str) -> std::result::Result<Vec<String>, sqlx::Error> {
        list_thing_action_names(self.pool(), thing_id).await
    }

    /// Thing 的知识文档摘要（新的在前，最多 5 条）。
    pub async fn list_thing_knowledge_doc_snippets(
        &self,
        thing_id: &str,
    ) -> std::result::Result<Vec<(String, Option<String>)>, sqlx::Error> {
        list_thing_knowledge_doc_snippets(self.pool(), thing_id).await
    }

    /// 统计 thing_actions 中 device + name 匹配数。
    pub async fn count_thing_action_by_name(
        &self,
        thing_id: &str,
        name: &str,
    ) -> std::result::Result<i64, sqlx::Error> {
        count_thing_action_by_name(self.pool(), thing_id, name).await
    }

    /// Action 参数 schema（无行或 NULL 均为 None）。
    pub async fn find_thing_action_parameters(
        &self,
        thing_id: &str,
        name: &str,
    ) -> std::result::Result<Option<String>, sqlx::Error> {
        find_thing_action_parameters(self.pool(), thing_id, name).await
    }

    /// 查询 thing 属性定义行。
    pub async fn find_thing_property(
        &self,
        thing_id: &str,
        name: &str,
    ) -> std::result::Result<Option<ThingPropertyRow>, sqlx::Error> {
        find_thing_property(self.pool(), thing_id, name).await
    }

    /// 查询 thing 文档完整行（workspace 作用域）。
    pub async fn find_thing_document(
        &self,
        resource_id: &str,
        workspace_id: &str,
    ) -> std::result::Result<Option<ThingDocumentRow>, sqlx::Error> {
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
    ) -> std::result::Result<Vec<ThingKnowledgeDocRow>, sqlx::Error> {
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
    thing_id: &str,
    workspace_id: &str,
) -> std::result::Result<Vec<OpenThingPropertyRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT name, display_name, data_type, value, unit, updated_at FROM thing_properties          WHERE thing_id = ? AND thing_id IN (SELECT id FROM things WHERE workspace_id = ?)          ORDER BY created_at DESC",
    )
    .bind(thing_id)
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
    thing_id: &str,
    workspace_id: &str,
) -> std::result::Result<Vec<OpenThingCommandRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, name, display_name, description, parameters FROM thing_actions          WHERE thing_id = ? AND thing_id IN (SELECT id FROM things WHERE workspace_id = ?) ORDER BY name",
    )
    .bind(thing_id)
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
        thing_id: &str,
        workspace_id: &str,
    ) -> std::result::Result<Vec<OpenThingPropertyRow>, sqlx::Error> {
        list_open_thing_properties(self.pool(), thing_id, workspace_id).await
    }

    /// Open API：列出 thing 命令（workspace 作用域子查询）。
    pub async fn list_open_thing_commands(
        &self,
        thing_id: &str,
        workspace_id: &str,
    ) -> std::result::Result<Vec<OpenThingCommandRow>, sqlx::Error> {
        list_open_thing_commands(self.pool(), thing_id, workspace_id).await
    }
}

// ══════════════════════════════════════════════
// 以下自 device.rs 并入（Task 5 模块收敛）：
// Thing/ThingCriteria 等 Rust 类型名保持不变（PR-2 再改类型名），
// SQL 已查询 things 表（Task 4 完成）。
// ══════════════════════════════════════════════

// ── 查询契约类型（自 core::repository::device 迁入，E6a）──

/// Criteria for querying devices
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThingCriteria {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub category: Option<String>,
    pub address: Option<String>,
    pub driver_name: Option<String>,
    pub state: Option<i32>,
    pub parent_id: Option<String>,
    pub template_id: Option<String>,
    pub workspace_id: Option<String>,
    pub search_text: Option<String>,
    pub tag_name: Option<String>,
    pub sort_by: ThingSortBy,
    pub sort_order: ThingSortOrder,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Sorting options for devices
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum ThingSortBy {
    Name,
    #[default]
    CreatedAt,
    UpdatedAt,
    Category,
    DriverName,
    State,
}

/// Sort order for devices
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum ThingSortOrder {
    Ascending,
    #[default]
    Descending,
}

impl ThingCriteria {
    pub fn builder() -> ThingCriteriaBuilder {
        ThingCriteriaBuilder::new()
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    pub fn with_display_name(mut self, display_name: String) -> Self {
        self.display_name = Some(display_name);
        self
    }

    pub fn with_category(mut self, category: String) -> Self {
        self.category = Some(category);
        self
    }

    pub fn with_address(mut self, address: String) -> Self {
        self.address = Some(address);
        self
    }

    pub fn with_driver_name(mut self, driver_name: String) -> Self {
        self.driver_name = Some(driver_name);
        self
    }

    pub fn with_state(mut self, state: i32) -> Self {
        self.state = Some(state);
        self
    }

    pub fn with_parent_id(mut self, parent_id: String) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    pub fn with_template_id(mut self, template_id: String) -> Self {
        self.template_id = Some(template_id);
        self
    }

    pub fn with_workspace_id(mut self, workspace_id: String) -> Self {
        self.workspace_id = Some(workspace_id);
        self
    }

    pub fn with_search_text(mut self, text: String) -> Self {
        self.search_text = Some(text);
        self
    }

    pub fn with_tag_name(mut self, tag_name: String) -> Self {
        self.tag_name = Some(tag_name);
        self
    }

    pub fn with_sort(mut self, sort_by: ThingSortBy, sort_order: ThingSortOrder) -> Self {
        self.sort_by = sort_by;
        self.sort_order = sort_order;
        self
    }

    pub fn with_pagination(mut self, limit: u32, offset: u32) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

/// Builder for ThingCriteria
pub struct ThingCriteriaBuilder {
    criteria: ThingCriteria,
}

impl ThingCriteriaBuilder {
    pub fn new() -> Self {
        Self {
            criteria: ThingCriteria::default(),
        }
    }

    pub fn name(mut self, name: String) -> Self {
        self.criteria.name = Some(name);
        self
    }

    pub fn display_name(mut self, display_name: String) -> Self {
        self.criteria.display_name = Some(display_name);
        self
    }

    pub fn category(mut self, category: String) -> Self {
        self.criteria.category = Some(category);
        self
    }

    pub fn address(mut self, address: String) -> Self {
        self.criteria.address = Some(address);
        self
    }

    pub fn driver_name(mut self, driver_name: String) -> Self {
        self.criteria.driver_name = Some(driver_name);
        self
    }

    pub fn state(mut self, state: i32) -> Self {
        self.criteria.state = Some(state);
        self
    }

    pub fn parent_id(mut self, parent_id: String) -> Self {
        self.criteria.parent_id = Some(parent_id);
        self
    }

    pub fn template_id(mut self, template_id: String) -> Self {
        self.criteria.template_id = Some(template_id);
        self
    }

    pub fn workspace_id(mut self, workspace_id: String) -> Self {
        self.criteria.workspace_id = Some(workspace_id);
        self
    }

    pub fn search_text(mut self, text: String) -> Self {
        self.criteria.search_text = Some(text);
        self
    }

    pub fn tag_name(mut self, tag_name: String) -> Self {
        self.criteria.tag_name = Some(tag_name);
        self
    }

    pub fn sort_by(mut self, sort_by: ThingSortBy) -> Self {
        self.criteria.sort_by = sort_by;
        self
    }

    pub fn sort_order(mut self, sort_order: ThingSortOrder) -> Self {
        self.criteria.sort_order = sort_order;
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.criteria.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.criteria.offset = Some(offset);
        self
    }

    pub fn build(self) -> ThingCriteria {
        self.criteria
    }
}

impl Default for ThingCriteriaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_criteria_builder() {
        let criteria = ThingCriteria::builder()
            .name("sensor-01".to_string())
            .category("temperature".to_string())
            .driver_name("modbus".to_string())
            .state(1)
            .sort_by(ThingSortBy::Name)
            .sort_order(ThingSortOrder::Ascending)
            .limit(100)
            .offset(0)
            .build();

        assert_eq!(criteria.name, Some("sensor-01".to_string()));
        assert_eq!(criteria.category, Some("temperature".to_string()));
        assert_eq!(criteria.driver_name, Some("modbus".to_string()));
        assert_eq!(criteria.state, Some(1));
        assert!(matches!(criteria.sort_by, ThingSortBy::Name));
        assert!(matches!(criteria.sort_order, ThingSortOrder::Ascending));
        assert_eq!(criteria.limit, Some(100));
        assert_eq!(criteria.offset, Some(0));
    }

    #[test]
    fn test_criteria_fluent_interface() {
        let criteria = ThingCriteria::default()
            .with_name("sensor-02".to_string())
            .with_state(0)
            .with_sort(ThingSortBy::State, ThingSortOrder::Descending)
            .with_pagination(50, 10);

        assert_eq!(criteria.name, Some("sensor-02".to_string()));
        assert_eq!(criteria.state, Some(0));
        assert!(matches!(criteria.sort_by, ThingSortBy::State));
        assert!(matches!(criteria.sort_order, ThingSortOrder::Descending));
        assert_eq!(criteria.limit, Some(50));
        assert_eq!(criteria.offset, Some(10));
    }
}

// ──────────────────────────────────────────────
// 设备持久化自由函数（pub(crate)，pool 首参）
// workspace_scope: Some(ws) 时按租户作用域过滤（E6a 合并原
// TenantThingRepository 行为）；三处内部事务（update/delete_by_ids/
// create_batch/update_states_batch/update_status_batch/scoped create_batch）
// 保持函数内自包含。SQL 与原仓储实现逐字一致。
// ──────────────────────────────────────────────

async fn find_thing_by_id_inner(pool: &SqlitePool, id: &str) -> Result<Option<Thing>> {
    let sql = format!("SELECT {} FROM things WHERE id = ?", thing_row_mapper::SELECT_COLUMNS);
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_optional(pool)
        .await?;

    if let Some(row) = row {
        Ok(Some(thing_row_mapper::row_to_thing(row)?))
    } else {
        Ok(None)
    }
}

async fn find_thing_by_name_inner(pool: &SqlitePool, name: &str) -> Result<Option<Thing>> {
    let sql = format!("SELECT {} FROM things WHERE name = ?", thing_row_mapper::SELECT_COLUMNS);
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(name)
        .fetch_optional(pool)
        .await?;

    if let Some(row) = row {
        Ok(Some(thing_row_mapper::row_to_thing(row)?))
    } else {
        Ok(None)
    }
}

async fn find_things_inner(pool: &SqlitePool, criteria: &ThingCriteria) -> Result<Vec<Thing>> {
    let mut builder = QueryBuilder::new("SELECT ");
    builder.push(thing_row_mapper::SELECT_COLUMNS);
    builder.push(" FROM things WHERE 1=1");
    if let Some(workspace_id) = &criteria.workspace_id {
        builder.push(" AND workspace_id = ").push_bind(workspace_id);
    }
    if let Some(name) = &criteria.name {
        builder.push(" AND name LIKE ").push_bind(format!("%{}%", name));
    }
    if let Some(display_name) = &criteria.display_name {
        builder
            .push(" AND display_name LIKE ")
            .push_bind(format!("%{}%", display_name));
    }
    if let Some(category) = &criteria.category {
        builder.push(" AND category = ").push_bind(category);
    }
    if let Some(address) = &criteria.address {
        builder.push(" AND address LIKE ").push_bind(format!("%{}%", address));
    }
    if let Some(driver_name) = &criteria.driver_name {
        builder.push(" AND driver_name = ").push_bind(driver_name);
    }
    if let Some(state) = &criteria.state {
        builder.push(" AND state = ").push_bind(state);
    }
    if let Some(parent_id) = &criteria.parent_id {
        builder.push(" AND parent_id = ").push_bind(parent_id);
    }
    if let Some(template_id) = &criteria.template_id {
        builder.push(" AND template_id = ").push_bind(template_id);
    }
    if let Some(search_text) = &criteria.search_text {
        let keywords: Vec<&str> = search_text.split_whitespace().collect();
        if !keywords.is_empty() {
            builder.push(" AND (");
            for (i, kw) in keywords.iter().enumerate() {
                let pattern = format!("%{}%", kw);
                if i > 0 {
                    builder.push(" OR ");
                }
                builder.push("(name LIKE ").push_bind(&pattern);
                builder.push(" OR display_name LIKE ").push_bind(&pattern);
                builder.push(" OR address LIKE ").push_bind(&pattern);
                builder.push(" OR description LIKE ").push_bind(&pattern);
                builder.push(" OR EXISTS (SELECT 1 FROM tag_bindings tb JOIN tags t ON tb.tag_id = t.id WHERE tb.target_id = things.id AND tb.target_type = 'thing' AND t.name LIKE ");
                builder.push_bind(&pattern);
                builder.push("))");
            }
            builder.push(")");
        }
    }
    if let Some(tag_name) = &criteria.tag_name {
        let pattern = format!("%{}%", tag_name);
        builder.push(" AND EXISTS (SELECT 1 FROM tag_bindings tb JOIN tags t ON tb.tag_id = t.id WHERE tb.target_id = things.id AND tb.target_type = 'thing' AND t.name LIKE ");
        builder.push_bind(&pattern);
        builder.push(")");
    }

    match criteria.sort_by {
        ThingSortBy::Name => builder.push(" ORDER BY name"),
        ThingSortBy::CreatedAt => builder.push(" ORDER BY created_at"),
        ThingSortBy::UpdatedAt => builder.push(" ORDER BY updated_at"),
        ThingSortBy::Category => builder.push(" ORDER BY category"),
        ThingSortBy::DriverName => builder.push(" ORDER BY driver_name"),
        ThingSortBy::State => builder.push(" ORDER BY state"),
    };

    match criteria.sort_order {
        ThingSortOrder::Ascending => builder.push(" ASC"),
        ThingSortOrder::Descending => builder.push(" DESC"),
    };

    if let Some(limit) = criteria.limit {
        builder.push(" LIMIT ").push_bind(limit as i64);
    }
    if let Some(offset) = criteria.offset {
        builder.push(" OFFSET ").push_bind(offset as i64);
    }

    let rows = builder.build().fetch_all(pool).await?;
    let mut devices = Vec::new();
    for row in rows {
        devices.push(thing_row_mapper::row_to_thing(row)?);
    }
    Ok(devices)
}

async fn count_things_inner(pool: &SqlitePool, criteria: &ThingCriteria) -> Result<i64> {
    let mut builder = QueryBuilder::new("SELECT COUNT(*) as count FROM things WHERE 1=1");
    if let Some(workspace_id) = &criteria.workspace_id {
        builder.push(" AND workspace_id = ").push_bind(workspace_id);
    }
    if let Some(name) = &criteria.name {
        builder.push(" AND name LIKE ").push_bind(format!("%{}%", name));
    }
    if let Some(display_name) = &criteria.display_name {
        builder
            .push(" AND display_name LIKE ")
            .push_bind(format!("%{}%", display_name));
    }
    if let Some(category) = &criteria.category {
        builder.push(" AND category = ").push_bind(category);
    }
    if let Some(address) = &criteria.address {
        builder.push(" AND address LIKE ").push_bind(format!("%{}%", address));
    }
    if let Some(driver_name) = &criteria.driver_name {
        builder.push(" AND driver_name = ").push_bind(driver_name);
    }
    if let Some(state) = &criteria.state {
        builder.push(" AND state = ").push_bind(state);
    }
    if let Some(parent_id) = &criteria.parent_id {
        builder.push(" AND parent_id = ").push_bind(parent_id);
    }
    if let Some(template_id) = &criteria.template_id {
        builder.push(" AND template_id = ").push_bind(template_id);
    }
    if let Some(search_text) = &criteria.search_text {
        let keywords: Vec<&str> = search_text.split_whitespace().collect();
        if !keywords.is_empty() {
            builder.push(" AND (");
            for (i, kw) in keywords.iter().enumerate() {
                let pattern = format!("%{}%", kw);
                if i > 0 {
                    builder.push(" OR ");
                }
                builder.push("(name LIKE ").push_bind(&pattern);
                builder.push(" OR display_name LIKE ").push_bind(&pattern);
                builder.push(" OR address LIKE ").push_bind(&pattern);
                builder.push(" OR description LIKE ").push_bind(&pattern);
                builder.push(" OR EXISTS (SELECT 1 FROM tag_bindings tb JOIN tags t ON tb.tag_id = t.id WHERE tb.target_id = things.id AND tb.target_type = 'thing' AND t.name LIKE ");
                builder.push_bind(&pattern);
                builder.push("))");
            }
            builder.push(")");
        }
    }
    if let Some(tag_name) = &criteria.tag_name {
        let pattern = format!("%{}%", tag_name);
        builder.push(" AND EXISTS (SELECT 1 FROM tag_bindings tb JOIN tags t ON tb.tag_id = t.id WHERE tb.target_id = things.id AND tb.target_type = 'thing' AND t.name LIKE ");
        builder.push_bind(&pattern);
        builder.push(")");
    }

    let row = builder.build().fetch_one(pool).await?;
    let count: i64 = row.get("count");
    Ok(count)
}

async fn create_thing_inner(pool: &SqlitePool, request: &CreateThingRequest) -> Result<Thing> {
    let id = generate_id();
    let now = now_string();

    sqlx::query(
        r#"
        INSERT INTO things (
            id, name, display_name, category, address, description, position,
            driver_name, device_model, protocol_type, factory_name, linked_data,
            driver_options, state, parent_id, template_id,
            linked_gateway, fingerprint, workspace_id, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&request.name)
    .bind(&request.display_name)
    .bind(&request.category)
    .bind(&request.address)
    .bind(&request.description)
    .bind(&request.position)
    .bind(&request.driver_name)
    .bind(&request.device_model)
    .bind(&request.protocol_type)
    .bind(&request.factory_name)
    .bind(&request.linked_data)
    .bind(&request.driver_options)
    .bind(0i32)
    .bind(&request.parent_id)
    .bind(&request.template_id)
    .bind(&request.linked_gateway)
    .bind(&request.fingerprint)
    .bind(&request.workspace_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    find_thing_by_id_inner(pool, &id).await?.ok_or(Error::NotFound)
}

async fn update_thing_inner(pool: &SqlitePool, id: &str, request: &UpdateThingRequest) -> Result<Thing> {
    let mut tx = pool.begin().await?;

    let mut builder = QueryBuilder::new("UPDATE things SET ");
    let mut has_updates = false;
    let now = now_string();

    if let Some(name) = &request.name {
        if has_updates {
            builder.push(", ");
        }
        builder.push("name = ").push_bind(name);
        has_updates = true;
    }
    if let Some(display_name) = &request.display_name {
        if has_updates {
            builder.push(", ");
        }
        builder.push("display_name = ").push_bind(display_name);
        has_updates = true;
    }
    if let Some(category) = &request.category {
        if has_updates {
            builder.push(", ");
        }
        builder.push("category = ").push_bind(category);
        has_updates = true;
    }
    if let Some(address) = &request.address {
        if has_updates {
            builder.push(", ");
        }
        builder.push("address = ").push_bind(address);
        has_updates = true;
    }
    if let Some(description) = &request.description {
        if has_updates {
            builder.push(", ");
        }
        builder.push("description = ").push_bind(description);
        has_updates = true;
    }
    if let Some(position) = &request.position {
        if has_updates {
            builder.push(", ");
        }
        builder.push("position = ").push_bind(position);
        has_updates = true;
    }
    if let Some(driver_name) = &request.driver_name {
        if has_updates {
            builder.push(", ");
        }
        builder.push("driver_name = ").push_bind(driver_name);
        has_updates = true;
    }
    if let Some(device_model) = &request.device_model {
        if has_updates {
            builder.push(", ");
        }
        builder.push("device_model = ").push_bind(device_model);
        has_updates = true;
    }
    if let Some(protocol_type) = &request.protocol_type {
        if has_updates {
            builder.push(", ");
        }
        builder.push("protocol_type = ").push_bind(protocol_type);
        has_updates = true;
    }
    if let Some(factory_name) = &request.factory_name {
        if has_updates {
            builder.push(", ");
        }
        builder.push("factory_name = ").push_bind(factory_name);
        has_updates = true;
    }
    if let Some(linked_data) = &request.linked_data {
        if has_updates {
            builder.push(", ");
        }
        builder.push("linked_data = ").push_bind(linked_data);
        has_updates = true;
    }
    if let Some(linked_gateway) = &request.linked_gateway {
        if has_updates {
            builder.push(", ");
        }
        builder.push("linked_gateway = ").push_bind(linked_gateway);
        has_updates = true;
    }
    if let Some(fingerprint) = &request.fingerprint {
        if has_updates {
            builder.push(", ");
        }
        builder.push("fingerprint = ").push_bind(fingerprint);
        has_updates = true;
    }
    if let Some(driver_options) = &request.driver_options {
        if has_updates {
            builder.push(", ");
        }
        builder.push("driver_options = ").push_bind(driver_options);
        has_updates = true;
    }
    if let Some(state) = &request.state {
        if has_updates {
            builder.push(", ");
        }
        builder.push("state = ").push_bind(state);
        has_updates = true;
    }
    if let Some(parent_id) = &request.parent_id {
        if has_updates {
            builder.push(", ");
        }
        builder.push("parent_id = ").push_bind(parent_id);
        has_updates = true;
    }
    if let Some(template_id) = &request.template_id {
        if has_updates {
            builder.push(", ");
        }
        builder.push("template_id = ").push_bind(template_id);
        has_updates = true;
    }

    if !has_updates {
        return find_thing_by_id_inner(pool, id).await?.ok_or(Error::NotFound);
    }

    builder.push(", updated_at = ").push_bind(&now);
    builder.push(" WHERE id = ").push_bind(id);

    let result = builder.build().execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    let sql = format!("SELECT {} FROM things WHERE id = ?", thing_row_mapper::SELECT_COLUMNS);
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_one(&mut *tx)
        .await;

    tx.commit().await?;

    match row {
        Ok(row) => thing_row_mapper::row_to_thing(row),
        Err(_) => Err(Error::NotFound),
    }
}

async fn delete_thing_inner(pool: &SqlitePool, id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM things WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

async fn delete_things_by_ids_inner(pool: &SqlitePool, ids: &[String]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    let mut builder = QueryBuilder::new("DELETE FROM things WHERE id IN (");
    let mut separated = builder.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");

    let result = builder.build().execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(result.rows_affected())
}

async fn create_things_batch_inner(pool: &SqlitePool, requests: &[CreateThingRequest]) -> Result<Vec<Thing>> {
    if requests.is_empty() {
        return Ok(vec![]);
    }

    let mut tx = pool.begin().await?;
    let mut created_things = Vec::new();
    let now = now_string();

    for request in requests {
        let id = generate_id();

        sqlx::query(
            r#"
            INSERT INTO things (
                id, name, display_name, category, address, description, position,
                driver_name, device_model, protocol_type, factory_name, linked_data,
                driver_options, state, parent_id, template_id,
                linked_gateway, fingerprint, workspace_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&request.display_name)
        .bind(&request.category)
        .bind(&request.address)
        .bind(&request.description)
        .bind(&request.position)
        .bind(&request.driver_name)
        .bind(&request.device_model)
        .bind(&request.protocol_type)
        .bind(&request.factory_name)
        .bind(&request.linked_data)
        .bind(&request.driver_options)
        .bind(0i32)
        .bind(&request.parent_id)
        .bind(&request.template_id)
        .bind(&request.linked_gateway)
        .bind(&request.fingerprint)
        .bind(&request.workspace_id)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        let device = Thing {
            id: id.clone(),
            name: request.name.clone(),
            display_name: request.display_name.clone(),
            category: request.category.clone(),
            address: request.address.clone(),
            description: request.description.clone(),
            position: request.position.clone(),
            driver_name: request.driver_name.clone(),
            device_model: request.device_model.clone(),
            protocol_type: request.protocol_type.clone(),
            factory_name: request.factory_name.clone(),
            linked_data: request.linked_data.clone(),
            driver_options: request.driver_options.clone(),
            status: tinyiothub_core::models::thing::ThingStatus::Offline,
            parent_id: request.parent_id.clone(),
            template_id: request.template_id.clone(),
            linked_gateway: request.linked_gateway.clone(),
            fingerprint: request.fingerprint.clone(),
            workspace_id: None,
            created_at: Some(now.clone()),
            updated_at: Some(now.clone()),
            tags: None,
            properties: None,
            commands: None,
            last_heartbeat: None,
        };

        created_things.push(device);
    }

    tx.commit().await?;
    Ok(created_things)
}

async fn update_thing_state_inner(pool: &SqlitePool, id: &str, state: i32) -> Result<()> {
    let now = now_string();
    let result = sqlx::query("UPDATE things SET state = ?, updated_at = ? WHERE id = ?")
        .bind(state)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}

async fn update_thing_states_batch_inner(pool: &SqlitePool, updates: &[(String, i32)]) -> Result<u64> {
    if updates.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    let mut total_affected = 0u64;
    let now = now_string();

    for (id, state) in updates {
        let result = sqlx::query("UPDATE things SET state = ?, updated_at = ? WHERE id = ?")
            .bind(state)
            .bind(&now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        total_affected += result.rows_affected();
    }

    tx.commit().await?;
    Ok(total_affected)
}

async fn update_thing_enabled_status_inner(pool: &SqlitePool, id: &str, enabled: bool) -> Result<bool> {
    let state = if enabled { 1 } else { 0 };
    match update_thing_state_inner(pool, id, state).await {
        Ok(()) => Ok(true),
        Err(Error::NotFound) => Ok(false),
        Err(e) => Err(e),
    }
}

async fn find_thing_children_inner(pool: &SqlitePool, parent_id: &str) -> Result<Vec<Thing>> {
    let sql = format!(
        "SELECT {} FROM things WHERE parent_id = ? ORDER BY name",
        thing_row_mapper::SELECT_COLUMNS
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(parent_id)
        .fetch_all(pool)
        .await?;

    let mut devices = Vec::new();
    for row in rows {
        devices.push(thing_row_mapper::row_to_thing(row)?);
    }
    Ok(devices)
}

async fn find_things_by_template_id_inner(pool: &SqlitePool, template_id: &str) -> Result<Vec<Thing>> {
    let sql = format!(
        "SELECT {} FROM things WHERE template_id = ? ORDER BY name",
        thing_row_mapper::SELECT_COLUMNS
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(template_id)
        .fetch_all(pool)
        .await?;

    let mut devices = Vec::new();
    for row in rows {
        devices.push(thing_row_mapper::row_to_thing(row)?);
    }
    Ok(devices)
}

async fn find_things_by_driver_name_inner(pool: &SqlitePool, driver_name: &str) -> Result<Vec<Thing>> {
    let sql = format!(
        "SELECT {} FROM things WHERE driver_name = ? ORDER BY name",
        thing_row_mapper::SELECT_COLUMNS
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(driver_name)
        .fetch_all(pool)
        .await?;

    let mut devices = Vec::new();
    for row in rows {
        devices.push(thing_row_mapper::row_to_thing(row)?);
    }
    Ok(devices)
}

async fn find_things_by_linked_gateway_inner(pool: &SqlitePool, linked_gateway: &str) -> Result<Vec<Thing>> {
    let sql = format!(
        "SELECT {} FROM things WHERE linked_gateway = ? ORDER BY created_at DESC",
        thing_row_mapper::SELECT_COLUMNS
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(linked_gateway)
        .fetch_all(pool)
        .await?;

    let mut devices = Vec::new();
    for row in rows {
        devices.push(thing_row_mapper::row_to_thing(row)?);
    }
    Ok(devices)
}

async fn thing_exists_by_name_inner(pool: &SqlitePool, name: &str) -> Result<bool> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM things WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await?;
    let count: i64 = row.get("count");
    Ok(count > 0)
}

async fn find_things_by_ids_inner(pool: &SqlitePool, ids: &[String]) -> Result<Vec<Thing>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let mut builder = QueryBuilder::new("SELECT ");
    builder.push(thing_row_mapper::SELECT_COLUMNS);
    builder.push(" FROM things WHERE id IN (");
    let mut separated = builder.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");

    let rows = builder.build().fetch_all(pool).await?;
    let mut devices = Vec::new();
    for row in rows {
        devices.push(thing_row_mapper::row_to_thing(row)?);
    }
    Ok(devices)
}

async fn find_things_with_filters_inner(
    pool: &SqlitePool,
    enabled: Option<bool>,
    search: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<Vec<Thing>> {
    let mut criteria = ThingCriteria {
        limit: Some(page_size),
        offset: Some((page.saturating_sub(1)) * page_size),
        ..Default::default()
    };

    if let Some(enabled) = enabled {
        criteria.state = Some(if enabled { 1 } else { 0 });
    }

    if let Some(search) = search {
        criteria.search_text = Some(search.to_string());
    }

    find_things_inner(pool, &criteria).await
}

async fn update_thing_status_batch_inner(pool: &SqlitePool, updates: &[ThingStatusUpdate]) -> Result<u64> {
    if updates.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    let mut total_affected = 0u64;

    for update in updates {
        let result = sqlx::query("UPDATE things SET state = ?, updated_at = ? WHERE id = ?")
            .bind(update.state)
            .bind(&update.updated_at)
            .bind(&update.thing_id)
            .execute(&mut *tx)
            .await?;
        total_affected += result.rows_affected();
    }

    tx.commit().await?;
    Ok(total_affected)
}

// ── 租户作用域辅助（E6a 合并自 TenantThingRepository）──

/// Check if a device belongs to this workspace
async fn thing_belongs_to_workspace(pool: &SqlitePool, ws: &str, device_id: &str) -> Result<bool> {
    let result: Option<(String,)> = sqlx::query_as("SELECT workspace_id FROM things WHERE id = ?")
        .bind(device_id)
        .fetch_optional(pool)
        .await?;

    match result {
        Some((workspace_id,)) => Ok(workspace_id == ws),
        None => Ok(false), // Thing doesn't exist
    }
}

/// Filter device IDs to only those belonging to this workspace
async fn filter_thing_ids_by_workspace(pool: &SqlitePool, ws: &str, ids: &[String]) -> Result<Vec<String>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // Use QueryBuilder to avoid lifetime issues with dynamic SQL
    let mut query_builder: sqlx::QueryBuilder<sqlx::Sqlite> =
        sqlx::QueryBuilder::new("SELECT id FROM things WHERE workspace_id = ");
    query_builder.push_bind(ws);
    query_builder.push(" AND id IN (");

    let mut separated = query_builder.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    query_builder.push(")");

    let query = query_builder.build();
    let rows = query.fetch_all(pool).await?;
    Ok(rows.into_iter().map(|row| row.get::<String, _>("id")).collect())
}

/// Filter device state updates to only those belonging to this workspace
async fn filter_thing_state_updates_by_workspace(
    pool: &SqlitePool,
    ws: &str,
    updates: &[(String, i32)],
) -> Result<Vec<(String, i32)>> {
    if updates.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<String> = updates.iter().map(|(id, _)| id.clone()).collect();
    let filtered_ids = filter_thing_ids_by_workspace(pool, ws, &ids).await?;

    // Create a set for fast lookup
    let filtered_set: std::collections::HashSet<String> = filtered_ids.into_iter().collect();

    let filtered_updates: Vec<(String, i32)> = updates
        .iter()
        .filter(|(id, _)| filtered_set.contains(id))
        .cloned()
        .collect();

    Ok(filtered_updates)
}

/// Filter device status updates to only those belonging to this workspace
async fn filter_thing_status_updates_by_workspace(
    pool: &SqlitePool,
    ws: &str,
    updates: &[ThingStatusUpdate],
) -> Result<Vec<ThingStatusUpdate>> {
    if updates.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<String> = updates.iter().map(|update| update.thing_id.clone()).collect();
    let filtered_ids = filter_thing_ids_by_workspace(pool, ws, &ids).await?;

    // Create a set for fast lookup
    let filtered_set: std::collections::HashSet<String> = filtered_ids.into_iter().collect();

    let filtered_updates: Vec<ThingStatusUpdate> = updates
        .iter()
        .filter(|update| filtered_set.contains(&update.thing_id))
        .cloned()
        .collect();

    Ok(filtered_updates)
}

// ── 作用域分发层（workspace_scope = Some 时的行为与原
// for_workspace 作用域仓储的公开方法逐字一致）──

pub(crate) async fn find_thing_by_id(pool: &SqlitePool, ws: Option<&str>, id: &str) -> Result<Option<Thing>> {
    let Some(ws) = ws else {
        return find_thing_by_id_inner(pool, id).await;
    };
    // Verify device belongs to this workspace
    if !thing_belongs_to_workspace(pool, ws, id).await? {
        return Ok(None);
    }

    find_thing_by_id_inner(pool, id).await
}

pub(crate) async fn find_thing_by_name(pool: &SqlitePool, ws: Option<&str>, name: &str) -> Result<Option<Thing>> {
    let Some(ws) = ws else {
        return find_thing_by_name_inner(pool, name).await;
    };
    let criteria = ThingCriteria::default()
        .with_name(name.to_string())
        .with_workspace_id(ws.to_string());
    let devices = find_things_inner(pool, &criteria).await?;
    Ok(devices.into_iter().next())
}

pub(crate) async fn find_things(pool: &SqlitePool, ws: Option<&str>, criteria: &ThingCriteria) -> Result<Vec<Thing>> {
    let Some(ws) = ws else {
        return find_things_inner(pool, criteria).await;
    };
    let mut criteria = criteria.clone();
    criteria.workspace_id = Some(ws.to_string());
    find_things_inner(pool, &criteria).await
}

pub(crate) async fn count_things(pool: &SqlitePool, ws: Option<&str>, criteria: &ThingCriteria) -> Result<i64> {
    let Some(ws) = ws else {
        return count_things_inner(pool, criteria).await;
    };
    let mut criteria = criteria.clone();
    criteria.workspace_id = Some(ws.to_string());
    count_things_inner(pool, &criteria).await
}

pub(crate) async fn create_thing(pool: &SqlitePool, ws: Option<&str>, request: &CreateThingRequest) -> Result<Thing> {
    let Some(ws) = ws else {
        return create_thing_inner(pool, request).await;
    };
    let id = generate_id();
    let now = now_string();

    // Insert device with workspace_id
    sqlx::query(
        r#"
        INSERT INTO things (
            id, name, display_name, category, address, description, position,
            driver_name, device_model, protocol_type, factory_name, linked_data,
            driver_options, state, parent_id, template_id, linked_gateway, fingerprint,
            workspace_id, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&request.name)
    .bind(&request.display_name)
    .bind(&request.category)
    .bind(&request.address)
    .bind(&request.description)
    .bind(&request.position)
    .bind(&request.driver_name)
    .bind(&request.device_model)
    .bind(&request.protocol_type)
    .bind(&request.factory_name)
    .bind(&request.linked_data)
    .bind(&request.driver_options)
    .bind(0i32) // default state
    .bind(&request.parent_id)
    .bind(&request.template_id)
    .bind(&request.linked_gateway)
    .bind(&request.fingerprint)
    .bind(ws)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    // Fetch the created device
    find_thing_by_id(pool, Some(ws), &id).await?.ok_or_else(|| {
        tinyiothub_core::error::Error::InvalidArgument(format!("Failed to find created device with id {}", id))
    })
}

pub(crate) async fn update_thing(
    pool: &SqlitePool,
    ws: Option<&str>,
    id: &str,
    request: &UpdateThingRequest,
) -> Result<Thing> {
    let Some(_ws) = ws else {
        return update_thing_inner(pool, id, request).await;
    };
    // Verify device belongs to this workspace before updating
    let device = find_thing_by_id(pool, ws, id).await?;
    if device.is_none() {
        return Err(tinyiothub_core::error::Error::NotFound);
    }

    update_thing_inner(pool, id, request).await
}

pub(crate) async fn delete_thing(pool: &SqlitePool, ws: Option<&str>, id: &str) -> Result<u64> {
    let Some(_ws) = ws else {
        return delete_thing_inner(pool, id).await;
    };
    // Verify device belongs to this workspace before deleting
    let device = find_thing_by_id(pool, ws, id).await?;
    if device.is_none() {
        return Ok(0); // Already doesn't exist in this workspace
    }

    delete_thing_inner(pool, id).await
}

pub(crate) async fn delete_things_by_ids(pool: &SqlitePool, ws: Option<&str>, ids: &[String]) -> Result<u64> {
    let Some(ws) = ws else {
        return delete_things_by_ids_inner(pool, ids).await;
    };
    // Filter IDs to only those belonging to this workspace
    let filtered_ids = filter_thing_ids_by_workspace(pool, ws, ids).await?;
    if filtered_ids.is_empty() {
        return Ok(0);
    }
    delete_things_by_ids_inner(pool, &filtered_ids).await
}

pub(crate) async fn create_things_batch(
    pool: &SqlitePool,
    ws: Option<&str>,
    requests: &[CreateThingRequest],
) -> Result<Vec<Thing>> {
    let Some(ws) = ws else {
        return create_things_batch_inner(pool, requests).await;
    };
    if requests.is_empty() {
        return Ok(vec![]);
    }

    let mut tx = pool.begin().await?;
    let mut device_ids = Vec::new();
    let now = now_string();

    for request in requests {
        let id = generate_id();
        device_ids.push(id.clone());

        sqlx::query(
            r#"
            INSERT INTO things (
                id, name, display_name, category, address, description, position,
                driver_name, device_model, protocol_type, factory_name, linked_data,
                driver_options, state, parent_id, template_id, linked_gateway, fingerprint,
                workspace_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&request.display_name)
        .bind(&request.category)
        .bind(&request.address)
        .bind(&request.description)
        .bind(&request.position)
        .bind(&request.driver_name)
        .bind(&request.device_model)
        .bind(&request.protocol_type)
        .bind(&request.factory_name)
        .bind(&request.linked_data)
        .bind(&request.driver_options)
        .bind(0i32) // default state
        .bind(&request.parent_id)
        .bind(&request.template_id)
        .bind(&request.linked_gateway)
        .bind(&request.fingerprint)
        .bind(ws)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // Fetch created devices
    find_things_by_ids(pool, Some(ws), &device_ids).await
}

pub(crate) async fn update_thing_state(pool: &SqlitePool, ws: Option<&str>, id: &str, state: i32) -> Result<()> {
    let Some(ws) = ws else {
        return update_thing_state_inner(pool, id, state).await;
    };
    let device = find_thing_by_id(pool, Some(ws), id).await?;
    if device.is_none() {
        return Err(tinyiothub_core::error::Error::InvalidArgument(format!(
            "Thing with id {} not found in workspace {}",
            id, ws
        )));
    }

    update_thing_state_inner(pool, id, state).await
}

pub(crate) async fn update_thing_states_batch(
    pool: &SqlitePool,
    ws: Option<&str>,
    updates: &[(String, i32)],
) -> Result<u64> {
    let Some(ws) = ws else {
        return update_thing_states_batch_inner(pool, updates).await;
    };
    // Filter updates to only devices in this workspace
    let filtered_updates = filter_thing_state_updates_by_workspace(pool, ws, updates).await?;
    if filtered_updates.is_empty() {
        return Ok(0);
    }
    update_thing_states_batch_inner(pool, &filtered_updates).await
}

pub(crate) async fn update_thing_enabled_status(
    pool: &SqlitePool,
    ws: Option<&str>,
    id: &str,
    enabled: bool,
) -> Result<bool> {
    let Some(ws) = ws else {
        return update_thing_enabled_status_inner(pool, id, enabled).await;
    };
    let device = find_thing_by_id(pool, Some(ws), id).await?;
    if device.is_none() {
        return Err(tinyiothub_core::error::Error::InvalidArgument(format!(
            "Thing with id {} not found in workspace {}",
            id, ws
        )));
    }

    update_thing_enabled_status_inner(pool, id, enabled).await
}

pub(crate) async fn find_thing_children(pool: &SqlitePool, ws: Option<&str>, parent_id: &str) -> Result<Vec<Thing>> {
    let Some(ws) = ws else {
        return find_thing_children_inner(pool, parent_id).await;
    };
    // Verify parent belongs to this workspace
    if !thing_belongs_to_workspace(pool, ws, parent_id).await? {
        return Ok(vec![]);
    }

    let criteria = ThingCriteria::default()
        .with_parent_id(parent_id.to_string())
        .with_workspace_id(ws.to_string());
    find_things_inner(pool, &criteria).await
}

pub(crate) async fn find_things_by_template_id(
    pool: &SqlitePool,
    ws: Option<&str>,
    template_id: &str,
) -> Result<Vec<Thing>> {
    let Some(ws) = ws else {
        return find_things_by_template_id_inner(pool, template_id).await;
    };
    let criteria = ThingCriteria::default()
        .with_template_id(template_id.to_string())
        .with_workspace_id(ws.to_string());
    find_things_inner(pool, &criteria).await
}

pub(crate) async fn find_things_by_driver_name(
    pool: &SqlitePool,
    ws: Option<&str>,
    driver_name: &str,
) -> Result<Vec<Thing>> {
    let Some(ws) = ws else {
        return find_things_by_driver_name_inner(pool, driver_name).await;
    };
    let criteria = ThingCriteria::default()
        .with_driver_name(driver_name.to_string())
        .with_workspace_id(ws.to_string());
    find_things_inner(pool, &criteria).await
}

pub(crate) async fn find_things_by_linked_gateway(
    pool: &SqlitePool,
    ws: Option<&str>,
    linked_gateway: &str,
) -> Result<Vec<Thing>> {
    let Some(ws) = ws else {
        return find_things_by_linked_gateway_inner(pool, linked_gateway).await;
    };
    let criteria = ThingCriteria::default().with_workspace_id(ws.to_string());
    let all = find_things_inner(pool, &criteria).await?;
    Ok(all
        .into_iter()
        .filter(|d| d.linked_gateway.as_deref() == Some(linked_gateway))
        .collect())
}

/// 按 id 检查设备是否存在（device_traces 等领域的外键前置检查；自 cloud trace_repository 迁入）。
pub(crate) async fn thing_exists_by_id(pool: &SqlitePool, id: &str) -> Result<bool> {
    match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM things WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
    {
        Ok(Some(count)) => Ok(count > 0),
        Ok(None) => Ok(false),
        Err(e) => {
            tracing::debug!("Failed to check device existence for {}: {}", id, e);
            Err(Error::IOError(format!("DB error: {}", e)))
        }
    }
}

pub(crate) async fn thing_exists_by_name(pool: &SqlitePool, ws: Option<&str>, name: &str) -> Result<bool> {
    let Some(_ws) = ws else {
        return thing_exists_by_name_inner(pool, name).await;
    };
    // Check within this workspace
    let criteria = ThingCriteria::builder().name(name.to_string()).build();

    let count = count_things(pool, ws, &criteria).await?;
    Ok(count > 0)
}

pub(crate) async fn find_things_by_ids(pool: &SqlitePool, ws: Option<&str>, ids: &[String]) -> Result<Vec<Thing>> {
    let Some(ws) = ws else {
        return find_things_by_ids_inner(pool, ids).await;
    };
    // Filter IDs to only those belonging to this workspace
    let filtered_ids = filter_thing_ids_by_workspace(pool, ws, ids).await?;
    if filtered_ids.is_empty() {
        return Ok(vec![]);
    }
    find_things_by_ids_inner(pool, &filtered_ids).await
}

pub(crate) async fn find_things_with_filters(
    pool: &SqlitePool,
    ws: Option<&str>,
    enabled: Option<bool>,
    search: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<Vec<Thing>> {
    let Some(_ws) = ws else {
        return find_things_with_filters_inner(pool, enabled, search, page, page_size).await;
    };

    let mut criteria = ThingCriteria::builder()
        .limit(page_size)
        .offset((page.saturating_sub(1)) * page_size)
        .build();

    if let Some(enabled) = enabled {
        // Map enabled boolean to state (1 for enabled, 0 for disabled)
        criteria.state = Some(if enabled { 1 } else { 0 });
    }

    if let Some(search) = search {
        criteria.search_text = Some(search.to_string());
    }

    find_things(pool, ws, &criteria).await
}

pub(crate) async fn update_thing_status_batch(
    pool: &SqlitePool,
    ws: Option<&str>,
    updates: &[ThingStatusUpdate],
) -> Result<u64> {
    let Some(ws) = ws else {
        return update_thing_status_batch_inner(pool, updates).await;
    };
    // Filter updates to only devices in this workspace
    let filtered_updates = filter_thing_status_updates_by_workspace(pool, ws, updates).await?;
    if filtered_updates.is_empty() {
        return Ok(0);
    }
    update_thing_status_batch_inner(pool, &filtered_updates).await
}

// ── Task 7 收编：cloud workspace 删除守卫的设备计数（devices 表归本领域）──

pub(crate) async fn count_things_by_workspace(pool: &SqlitePool, workspace_id: &str) -> Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE workspace_id = ?")
        .bind(workspace_id)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

// ── Task 8 收编：cloud driver/legacy query_service_impl 的设备统计查询
// （devices 表归本领域；SQL 逐字迁移）──

pub(crate) async fn thing_stats_overview(pool: &SqlitePool) -> Result<ThingStats> {
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*) as total_devices,
            COUNT(CASE WHEN state = 1 THEN 1 END) as online_devices,
            COUNT(CASE WHEN state = 0 OR state = 3 THEN 1 END) as offline_devices,
            COUNT(CASE WHEN state = 2 THEN 1 END) as alarm_devices
        FROM things
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(ThingStats {
        total_devices: row.get("total_devices"),
        online_devices: row.get("online_devices"),
        offline_devices: row.get("offline_devices"),
        alarm_devices: row.get("alarm_devices"),
    })
}

pub(crate) async fn count_things_by_type(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    let rows = sqlx::query(
        r#"
        SELECT COALESCE(category, 'Unknown') as category, COUNT(*) as count
        FROM things
        GROUP BY category
        ORDER BY count DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut stats = Vec::new();
    for row in rows {
        let category: String = row.get("category");
        let count: i64 = row.get("count");
        stats.push((category, count));
    }
    Ok(stats)
}

pub(crate) async fn count_things_by_driver(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    let rows = sqlx::query(
        r#"
        SELECT COALESCE(driver_name, 'Unknown') as driver_name, COUNT(*) as count
        FROM things
        GROUP BY driver_name
        ORDER BY count DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut stats = Vec::new();
    for row in rows {
        let driver_name: String = row.get("driver_name");
        let count: i64 = row.get("count");
        stats.push((driver_name, count));
    }
    Ok(stats)
}

// ── 终审修复（F1）：cloud driver/legacy query_service_impl 的 4 处
// QueryBuilder 动态 SQL 迁入本领域（SQL 逐字迁移；返回 DTO 随迁，
// serde 属性保持不变）──

/// 设备状态分布（cloud device dashboard 用，自 cloud driver/legacy/types 迁入）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThingStatusDistribution {
    /// 在线设备数
    pub online: i64,
    /// 离线设备数
    pub offline: i64,
    /// 故障设备数
    pub error: i64,
    /// 维护中设备数
    pub maintenance: i64,
}

/// 关键设备信息（cloud device dashboard 用，自 cloud driver/legacy/types 迁入）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickThing {
    /// 设备ID
    pub id: String,
    /// 设备名称
    pub name: String,
    /// 设备状态
    pub status: String,
    /// 最后在线时间
    pub last_seen: chrono::DateTime<chrono::Utc>,
    /// 设备类型
    pub category: String,
}

pub(crate) async fn search_things(pool: &SqlitePool, keyword: &str, limit: Option<u32>) -> Result<Vec<Thing>> {
    let search_pattern = format!("%{}%", keyword);
    let exact_pattern = format!("{}%", keyword);

    let mut builder = QueryBuilder::new("SELECT ");
    builder.push(thing_row_mapper::SELECT_COLUMNS);
    builder.push(
        " FROM things WHERE name LIKE ? OR display_name LIKE ? OR address LIKE ? OR description LIKE ?
             ORDER BY CASE
                WHEN name LIKE ? THEN 1
                WHEN display_name LIKE ? THEN 2
                WHEN address LIKE ? THEN 3
                ELSE 4
             END, name",
    );

    builder.push_bind(&search_pattern);
    builder.push_bind(&search_pattern);
    builder.push_bind(&search_pattern);
    builder.push_bind(&search_pattern);
    builder.push_bind(&exact_pattern);
    builder.push_bind(&exact_pattern);
    builder.push_bind(&exact_pattern);

    if let Some(limit) = limit {
        builder.push(" LIMIT ").push_bind(limit as i64);
    }

    let rows = builder.build().fetch_all(pool).await?;
    let mut devices = Vec::new();
    for row in rows {
        devices.push(thing_row_mapper::row_to_thing(row)?);
    }
    Ok(devices)
}

pub(crate) async fn thing_tree(pool: &SqlitePool, root_id: Option<&str>) -> Result<Vec<Thing>> {
    let mut builder = QueryBuilder::new("SELECT ");
    builder.push(thing_row_mapper::SELECT_COLUMNS);
    builder.push(" FROM things WHERE ");

    if let Some(root_id) = root_id {
        builder.push("parent_id = ").push_bind(root_id);
    } else {
        builder.push("parent_id IS NULL");
    }

    builder.push(" ORDER BY name");

    let rows = builder.build().fetch_all(pool).await?;
    let mut devices = Vec::new();
    for row in rows {
        devices.push(thing_row_mapper::row_to_thing(row)?);
    }
    Ok(devices)
}

pub(crate) async fn thing_status_distribution(
    pool: &SqlitePool,
    workspace_id: Option<&str>,
) -> Result<ThingStatusDistribution> {
    let mut builder = QueryBuilder::new(
        "SELECT
                SUM(CASE WHEN state = 1 THEN 1 ELSE 0 END) as online,
                SUM(CASE WHEN state = 0 THEN 1 ELSE 0 END) as offline,
                SUM(CASE WHEN state < 0 THEN 1 ELSE 0 END) as error_count,
                SUM(CASE WHEN state = 2 THEN 1 ELSE 0 END) as maintenance
            FROM things",
    );

    if let Some(wid) = workspace_id {
        builder.push(" WHERE workspace_id = ").push_bind(wid);
    }

    let row = builder.build().fetch_one(pool).await?;

    Ok(ThingStatusDistribution {
        online: row.get("online"),
        offline: row.get("offline"),
        error: row.get("error_count"),
        maintenance: row.get("maintenance"),
    })
}

pub(crate) async fn quick_things(pool: &SqlitePool, limit: i32, workspace_id: Option<&str>) -> Result<Vec<QuickThing>> {
    let mut builder = QueryBuilder::new("SELECT id, name, category, state, updated_at FROM things");

    if let Some(wid) = workspace_id {
        builder.push(" WHERE workspace_id = ").push_bind(wid);
    }

    builder.push(
        " ORDER BY
                CASE
                    WHEN state = 1 THEN 0
                    WHEN state = 0 THEN 1
                    WHEN state < 0 THEN 2
                    ELSE 3
                END,
                updated_at DESC
            LIMIT ",
    );
    builder.push_bind(limit);

    let devices: Vec<(String, String, Option<String>, i32, chrono::NaiveDateTime)> =
        builder.build_query_as().fetch_all(pool).await?;

    let quick_things = devices
        .into_iter()
        .map(|(id, name, category, state, updated_at)| {
            let status = match state {
                1 => "online",
                0 => "offline",
                2 => "maintenance",
                _ => "error",
            };

            QuickThing {
                id,
                name,
                status: status.to_string(),
                last_seen: updated_at.and_utc(),
                category: category.unwrap_or_else(|| "unknown".to_string()),
            }
        })
        .collect();

    Ok(quick_things)
}

impl Db {
    /// 按 ID 查设备；`workspace_scope` 为 Some 时先校验归属（不属于返回 None）。
    pub async fn find_thing_by_id(&self, workspace_scope: Option<&str>, id: &str) -> Result<Option<Thing>> {
        find_thing_by_id(self.pool(), workspace_scope, id).await
    }

    /// 按名称查设备；Some(scope) 时限定该 workspace。
    pub async fn find_thing_by_name(&self, workspace_scope: Option<&str>, name: &str) -> Result<Option<Thing>> {
        find_thing_by_name(self.pool(), workspace_scope, name).await
    }

    /// 按条件列出设备；Some(scope) 时强制 workspace_id = scope。
    pub async fn find_things(&self, workspace_scope: Option<&str>, criteria: &ThingCriteria) -> Result<Vec<Thing>> {
        find_things(self.pool(), workspace_scope, criteria).await
    }

    /// 按条件统计设备数；Some(scope) 时强制 workspace_id = scope。
    pub async fn count_things(&self, workspace_scope: Option<&str>, criteria: &ThingCriteria) -> Result<i64> {
        count_things(self.pool(), workspace_scope, criteria).await
    }

    /// 创建设备并回读；Some(scope) 时 workspace_id 取 scope（忽略请求值）。
    pub async fn create_thing(&self, workspace_scope: Option<&str>, request: &CreateThingRequest) -> Result<Thing> {
        create_thing(self.pool(), workspace_scope, request).await
    }

    /// 更新设备并回读（内部事务）；Some(scope) 时先校验归属，不存在返回 NotFound。
    pub async fn update_thing(
        &self,
        workspace_scope: Option<&str>,
        id: &str,
        request: &UpdateThingRequest,
    ) -> Result<Thing> {
        update_thing(self.pool(), workspace_scope, id, request).await
    }

    /// 删除设备，返回影响行数；Some(scope) 时不属于该 workspace 返回 0。
    pub async fn delete_thing(&self, workspace_scope: Option<&str>, id: &str) -> Result<u64> {
        delete_thing(self.pool(), workspace_scope, id).await
    }

    /// 批量删除（内部事务）；Some(scope) 时先按 workspace 过滤 ID。
    pub async fn delete_things_by_ids(&self, workspace_scope: Option<&str>, ids: &[String]) -> Result<u64> {
        delete_things_by_ids(self.pool(), workspace_scope, ids).await
    }

    /// 批量创建（内部事务）；Some(scope) 时 workspace_id 取 scope 并回读。
    pub async fn create_things_batch(
        &self,
        workspace_scope: Option<&str>,
        requests: &[CreateThingRequest],
    ) -> Result<Vec<Thing>> {
        create_things_batch(self.pool(), workspace_scope, requests).await
    }

    /// 更新单设备状态；Some(scope) 时设备不存在返回 InvalidArgument。
    pub async fn update_thing_state(&self, workspace_scope: Option<&str>, id: &str, state: i32) -> Result<()> {
        update_thing_state(self.pool(), workspace_scope, id, state).await
    }

    /// 批量更新状态（内部事务）；Some(scope) 时先按 workspace 过滤。
    pub async fn update_thing_states_batch(
        &self,
        workspace_scope: Option<&str>,
        updates: &[(String, i32)],
    ) -> Result<u64> {
        update_thing_states_batch(self.pool(), workspace_scope, updates).await
    }

    /// 启用/禁用设备（state 1/0）；Some(scope) 时设备不存在返回 InvalidArgument。
    pub async fn update_thing_enabled_status(
        &self,
        workspace_scope: Option<&str>,
        id: &str,
        enabled: bool,
    ) -> Result<bool> {
        update_thing_enabled_status(self.pool(), workspace_scope, id, enabled).await
    }

    /// 列出子设备；Some(scope) 时父设备不属于该 workspace 返回空。
    pub async fn find_thing_children(&self, workspace_scope: Option<&str>, parent_id: &str) -> Result<Vec<Thing>> {
        find_thing_children(self.pool(), workspace_scope, parent_id).await
    }

    /// 按模板 ID 列出设备；Some(scope) 时限定 workspace。
    pub async fn find_things_by_template_id(
        &self,
        workspace_scope: Option<&str>,
        template_id: &str,
    ) -> Result<Vec<Thing>> {
        find_things_by_template_id(self.pool(), workspace_scope, template_id).await
    }

    /// 按驱动名列出设备；Some(scope) 时限定 workspace。
    pub async fn find_things_by_driver_name(
        &self,
        workspace_scope: Option<&str>,
        driver_name: &str,
    ) -> Result<Vec<Thing>> {
        find_things_by_driver_name(self.pool(), workspace_scope, driver_name).await
    }

    /// 按关联网关列出设备；Some(scope) 时限定 workspace。
    pub async fn find_things_by_linked_gateway(
        &self,
        workspace_scope: Option<&str>,
        linked_gateway: &str,
    ) -> Result<Vec<Thing>> {
        find_things_by_linked_gateway(self.pool(), workspace_scope, linked_gateway).await
    }

    /// 按 id 检查设备是否存在。
    pub async fn thing_exists_by_id(&self, id: &str) -> Result<bool> {
        thing_exists_by_id(self.pool(), id).await
    }

    /// 设备名是否已存在；Some(scope) 时限定 workspace 内判断。
    pub async fn thing_exists_by_name(&self, workspace_scope: Option<&str>, name: &str) -> Result<bool> {
        thing_exists_by_name(self.pool(), workspace_scope, name).await
    }

    /// 按 ID 列表查设备；Some(scope) 时先按 workspace 过滤 ID。
    pub async fn find_things_by_ids(&self, workspace_scope: Option<&str>, ids: &[String]) -> Result<Vec<Thing>> {
        find_things_by_ids(self.pool(), workspace_scope, ids).await
    }

    /// 启用状态 + 搜索文本的分页查询；Some(scope) 时限定 workspace。
    pub async fn find_things_with_filters(
        &self,
        workspace_scope: Option<&str>,
        enabled: Option<bool>,
        search: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Thing>> {
        find_things_with_filters(self.pool(), workspace_scope, enabled, search, page, page_size).await
    }

    /// 批量更新设备状态（含 updated_at，内部事务）；Some(scope) 时先按 workspace 过滤。
    pub async fn update_thing_status_batch(
        &self,
        workspace_scope: Option<&str>,
        updates: &[ThingStatusUpdate],
    ) -> Result<u64> {
        update_thing_status_batch(self.pool(), workspace_scope, updates).await
    }

    /// 工作空间下的设备数（workspace 删除守卫用）。
    pub async fn count_things_by_workspace(&self, workspace_id: &str) -> Result<i64> {
        count_things_by_workspace(self.pool(), workspace_id).await
    }

    /// 设备总数/在线/离线/告警统计（cloud dashboard 用）。
    pub async fn thing_stats_overview(&self) -> Result<ThingStats> {
        thing_stats_overview(self.pool()).await
    }

    /// 按设备类型分组计数（cloud dashboard 用）。
    pub async fn count_things_by_type(&self) -> Result<Vec<(String, i64)>> {
        count_things_by_type(self.pool()).await
    }

    /// 按驱动名分组计数（cloud dashboard 用）。
    pub async fn count_things_by_driver(&self) -> Result<Vec<(String, i64)>> {
        count_things_by_driver(self.pool()).await
    }

    /// 关键字搜索设备（cloud device dashboard 用）。
    pub async fn search_things(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Thing>> {
        search_things(self.pool(), keyword, limit).await
    }

    /// 设备树（按 parent_id 取一层；cloud device dashboard 用）。
    pub async fn thing_tree(&self, root_id: Option<&str>) -> Result<Vec<Thing>> {
        thing_tree(self.pool(), root_id).await
    }

    /// 设备状态分布（cloud device dashboard 用）。
    pub async fn thing_status_distribution(&self, workspace_id: Option<&str>) -> Result<ThingStatusDistribution> {
        thing_status_distribution(self.pool(), workspace_id).await
    }

    /// 关键设备列表（cloud device dashboard 用）。
    pub async fn quick_things(&self, limit: i32, workspace_id: Option<&str>) -> Result<Vec<QuickThing>> {
        quick_things(self.pool(), limit, workspace_id).await
    }
}

// ──────────────────────────────────────────────
// Open API 投影查询（自 cloud admin/open 迁入，Task 12）
// ──────────────────────────────────────────────

/// Open API thing 列表行。
#[derive(Debug)]
pub struct OpenThingRow {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub category: Option<String>,
    pub state: i32,
    pub created_at: String,
}

/// Open API thing 详情行。
#[derive(Debug)]
pub struct OpenThingDetailRow {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub category: Option<String>,
    pub address: Option<String>,
    pub state: i32,
    pub protocol_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Open API：列出 workspace 内 things（最新 100 条）。
pub(crate) async fn list_open_things(pool: &SqlitePool, workspace_id: &str) -> Result<Vec<OpenThingRow>> {
    let rows = sqlx::query(
        "SELECT id, name, display_name, category, state, created_at FROM things WHERE workspace_id = ? ORDER BY created_at DESC LIMIT 100",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| OpenThingRow {
            id: row.try_get::<String, _>("id").unwrap_or_default(),
            name: row.try_get::<String, _>("name").unwrap_or_default(),
            display_name: row.try_get::<Option<String>, _>("display_name").unwrap_or_default(),
            category: row.try_get::<Option<String>, _>("category").unwrap_or_default(),
            state: row.try_get::<i32, _>("state").unwrap_or_default(),
            created_at: row.try_get::<String, _>("created_at").unwrap_or_default(),
        })
        .collect())
}

/// Open API：按 id + workspace 查 thing 详情。
pub(crate) async fn find_open_thing(
    pool: &SqlitePool,
    id: &str,
    workspace_id: &str,
) -> Result<Option<OpenThingDetailRow>> {
    let row = sqlx::query(
        "SELECT id, name, display_name, category, address, state, protocol_type, created_at, updated_at FROM things WHERE id = ? AND workspace_id = ? LIMIT 1"
    )
    .bind(id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| OpenThingDetailRow {
        id: row.try_get::<String, _>("id").unwrap_or_default(),
        name: row.try_get::<String, _>("name").unwrap_or_default(),
        display_name: row.try_get::<Option<String>, _>("display_name").unwrap_or_default(),
        category: row.try_get::<Option<String>, _>("category").unwrap_or_default(),
        address: row.try_get::<Option<String>, _>("address").unwrap_or_default(),
        state: row.try_get::<i32, _>("state").unwrap_or_default(),
        protocol_type: row.try_get::<Option<String>, _>("protocol_type").unwrap_or_default(),
        created_at: row.try_get::<String, _>("created_at").unwrap_or_default(),
        updated_at: row.try_get::<String, _>("updated_at").unwrap_or_default(),
    }))
}

/// Open API：查 (id, thing_type)（workspace 作用域）。
pub(crate) async fn find_open_thing_type(
    pool: &SqlitePool,
    id: &str,
    workspace_id: &str,
) -> Result<Option<(String, String)>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT id, thing_type FROM things WHERE id = ? AND workspace_id = ?")
            .bind(id)
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

impl Db {
    /// Open API：列出 workspace 内 things（最新 100 条）。
    pub async fn list_open_things(&self, workspace_id: &str) -> Result<Vec<OpenThingRow>> {
        list_open_things(self.pool(), workspace_id).await
    }

    /// Open API：按 id + workspace 查 thing 详情。
    pub async fn find_open_thing(&self, id: &str, workspace_id: &str) -> Result<Option<OpenThingDetailRow>> {
        find_open_thing(self.pool(), id, workspace_id).await
    }

    /// Open API：查 (id, thing_type)（workspace 作用域）。
    pub async fn find_open_thing_type(&self, id: &str, workspace_id: &str) -> Result<Option<(String, String)>> {
        find_open_thing_type(self.pool(), id, workspace_id).await
    }
}

// ──────────────────────────────────────────────
// Dashboard 统计（自 cloud admin/monitoring 迁入，Task 12）
// ──────────────────────────────────────────────

/// Dashboard：设备总数（可选 workspace 过滤）。
pub(crate) async fn count_things_total(pool: &SqlitePool, workspace_id: Option<&str>) -> Result<i64> {
    let (query_str, wid) = match workspace_id {
        Some(wid) => ("SELECT COUNT(*) FROM things WHERE workspace_id = ?", Some(wid)),
        None => ("SELECT COUNT(*) FROM things", None),
    };
    let mut q = sqlx::query_scalar(sqlx::AssertSqlSafe(query_str));
    if let Some(w) = wid {
        q = q.bind(w);
    }
    let count: i64 = q.fetch_one(pool).await?;
    Ok(count)
}

/// Dashboard：在线设备数（可选 workspace 过滤）。
pub(crate) async fn count_online_things(pool: &SqlitePool, workspace_id: Option<&str>) -> Result<i64> {
    let (query_str, wid) = match workspace_id {
        Some(wid) => (
            "SELECT COUNT(*) FROM things WHERE state = 1 AND workspace_id = ?",
            Some(wid),
        ),
        None => ("SELECT COUNT(*) FROM things WHERE state = 1", None),
    };
    let mut q = sqlx::query_scalar(sqlx::AssertSqlSafe(query_str));
    if let Some(w) = wid {
        q = q.bind(w);
    }
    let count: i64 = q.fetch_one(pool).await?;
    Ok(count)
}

impl Db {
    /// Dashboard：设备总数（可选 workspace 过滤）。
    pub async fn count_things_total(&self, workspace_id: Option<&str>) -> Result<i64> {
        count_things_total(self.pool(), workspace_id).await
    }

    /// Dashboard：在线设备数（可选 workspace 过滤）。
    pub async fn count_online_things(&self, workspace_id: Option<&str>) -> Result<i64> {
        count_online_things(self.pool(), workspace_id).await
    }
}

// ──────────────────────────────────────────────
// 初始化引导：孤儿设备归属（自 cloud shared/initialization.rs 迁入，Task 12；
// 错误保持 sqlx::Error 以沿用调用方既有错误文案）
// ──────────────────────────────────────────────

/// 将未分配设备归属到默认租户。
pub(crate) async fn assign_orphan_things_to_default_tenant(pool: &SqlitePool) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("UPDATE things SET tenant_id = 'tenant-default-001' WHERE tenant_id IS NULL")
        .execute(pool)
        .await?;
    Ok(())
}

/// 将默认租户下未分配设备归属到默认工作空间。
pub(crate) async fn assign_orphan_things_to_default_workspace(
    pool: &SqlitePool,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE things SET workspace_id = 'ws-default-001' WHERE workspace_id IS NULL AND tenant_id = 'tenant-default-001'"
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    /// 将未分配设备归属到默认租户。
    pub async fn assign_orphan_things_to_default_tenant(&self) -> std::result::Result<(), sqlx::Error> {
        assign_orphan_things_to_default_tenant(self.pool()).await
    }

    /// 将默认租户下未分配设备归属到默认工作空间。
    pub async fn assign_orphan_things_to_default_workspace(&self) -> std::result::Result<(), sqlx::Error> {
        assign_orphan_things_to_default_workspace(self.pool()).await
    }
}

/// 确保 devices 表存在（edge 网关本地库不走 migrations，Task 13 自 edge storage 收编）。
pub(crate) async fn ensure_things_table(pool: &SqlitePool) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS things (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            display_name TEXT,
            category TEXT,
            address TEXT,
            description TEXT,
            position TEXT,
            driver_name TEXT,
            device_model TEXT,
            protocol_type TEXT,
            factory_name TEXT,
            linked_data TEXT,
            driver_options TEXT,
            state INTEGER NOT NULL DEFAULT 0,
            parent_id TEXT,
            template_id TEXT,
            workspace_id TEXT,
            linked_gateway TEXT,
            fingerprint TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    /// 确保 devices 表存在（edge 本地库 bootstrap 用）。
    pub async fn ensure_things_table(&self) -> std::result::Result<(), sqlx::Error> {
        ensure_things_table(self.pool()).await
    }
}
