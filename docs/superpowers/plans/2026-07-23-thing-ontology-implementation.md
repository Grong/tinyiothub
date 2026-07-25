# Thing Ontology 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 设备→物本体中心化重构：新增 thing 模块，API 一刀切 /api/things，Agent 9 工具，事件体系统一，图谱拆除，前端 T18-T21，E1/E2/E3 扩展。

**Architecture:** devices 表原地泛化为 Thing（加 thing_type/ontology_summary/summary_status + parent_id FK→RESTRICT + product_id→template_id），新建 `cloud/src/modules/thing` 模块（handler/service/repo/types 分层）承载管理面，device 模块保留连接运行时。迁移阶段 `PRAGMA defer_foreign_keys=ON` 下 6 组 SQLite 表重建。

**Tech Stack:** Rust + Axum + Tokio + SQLite (sqlx) + 手写 TS 视图 (home.css tokens + glassmorphic 暗色风格)

**Spec:** `docs/superpowers/specs/2026-07-22-thing-ontology-design.md` (CEO+ENG+DESIGN CLEARED, v6)

---

## Phase 1 — Foundation (迁移 + Thing 模块 + 拆除 + products)

**依赖链:** T1 (migration) → T2 (thing module) & T7 (teardown) & T8 (products) 可并行
**验证门:** `cargo test integration` 全绿 + `/api/devices` 404 + `/api/things` CRUD 可用

### Task 1.1: SQLite 表重建迁移

**Files:**
- Create: `cloud/migrations/20260723000001_thing_ontology_rebuild.sql`
- Modify: `cloud/src/shared/persistence/repositories/event_repository_impl.rs`
- Modify: `cloud/src/modules/event/handler/real_time.rs`
- Modify: `cloud/src/shared/event/handlers/real_time_status_handler.rs`

> 此迁移是整条 mega-branch 的基础。6 组表重建在一个迁移文件中编排（共享 defer_foreign_keys 上下文）。

- [ ] **Step 1: 写迁移 SQL**

参照 spec 七，详列 6 组重建的顺序、新表 DDL、拷贝 INSERT、旧表 DROP、RENAME、索引重建。要点：

```
-- 0a. 删 products 表 (DROP TABLE IF EXISTS products;)
-- 0b. device_templates → thing_templates (新表含 thing_type/actions/events/default_knowledge 列; commands→actions 拷贝; COALESCE 表达式唯一索引)
-- 1. devices 泛化 (最重建: 新表 + thing_type/ontology_summary/summary_status/template_id; 12 索引重; 8 内向 FK 延迟校验; name 表达式唯一索引)
-- 2. tags CHECK 放宽 (type IN('device','app','thing'); 唯一约束改表达式索引)
-- 3. resources → thing_resources (workspace_id NOT NULL, device_id NULL, 删 parse_status 列)
-- 4. events 体系统一 (删 cleanup_old_events 触发器; 加 occurrence_count/acknowledged/acknowledged_by/acknowledged_at/workspace_id NOT NULL; source 去重表达式索引)
-- 5. DROP TABLE IF EXISTS real_time_events; DROP TABLE IF EXISTS lost_events; DROP TABLE IF EXISTS event_performance_metrics;
-- 6. workspaces 加 require_action_confirm BOOLEAN DEFAULT 1
-- 7. device_alarm_rules CHECK 放宽 rule_type (支持 'event')
-- 8. PRAGMA foreign_key_check 在 commit 前执行
```

每个表重建精确 SQL 格式：

```sql
-- 模式: 建新表 → 拷数据 → 删旧表 → 改名 → 建索引 → 恢复 FK
CREATE TABLE <table>_new (...<完整列>, 无 FK 内联声明);  -- FK 在 ALTER 阶段加
INSERT INTO <table>_new SELECT ... FROM <table>;           -- 列映射精确对齐
DROP TABLE <table>;
ALTER TABLE <table>_new RENAME TO <table>;
CREATE INDEX ...;
-- 必要时: ALTER TABLE <table> ADD FOREIGN KEY ...
```

- [ ] **Step 2: backfill template_id 脚本**

在 migration 中或独立 SQL 文件完成 8 设备 product_id→template_id 重映射。规则：按 device_type 匹配 thing_templates.name（如"环境传感器"→"SHT30 温湿度传感器模板"等内置模板名），无匹配列 NULL。如果作为独立 SQL 脚本，放在 `cloud/migrations/20260723000002_backfill_template_id.sql`。

```sql
-- 示例: 环境传感器 → SHT30 模板
UPDATE devices SET template_id = (
    SELECT id FROM thing_templates
    WHERE name LIKE '%SHT30%' AND (workspace_id IS NULL OR workspace_id = devices.workspace_id)
    LIMIT 1
) WHERE device_type = 'environment_sensor' AND template_id IS NULL;
-- ... 其余 device_type 映射
```

- [ ] **Step 3: 集成测试 — 迁移全链路**

在 `cloud/src/tests/` 下新建迁移集成测试或扩展现有 migration tests:

```rust
// tests/migration_thing_ontology.rs (使用 #[sqlx::test] 宏)
#[sqlx::test]
async fn test_thing_ontology_migration_applied(pool: SqlitePool) {
    // 验证迁移后 schema 状态
    // Assert: products 表不存在
    // Assert: thing_templates 表存在，name 有 COALESCE 表达式唯一索引
    // Assert: devices 表含 thing_type/ontology_summary/summary_status/template_id 列
    // Assert: devices.name 不再全局 UNIQUE (仅通过 idx_devices_name_ws)
    // Assert: thing_resources 表存在，device_id 可空
    // Assert: events 表含 occurrence_count/acknowledged/workspace_id 列
    // Assert: real_time_events/lost_events/event_performance_metrics 表不存在
    // Assert: workspaces 表含 require_action_confirm 列，默认值 1
    // Assert: tags CHECK 约束包含 'thing'
}

#[sqlx::test]
async fn test_name_conflict_across_workspaces(pool: SqlitePool) {
    // 同工作区同名 → 冲突
    // 跨工作区同名 → OK
}

#[sqlx::test]
async fn test_parent_id_restrict_delete(pool: SqlitePool) {
    // 插入父子物 → 删父 → 预期 FK RESTRICT 拒绝
}

#[sqlx::test]
async fn test_require_action_confirm_default(pool: SqlitePool) {
    // 新建 workspace → require_action_confirm = true (BOOLEAN 1)
}
```

- [ ] **Step 4: 修改 ack API 写 events 表**

修改 `cloud/src/modules/event/handler/real_time.rs` — acknowledge 端点不再读写 real_time_events，改为 UPDATE events SET acknowledged=1, acknowledged_by=?, acknowledged_at=? WHERE ...。

修改 `cloud/src/shared/event/handlers/real_time_status_handler.rs` — upsert_status 不再写入 real_time_events，改为 INSERT INTO events ... ON CONFLICT(source去重维度) DO UPDATE。

- [ ] **Step 5: 验证迁移**

Run: `cargo test --test integration -- migration_thing_ontology -- --nocapture`
Expected: 全部 PASS。迁移集成测试 + 既有测试不退化。

- [ ] **Step 6: Commit**

```bash
git add cloud/migrations/ cloud/src/modules/event/handler/real_time.rs \
    cloud/src/shared/event/handlers/real_time_status_handler.rs \
    cloud/src/tests/
git commit -m "feat(migration): SQLite table rebuild for Thing Ontology

- devices: add thing_type/ontology_summary/summary_status/template_id, name unique → COALESCE expression index, parent_id FK→RESTRICT
- device_templates→thing_templates: commands→actions rename, name unique → expression index
- tags: CHECK +'thing', unique→expression index
- resources→thing_resources: workspace_id NOT NULL, device_id nullable
- events: add occurrence_count/acknowledged*/workspace_id, drop cleanup trigger, source upsert index
- Delete: real_time_events, lost_events, event_performance_metrics (all 0 rows verified)
- Delete: products table (6 rows, hollow model)
- workspaces: add require_action_confirm BOOLEAN DEFAULT 1
- Ack API & real_time_status_handler repointed to events table
- Integration test: schema verification + name conflict + RESTRICT + default"
```

---

### Task 1.2: Thing 模块核心

**Files:**
- Create: `cloud/src/modules/thing/mod.rs`
- Create: `cloud/src/modules/thing/types.rs`
- Create: `cloud/src/modules/thing/repo.rs`
- Create: `cloud/src/modules/thing/service.rs`
- Create: `cloud/src/modules/thing/handler/mod.rs`
- Create: `cloud/src/modules/thing/handler/crud.rs`
- Create: `cloud/src/modules/thing/handler/ontology.rs`
- Create: `cloud/src/modules/thing/handler/resources.rs`
- Create: `cloud/src/modules/thing/errors.rs`
- Modify: `cloud/src/modules/mod.rs` (register thing module)

> thing 模块是管理面核心——物 CRUD、层级树、本体聚合、资源挂载。遵循现有 device/event 模块的 handler/service/repo/types 分层模式。

- [ ] **Step 1: 定义 types.rs**

```rust
use serde::{Deserialize, Serialize};

// 物类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ThingType {
    Device,
    Space,
    Line,
    Building,
    // 扩展自由文本 catch-all
    Custom(String),
}

impl ThingType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Device => "device",
            Self::Space => "space",
            Self::Line => "line",
            Self::Building => "building",
            Self::Custom(s) => s.as_str(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "device" => Self::Device,
            "space" => Self::Space,
            "line" => Self::Line,
            "building" => Self::Building,
            other => Self::Custom(other.to_string()),
        }
    }
}

// 摘要状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SummaryStatus {
    Ok,
    Dirty,
    Failed,
}

// 物列表查询参数
#[derive(Debug, Clone, Deserialize)]
pub struct ListThingsParams {
    pub thing_type: Option<String>,
    pub parent_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// Thing 详情 DTO
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingResponse {
    pub id: String,
    pub workspace_id: Option<String>,
    pub name: String,
    pub device_type: Option<String>,
    pub thing_type: String,
    pub parent_id: Option<String>,
    pub template_id: Option<String>,
    pub state: i32,
    pub driver_name: Option<String>,
    pub protocol_type: Option<String>,
    pub ontology_summary: Option<String>,
    pub summary_status: Option<String>,
    pub breadcrumb: Vec<BreadcrumbNode>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BreadcrumbNode {
    pub id: String,
    pub name: String,
    pub thing_type: String,
}

// 物创建/更新请求
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateThingRequest {
    pub name: String,
    pub workspace_id: Option<String>,
    pub thing_type: Option<String>,
    pub parent_id: Option<String>,
    pub template_id: Option<String>,
    pub device_type: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateThingRequest {
    pub name: Option<String>,
    pub parent_id: Option<String>,
    pub thing_type: Option<String>,
    pub template_id: Option<String>,
    pub device_type: Option<String>,
    pub tags: Option<Vec<String>>,
}

// 物树节点
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingTreeNode {
    pub id: String,
    pub name: String,
    pub thing_type: String,
    pub children: Vec<ThingTreeNode>,
}
```

- [ ] **Step 2: 定义 errors.rs**

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ThingError {
    #[error("物不存在: {0}")]
    NotFound(String),
    #[error("名称已被占用: {0}")]
    NameConflict(String),
    #[error("层级成环: {0} → {1} 将形成循环引用")]
    CycleDetected(String, String),
    #[error("该物有 {0} 个子物，无法删除")]
    HasChildren(usize),
    #[error("该物不支持动作")]
    ActionNotSupported,
    #[error("数据库错误: {0}")]
    Database(String),
    #[error("工作区不存在: {0}")]
    WorkspaceNotFound(String),
}

impl IntoResponse for ThingError {
    fn into_response(self) -> Response {
        let (status, code, msg) = match &self {
            Self::NotFound(id) => (StatusCode::NOT_FOUND, "THING_NOT_FOUND", format!("Thing {} not found", id)),
            Self::NameConflict(name) => (StatusCode::CONFLICT, "NAME_CONFLICT", format!("Name '{}' already taken", name)),
            Self::CycleDetected(from, to) => (StatusCode::CONFLICT, "CYCLE_DETECTED", format!("{} → {} would form a cycle", from, to)),
            Self::HasChildren(n) => (StatusCode::CONFLICT, "HAS_CHILDREN", format!("Cannot delete: has {} children", n)),
            Self::ActionNotSupported => (StatusCode::BAD_REQUEST, "ACTION_NOT_SUPPORTED", "Thing does not support actions".into()),
            Self::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR", self.to_string()),
            Self::WorkspaceNotFound(ws) => (StatusCode::NOT_FOUND, "WORKSPACE_NOT_FOUND", format!("Workspace {} not found", ws)),
        };
        (status, Json(json!({"error": {"code": code, "message": msg}}))).into_response()
    }
}

impl From<sqlx::Error> for ThingError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.to_string())
    }
}
```

- [ ] **Step 3: 写 repo.rs**

```rust
use sqlx::SqlitePool;
use crate::modules::thing::types::*;
use crate::modules::thing::errors::ThingError;

pub struct ThingRepo {
    pool: SqlitePool,
}

impl ThingRepo {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }

    // workspace 作用域按名查找（find_by_name 全部 workspace 作用域化）
    pub async fn find_by_name(&self, workspace_id: &str, name: &str) -> Result<Option<ThingRow>, ThingError> {
        let row = sqlx::query_as::<_, ThingRow>(
            "SELECT * FROM devices WHERE COALESCE(workspace_id,'') = ? AND name = ?"
        )
        .bind(workspace_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    // 列表查询 (thing_type 可选; parent_id 可选; tags 可选; q 名称搜索; 分页)
    pub async fn list(&self, workspace_id: &str, params: &ListThingsParams) -> Result<(Vec<ThingRow>, i64), ThingError> {
        let limit = params.limit.unwrap_or(50).min(200);
        let offset = params.offset.unwrap_or(0);
        // 动态 SQL: WHERE workspace_id=? [AND thing_type=?] [AND parent_id=?] [AND name LIKE ?]
        // JOIN thing_resources 算知识挂载数 (用于列表"知识"列徽标)
        // 返回 (rows, total_count)
        todo!() // 实现时根据 params 动态拼接
    }

    // 单个物（含父链递归上溯做 breadcrumb，深度上限 10 兜底）
    pub async fn get_by_id(&self, id: &str) -> Result<Option<ThingRow>, ThingError> {
        sqlx::query_as::<_, ThingRow>("SELECT * FROM devices WHERE id = ?")
            .bind(id).fetch_optional(&self.pool).await.map_err(Into::into)
    }

    // 创建（INSERT 前做 name 冲突 / parent_id FK / 成环检查）
    pub async fn create(&self, row: &CreateThingRow) -> Result<String, ThingError> { todo!() }

    // 更新（含成环检查：改 parent_id 时确保新父不是自己或自己的子节点）
    pub async fn update(&self, id: &str, row: &UpdateThingRow) -> Result<(), ThingError> { todo!() }

    // 删除（先查子物数量 > 0 → HasChildren 拒绝，否则 DELETE）
    pub async fn delete(&self, id: &str) -> Result<(), ThingError> { todo!() }

    // 子树查询 (递归 CTE 上溯/下钻，默认深度 3，get_thing_tree 工具用)
    pub async fn get_tree(&self, root_id: Option<&str>, workspace_id: &str, max_depth: i32) -> Result<Vec<ThingTreeNode>, ThingError> { todo!() }

    // 面包屑 (递归 CTE 沿 parent_id 上溯)
    pub async fn get_breadcrumb(&self, id: &str, max_depth: i32) -> Result<Vec<BreadcrumbNode>, ThingError> { todo!() }

    // 成环检测: 从候选父节点沿 parent_id 递归上溯，检查是否会撞到目标节点自己
    pub async fn check_cycle(&self, thing_id: &str, candidate_parent_id: &str) -> Result<bool, ThingError> { todo!() }

    // 批量标脏 (模板变更/改名/换父 → 子树: UPDATE devices SET summary_status='dirty' WHERE id IN (子树 IDs))
    pub async fn mark_subtree_dirty(&self, root_id: &str) -> Result<u64, ThingError> { todo!() }

    // template_id 重映射（migration 用 — 按 device_type 匹配 thing_templates 内置模板）
    pub async fn backfill_template_ids(&self) -> Result<u64, ThingError> { todo!() }
}

// SQLite 行 → ThingRow (与 devices 新 schema 对齐)
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ThingRow {
    pub id: String,
    pub workspace_id: Option<String>,
    pub organization_id: Option<String>,
    pub parent_id: Option<String>,
    pub template_id: Option<String>,
    pub name: String,
    pub device_type: String,
    pub thing_type: String,
    pub driver_name: Option<String>,
    pub protocol_type: Option<String>,
    pub state: i32,
    pub config: String,
    pub last_heartbeat: Option<String>,
    pub metadata: String,
    pub ontology_summary: Option<String>,
    pub summary_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

- [ ] **Step 4: 写 service.rs**

```rust
use crate::modules::thing::repo::ThingRepo;
use crate::modules::thing::types::*;
use crate::modules::thing::errors::ThingError;

pub struct ThingService {
    repo: ThingRepo,
}

impl ThingService {
    pub fn new(repo: ThingRepo) -> Self { Self { repo } }

    // list_things: 按 workspace/type/parent/tags/q 过滤，分页 50/200 clamp
    // 每条结果附加 breadcrumb 首节点 & 知识挂载数
    pub async fn list_things(&self, workspace_id: &str, params: &ListThingsParams) -> Result<ListThingsResult, ThingError> { todo!() }

    // get_thing: 单物详情 (含 breadcrumb 递归、模板定义)
    pub async fn get_thing(&self, id: &str) -> Result<ThingResponse, ThingError> { todo!() }

    // get_thing_profile: 全快照 (get_thing + 属性实时值 + 最近 10 事件 + 知识列表)
    pub async fn get_thing_profile(&self, id: &str) -> Result<ThingProfileResponse, ThingError> { todo!() }

    // get_thing_tree: 子树（默认 3 层，仅 id/名称/类型，轻量全局视图）
    pub async fn get_thing_tree(&self, workspace_id: &str, root_id: Option<&str>, depth: Option<i32>) -> Result<Vec<ThingTreeNode>, ThingError> { todo!() }

    // create_thing: 验证 name 唯一 + 父节点存在 + workspace 存在 → INSERT
    pub async fn create_thing(&self, req: &CreateThingRequest) -> Result<ThingResponse, ThingError> { todo!() }

    // update_thing: 成环检测 + 改父则标脏子树 → UPDATE
    pub async fn update_thing(&self, id: &str, req: &UpdateThingRequest) -> Result<ThingResponse, ThingError> { todo!() }

    // delete_thing: 查子物 count → 拒绝 or DELETE (events 保留/resource SET NULL/alarm CASCADE)
    pub async fn delete_thing(&self, id: &str) -> Result<(), ThingError> { todo!() }

    // --- 资源挂载 ---
    // 挂文档: INSERT thing_resources (device_id, workspace_id)
    pub async fn attach_resource(&self, thing_id: &str, resource_id: &str) -> Result<(), ThingError> { todo!() }
    // 未指派资源列表: SELECT FROM thing_resources WHERE workspace_id=? AND device_id IS NULL
    pub async fn list_unassigned_resources(&self, workspace_id: &str) -> Result<Vec<ThingResource>, ThingError> { todo!() }

    // --- 摘要触发 (service 内薄包装，不建独立管线) ---
    // mark_dirty: thing resource 增删改时标记 summary_status='dirty'
    // trigger_summary: get_thing/get_thing_profile 调用，dirty→LLM 重算 → 写回 (T6 完成)
}

// 列表返回结构
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListThingsResult {
    pub items: Vec<ThingResponse>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub unassigned_resource_count: i64, // 知识列徽标数据
}
```

- [ ] **Step 5: 写 handler/crud.rs — /api/things CRUD 路由**

```rust
use axum::{Router, routing::{get, post, put, delete}, extract::{Path, Query, State}, Json};
use crate::modules::thing::service::ThingService;
use crate::modules::thing::types::*;

// 路由注册
pub fn thing_routes() -> Router<AppState> {
    Router::new()
        .route("/api/things", get(list_things).post(create_thing))
        .route("/api/things/{id}", get(get_thing).put(update_thing).delete(delete_thing))
        .route("/api/things/{id}/ontology", get(get_thing_ontology))        // 轻量本体 = get_thing
        .route("/api/things/{id}/profile", get(get_thing_profile))          // 全聚合快照
        .route("/api/things/{id}/tree", get(get_thing_tree))                // 子树
}

// list_things handler:
async fn list_things(
    State(state): State<AppState>,
    Extension(workspace): Extension<WorkspaceContext>,
    Query(params): Query<ListThingsParams>,
) -> Result<Json<ListThingsResult>, ThingError> {
    let result = state.thing_service.list_things(&workspace.id, &params).await?;
    Ok(Json(result))
}

// get_thing handler (含面包屑):
async fn get_thing(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ThingResponse>, ThingError> {
    let thing = state.thing_service.get_thing(&id).await?;
    Ok(Json(thing))
}

// create_thing handler:
async fn create_thing(
    State(state): State<AppState>,
    Extension(workspace): Extension<WorkspaceContext>,
    Json(req): Json<CreateThingRequest>,
) -> Result<(StatusCode, Json<ThingResponse>), ThingError> {
    let thing = state.thing_service.create_thing(&req).await?;
    Ok((StatusCode::CREATED, Json(thing)))
}

// ... 其余 update/delete/get_ontology/get_profile/get_tree handlers
```

- [ ] **Step 6: 注册模块**

修改 `cloud/src/modules/mod.rs` — 注册 thing handler 路由，加入 axum Router。

```rust
pub mod thing;

// 在 app router 中:
router = router.merge(thing::handler::crud::thing_routes());
```

- [ ] **Step 7: 集成测试 — Thing CRUD + 层级**

```rust
// tests/thing_crud.rs
#[sqlx::test]
async fn test_create_thing(pool: SqlitePool) {
    // POST /api/things {name:"测试物", thing_type:"device"}
    // Assert: 201 + response.id non-empty + thing_type=device
}

#[sqlx::test]
async fn test_name_conflict_same_workspace(pool: SqlitePool) {
    // 同 workspace 创建同名物 → 409 NAME_CONFLICT
}

#[sqlx::test]
async fn test_name_no_conflict_different_workspace(pool: SqlitePool) {
    // 跨 workspace 相同名称 → 201 OK
}

#[sqlx::test]
async fn test_parent_id_cycle_rejected(pool: SqlitePool) {
    // 创建 A→B→A 层级 → 409 CYCLE_DETECTED
}

#[sqlx::test]
async fn test_delete_with_children_rejected(pool: SqlitePool) {
    // 创建父子，DELETE 父 → 409 HAS_CHILDREN
}

#[sqlx::test]
async fn test_breadcrumb_depth_limit(pool: SqlitePool) {
    // 创建 11 层链，get_thing 深度 10 → breadcrumb 截断 10 层
}

#[sqlx::test]
async fn test_pagination_clamp(pool: SqlitePool) {
    // limit=500 → clamp to 200; limit=0→default 50; offset 越界→空列表
}
```

- [ ] **Step 8: Commit**

---

### Task 1.3: 图谱拆除 + API 改名

**Files:**
- Delete: `cloud/src/modules/workspace/types/knowledge.rs`
- Delete: `cloud/src/modules/workspace/service/knowledge.rs`
- Delete: `cloud/src/modules/workspace/repo/knowledge.rs`
- Delete: `cloud/src/modules/workspace/handler/knowledge.rs`
- Delete: `cloud/src/modules/agent/tools/knowledge.rs`
- Delete: `cloud/src/modules/agent/tools/search_resources.rs`
- Modify: `cloud/src/modules/agent/tools/mod.rs` (remove knowledge & search_resources)
- Modify: `cloud/src/modules/workspace/mod.rs` (remove knowledge submodule)
- Modify: `cloud/src/modules/device/handler/management.rs` (disable old /api/devices routes)
- Modify: `cloud/src/modules/workspace/handler/mod.rs` (disable knowledge routes)

- [ ] **Step 1: 删除知识图谱代码文件**

逐个删除上述 6 个文件。验证编译不报 missing module/import 错误，如果有——更新 mod.rs 移除 pub mod 声明。

- [ ] **Step 2: 禁用 /api/devices** 路由**

在 `cloud/src/modules/device/handler/management.rs` 中，将 `/api/devices` CRUD 路由替换为返回 404 + 迁移提示的 handler：

```rust
// 替代原 device CRUD handler vs 直接删路由：
// 最干净方案: 删除 management.rs 中的 /api/devices 路由注册，
// 新增一个 catch 路由返回 404 + 迁移引导 JSON:
async fn devices_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Json(json!({
        "error": {"code": "ENDPOINT_REMOVED", "message": "/api/devices has been removed. Use /api/things instead."}
    })))
}
```

注意：**运行时数据面路由不动**（遥测 ingest、心跳、网关协议端点保持原样）。

- [ ] **Step 3: 禁用知识图谱 API**

同样给 knowledge handler 路由返回 404 + 引导信息；或直接删除路由注册。

- [ ] **Step 4: 验证编译**

```bash
cargo build 2>&1 | grep -E "error|warning: unused"
```
Expected: 0 errors。残存的 knowledge 引用全部清除。

- [ ] **Step 5: 集成测试 — 拆除验证**

```rust
#[sqlx::test]
async fn test_knowledge_tables_not_exist(pool: SqlitePool) {
    // SELECT name FROM sqlite_master WHERE type='table' AND name IN ('knowledge_entities','knowledge_relations','knowledge_parse_jobs')
    // Assert: 0 rows
}

#[sqlx::test]
async fn test_devices_api_404(pool: SqlitePool) {
    // GET /api/devices → 404
    // GET /api/devices/{id} → 404
    // POST /api/devices → 404
}

#[sqlx::test]
async fn test_knowledge_api_404(pool: SqlitePool) {
    // GET /api/workspaces/{ws}/knowledge/entities → 404
}
```

- [ ] **Step 6: Commit**

---

### Task 1.4: Products 收敛

**Files:**
- 迁移已在 Task 1.1 中完成 DROP TABLE products + devices.template_id 列
- Modify: `cloud/src/modules/device/types.rs` (若引用 product_id)

- [ ] **Step 1: 移除 product_id 代码引用**

搜索全局 `product_id` 引用（Rust 代码、不搜 migration SQL），移除或更新为 template_id：

```bash
rg "product_id" --type rust cloud/src/
```

对于 device types/handler 中的 product_id 字段 → 改为 template_id；对 products 表查询 → 删除。

- [ ] **Step 2: 验证 template_id 重映射逻辑**

手检或单元测试验证 8 设备 device_type→template_id 映射结果：
- 环境传感器 (SHT30) → template_id 有值
- 无法匹配的设备类型 → template_id IS NULL（合法状态）

- [ ] **Step 3: Verify**

```bash
cargo build && cargo test --test integration -- products
```

- [ ] **Step 4: Commit**

---

### Phase 1 验证门

```bash
# 全量集成测试必须绿
cargo test --test integration -- --nocapture
# 验证端点:
#  POST /api/things → 201
#  GET  /api/things → 列表
#  GET  /api/devices → 404
#  GET  /api/workspaces/{ws}/knowledge/entities → 404
```

---

## Phase 2 — Event & Alarm & Summary (事件管线 + 告警 + 摘要)

**依赖:** Phase 1 complete
**验证门:** 真实 MQTT 上报 → events 表落库 → alarm rule_type='event' 触发 → 通知送达

### Task 2.1: 事件路由

**Files:**
- Create: `cloud/src/modules/event/router.rs` (新事件路由函数)
- Modify: `cloud/src/shared/mqtt_client.rs` (增加 `thing/+/event/+` 订阅)
- Modify: `cloud/src/modules/event/handler/` (事件 HTTP handler)

- [ ] **Step 1: 新增 MQTT 订阅**

在 `cloud/src/shared/mqtt_client.rs` 订阅 `thing/+/event/+`：

```rust
// 在 PlatformMqttClient 订阅集中增加:
client.subscribe("thing/+/event/+", QoS::AtLeastOnce).await?;
```

消息到达后解析 topic: `thing/{thing_id}/event/{event_name}`，payload 预期 JSON `{level, data, ts?}`。解析后的 ThingEvent 交事件路由函数。

- [ ] **Step 2: 写事件路由函数**

`cloud/src/modules/event/router.rs` — 物事件唯一写入入口：

```rust
pub struct ThingEventInput {
    pub thing_id: String,
    pub workspace_id: String,
    pub event_name: String,       // = event_subtype
    pub level: EventLevel,        // info/warning/error/critical; debug 级直接 drop
    pub data: serde_json::Value,  // 事件 payload
    pub ts: Option<String>,       // RFC3339 UTC, 缺省服务端填
    pub source: EventSource,      // MQTT / DriverDirect
}

pub enum EventLevel { Info=2, Warning=3, Error=4, Critical=5 }

pub enum EventSource { Mqtt, DriverDirect }

/// 事件路由: 校验 → 降级 → 节流 → 落库 → 告警匹配
pub async fn route_thing_event(
    state: &AppState,
    input: ThingEventInput,
) -> Result<EventRouteResult, EventError> {
    // 1. 校验: 畸形 payload (非JSON/缺字段) → 拒收 + malformed metric
    // 2. 降级: 未知事件名 (thing template 无此 event 定义) → event_level=Info + unknown_event=true
    // 3. 节流: 60/min/物 (仅计数 info/warning; error/critical 豁免) → 超限丢弃 + throttled metric
    // 4. 落库: INSERT INTO events (event_type='device', event_subtype=event_name, event_level, device_id=thing_id, workspace_id, content=data, metadata.unknown_event)
    // 5. 告警匹配: 异步查 alarm rules (rule_type='event', condition_config.event_name match, min_level <= event level) → 触发告警
    todo!()
}

pub struct EventRouteResult {
    pub event_id: Option<String>,
    pub throttled: bool,
    pub unknown_event: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("Malformed payload: {0}")]
    MalformedPayload(String),
    #[error("Throttled: {0} events/min exceeded")]
    Throttled(u32),
    #[error("Internal: {0}")]
    Internal(String),
}
```

- [ ] **Step 3: 实现分级节流**

```rust
// 60/min/物 滑动窗口计数器 (info/warning 级别)
// 使用 DashMap<thing_id, VecDeque<Instant>> 或 Redis-style counter
// error/critical → noop check → always pass through

use std::sync::Arc;
use dashmap::DashMap;
use tokio::time::Instant;

pub struct ThrottleState {
    windows: Arc<DashMap<String, VecDeque<Instant>>>,
    max_per_minute: usize, // 60
}

impl ThrottleState {
    pub fn check_and_record(&self, thing_id: &str, level: EventLevel) -> bool {
        match level {
            EventLevel::Error | EventLevel::Critical => true, // 豁免
            EventLevel::Info | EventLevel::Warning | _ => {
                let now = Instant::now();
                let minute_ago = now - Duration::from_secs(60);
                let mut entry = self.windows.entry(thing_id.to_string()).or_default();
                // 清理过期时间戳
                while entry.front().map_or(false, |t| *t < minute_ago) { entry.pop_front(); }
                if entry.len() >= self.max_per_minute {
                    false // 节流拒绝
                } else {
                    entry.push_back(now);
                    true
                }
            }
        }
    }
}
```

- [ ] **Step 4: 写畸形 payload 降级 & 未知事件降级**

```rust
// 畸形检测: serde_json::from_str::<ThingEventPayload> 失败 → 
//   increment metric "events_malformed" + tracing::warn!(payload_first_200_chars) + 不落库

// 未知事件检测: 查 thing_templates.events JSON 数组 → 找不到 event_name →
//   level=Info, metadata.unknown_event=true, 仍落库 (固件可能先于模板更新)
```

- [ ] **Step 5: Integration test — 事件全链路**

```rust
#[sqlx::test]
async fn test_event_full_chain_mqtt_to_db(pool: SqlitePool) {
    // 用真实 MQTT broker (tests/e2e/docker-compose.yml mosquitto)
    // Publish thing/{id}/event/temp_high {level:"error",data:{temp:85}}
    // Assert: events 表有 1 行, event_subtype="temp_high", event_level=4
    // Assert: 对应 alarm rule_type='event' 被触发
}

#[sqlx::test]
async fn test_event_malformed_payload_rejected(pool: SqlitePool) {
    // Publish thing/{id}/event/test "not json" → malformed metric+1, events 表 0 行
}

#[sqlx::test]
async fn test_event_unknown_name_downgraded(pool: SqlitePool) {
    // Publish 未在模板定义的事件 → events 表 1 行, event_level=2(info), metadata.unknown_event=true
}

#[sqlx::test]
async fn test_event_throttle_61_info_drops_61st(pool: SqlitePool) {
    // 连续 publish 61 条 info 事件 → 前 60 条入库, 第 61 条被丢弃, throttled metric=1
}

#[sqlx::test]
async fn test_event_throttle_spares_critical(pool: SqlitePool) {
    // 风暴窗口 61 条 → 60 info 拒绝 + 1 critical → critical 落库, throttled metric 不减 critical
}
```

- [ ] **Step 6: Commit**

---

### Task 2.2: 事件告警 (rule_type='event')

**Files:**
- Create: `cloud/src/modules/alarm/event_matcher.rs`
- Modify: `cloud/src/modules/alarm/service.rs` (事件触发匹配)

- [ ] **Step 1: 事件告警匹配逻辑**

```rust
// event_matcher.rs
pub struct EventAlarmCondition {
    pub event_name: String,          // 匹配事件名
    pub min_level: EventLevel,       // 最低触发级别 (>=)
}

/// 检查事件是否命中告警规则
pub fn match_event_rule(condition: &EventAlarmCondition, event_name: &str, level: EventLevel) -> bool {
    condition.event_name == event_name && level as u8 >= condition.min_level as u8
}
```

- [ ] **Step 2: alarm service 中接收事件**

在 `cloud/src/modules/alarm/service.rs` 中，register_event_alarm_check 方法：查 device_alarm_rules WHERE rule_type='event' AND device_id=? (可选，全局 also)，按 condition_config 匹配 → trigger_alarm。

- [ ] **Step 3: Integration test**

```rust
#[sqlx::test]
async fn test_event_alarm_triggered(pool: SqlitePool) {
    // 1. 创建 alarm rule: rule_type='event', condition_config={event_name:"temp_high", min_level:"warning"}
    // 2. Publish thing/{id}/event/temp_high level=error
    // 3. Assert: alarm created with correct level & device_id
}

#[sqlx::test]
async fn test_event_alarm_not_triggered_below_min_level(pool: SqlitePool) {
    // Rule min_level=error, event level=warning → NOT triggered
}
```

- [ ] **Step 4: Commit**

---

### Task 2.3: LLM 知识摘要（懒计算）

**Files:**
- Create: `cloud/src/modules/thing/service/summary.rs`

- [ ] **Step 1: 脏标记三触发源**

```rust
// summary.rs
pub async fn mark_dirty_for_resource_change(repo: &ThingRepo, thing_id: &str) -> Result<(), ThingError> {
    // 物的 resource 增/删/改 → 该物标 dirty
    sqlx::query("UPDATE devices SET summary_status='dirty' WHERE id = ?")
        .bind(thing_id).execute(&repo.pool).await?;
    Ok(())
}

pub async fn mark_dirty_for_template_change(repo: &ThingRepo, template_id: &str) -> Result<u64, ThingError> {
    // 模板变更 → 所有引用该模板的物标 dirty (一条 UPDATE)
    let result = sqlx::query("UPDATE devices SET summary_status='dirty' WHERE template_id = ?")
        .bind(template_id).execute(&repo.pool).await?;
    Ok(result.rows_affected() as u64)
}

pub async fn mark_dirty_for_name_or_parent_change(repo: &ThingRepo, thing_id: &str) -> Result<u64, ThingError> {
    // 物改名/换父 → 该物及子树标 dirty
    repo.mark_subtree_dirty(thing_id).await
}
```

- [ ] **Step 2: 读时计算 + single-flight**

```rust
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::Mutex;

pub struct SummaryComputer {
    single_flight: Arc<DashMap<String, Arc<tokio::sync::Notify>>>,
}

impl SummaryComputer {
    /// 读时计算: status=ok 直接返回缓存; dirty→LLM 重算 (10s timeout + single-flight)
    pub async fn get_or_compute(&self, thing_id: &str, service: &ThingService) -> Result<Option<String>, ThingError> {
        // 1. 读取当前 summary_status
        // 2. ok → 直接返回 ontology_summary
        // 3. dirty/null → single-flight gate:
        //    - 已有进行中的计算 → 等待 Notify
        //    - 无进行中 → 插入标记，开始计算
        // 4. LLM 调用 (10s timeout, tokio::time::timeout):
        //    - 输入拼接: 物名称/类型/面包屑 + 物模型定义 + 文档前 2000 字 (≤5 篇)
        //    - Prompt: "你是 IoT 物本体专家。为该物写 ≤500 字的摘要..."
        //    - 防注入: <user_document>fencing
        //    - 成功: UPDATE ontology_summary, summary_status='ok', 返回
        //    - 超时/失败: summary_status='failed', 返回旧值或 None
        // 5. 释放 single-flight 锁
        todo!()
    }

    /// 拼装 LLM prompt
    fn build_summary_prompt(thing: &ThingResponse, docs: &[DocumentSnippet]) -> String {
        let mut prompt = format!(
            "你是 IoT 物本体专家。请为该物写一段 ≤500 字的中文摘要:\n\
             物名称: {}\n类型: {}\n路径: {}\n\n物模型:\n",
            thing.name, thing.thing_type,
            thing.breadcrumb.iter().map(|b| b.name.as_str()).collect::<Vec<_>>().join(" → ")
        );
        // 属性/事件/动作定义 (从 template 填充)
        // 文档内容 (每篇前 2000 字, ≤5 篇, <user_document> 围栏包裹):
        for doc in docs.iter().take(5) {
            prompt.push_str(&format!("\n<user_document>\n{}\n</user_document>\n", &doc.content[..doc.content.len().min(2000)]));
        }
        prompt
    }
}
```

- [ ] **Step 3: Integration test — 摘要 5 分支**

```rust
#[sqlx::test]
async fn test_summary_dirty_triggers_recompute(pool: SqlitePool) {
    // 建物+挂文档 → summary_status='dirty' → get_thing → 调 llm → 写回 status='ok'
}

#[sqlx::test]
async fn test_summary_timeout_returns_stale(pool: SqlitePool) {
    // mock LLM 超时 (10s) → 返回旧值, status='failed'
}

#[sqlx::test]
async fn test_summary_single_flight_dedup(pool: SqlitePool) {
    // 并发 3 个 get_thing 调用 → LLM 只调 1 次
}

#[sqlx::test]
async fn test_summary_template_change_marks_all_instances_dirty(pool: SqlitePool) {
    // 更新模板 → 该模板下所有物 status='dirty'
}

#[sqlx::test]
async fn test_summary_parent_change_marks_subtree_dirty(pool: SqlitePool) {
    // 父物改名 → 子树全部 dirty
}
```

- [ ] **Step 4: Commit**

---

### Task 2.4: 可观测性（事件+摘要指标）

**Files:**
- Modify: `cloud/src/modules/event/router.rs` (metric counters)
- Modify: `cloud/src/modules/thing/service/summary.rs` (metric counters)

- [ ] **Step 1: 事件指标**

```rust
// 在事件路由函数中埋点:
// events_ingested{thing_id, event_name, level} += 1
// events_unknown{thing_id, event_name} += 1  
// events_malformed += 1  
// events_throttled{thing_id} += 1

use std::sync::atomic::{AtomicU64, Ordering};

pub struct EventMetrics {
    pub ingested: AtomicU64,
    pub unknown: AtomicU64,
    pub malformed: AtomicU64,
    pub throttled: AtomicU64,
}
// 挂到 AppState; 暴露 /api/metrics/events 查询端点 (可选)
```

- [ ] **Step 2: 摘要指标**

```rust
// summary_success/summary_failed/summary_duration + 触发原因标签
pub struct SummaryMetrics {
    pub success: AtomicU64,
    pub failed: AtomicU64,
    pub total_duration_ms: AtomicU64,
}
```

- [ ] **Step 3: 审计日志**

```rust
// 物操作审计: 创建/删除/改父/invoke_action → 记 audit log
// 复用现有系统审计机制 or 简单 tracing::info!
impl ThingService {
    fn audit_log(&self, action: &str, thing_id: &str, operator: &str) {
        tracing::info!(target: "audit.thing", action, thing_id, operator, "thing operation");
    }
}
```

- [ ] **Step 4: Commit**

---

## Phase 3 — Agent Tools & API (9 工具 + open/mcp + examples)

**依赖:** Phase 2 complete
**验证门:** 9 工具集成测试全绿，open/mcp/examples 走 thing 语义

### Task 3.1: Agent 9 工具

**Files:**
- Create: `cloud/src/modules/agent/tools/thing.rs` (9 工具全在一个文件，工具函数与参数结构体)
- Modify: `cloud/src/modules/agent/tools/mod.rs`
- Modify: `cloud/src/modules/agent/service.rs` (remove build_context prompt 注入)

- [ ] **Step 1: 定义 9 工具参数结构体**

```rust
// agent/tools/thing.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ListThingsParams {
    pub thing_type: Option<String>,
    pub parent_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub q: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,  // default 50, max 200
    #[serde(default)]
    pub offset: i64,
}
fn default_limit() -> i64 { 50 }

#[derive(Debug, Deserialize)]
pub struct GetThingParams { pub thing_id: String }

#[derive(Debug, Deserialize)]
pub struct GetThingProfileParams { pub thing_id: String }

#[derive(Debug, Deserialize)]
pub struct GetThingTreeParams {
    pub root_id: Option<String>,
    #[serde(default = "default_tree_depth")]
    pub depth: i32,  // default 3
}
fn default_tree_depth() -> i32 { 3 }

#[derive(Debug, Deserialize)]
pub struct ReadPropertyParams {
    pub thing_id: String,
    pub property_name: String,
}

#[derive(Debug, Deserialize)]
pub struct InvokeActionParams {
    pub thing_id: String,
    pub action_name: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct QueryEventsParams {
    pub thing_id: String,
    pub event_name: Option<String>,
    pub level: Option<String>,
    pub since: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
pub struct SearchKnowledgeParams {
    pub thing_id: Option<String>,
    pub q: String,
    pub tags: Option<Vec<String>>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
pub struct ReadDocumentParams { pub resource_id: String }
```

- [ ] **Step 2: 实现 9 个工具函数**

```rust
/// list_things — 发现有哪些物
/// 什么时候用: 需要知道当前工作区有哪些物、某个类型/标签/父节点下的物、搜索物
/// 中文描述: 列出工作区内的物。可按类型、父节点、标签、关键词过滤。返回物名称/类型/路径。
pub async fn list_things(state: &AppState, workspace_id: &str, params: ListThingsParams) -> ToolResult {
    let clamped_limit = params.limit.min(200).max(1);
    let query_params = ListThingsParams { limit: Some(clamped_limit), ..params.into_service_params() };
    let result = state.thing_service.list_things(workspace_id, &query_params).await?;
    Ok(json!(result))
}

/// get_thing — 轻量，"这个物是什么、能做什么"
/// 什么时候用: 用户问某个物是什么、它的属性/事件/动作模板、在哪儿
pub async fn get_thing(state: &AppState, params: GetThingParams) -> ToolResult {
    let thing = state.thing_service.get_thing(&params.thing_id).await?;
    Ok(json!(thing))  // 含 breadcrumb + tags + ontology_summary + 模板定义
}

/// get_thing_profile — 聚合快照，一次拿全
/// 什么时候用: 用户需要物全部信息: 状态、属性当前值、最近事件、知识文档
pub async fn get_thing_profile(state: &AppState, params: GetThingProfileParams) -> ToolResult {
    state.thing_service.get_thing_profile(&params.thing_id).await.map(|p| json!(p))
}

/// get_thing_tree — 全局视野
/// 什么时候用: 用户问"有哪些物"、"物怎么组织的"、"某个区域下有什么"
pub async fn get_thing_tree(state: &AppState, workspace_id: &str, params: GetThingTreeParams) -> ToolResult {
    let depth = params.depth.min(10).max(1);
    state.thing_service.get_thing_tree(workspace_id, params.root_id.as_deref(), Some(depth)).await.map(|t| json!(t))
}

/// read_property — 读属性当前值
/// 什么时候用: 用户问某个物某个属性的实时值
pub async fn read_property(state: &AppState, params: ReadPropertyParams) -> ToolResult {
    // 读 app_state.device_cache (既有缓存服务)
    if let Some(cache) = state.device_cache.get(&params.thing_id) {
        if let Some(prop) = cache.properties.iter().find(|p| p.name == params.property_name) {
            return Ok(json!({"value": prop.value, "timestamp": prop.updated_at, "thing_id": params.thing_id, "property": params.property_name}));
        }
    }
    Ok(json!({"value": null, "message": "该属性暂无上报数据"}))
}

/// invoke_action — 下发动作
/// 什么时候用: 用户想控制设备、调某个动作
pub async fn invoke_action(state: &AppState, workspace_id: &str, params: InvokeActionParams) -> ToolResult {
    // 1. 查物 thing_type → 非 device → 返回错误
    // 2. 查 require_action_confirm:
    //    - 开 → require_action_confirm → 返回 {"status": "confirmation_required", "token": uuid, "action": ..., "params": ...}
    //    - 关 → 直接下发 → 返回 {"status": "dispatched", "task_id": uuid}
    // 3. schema 校验 (按模板 actions JSON 定义)
    todo!()
}

/// query_events — 查事件实例列表
/// 什么时候用: 用户问"这个物最近发生了什么事件"、"某个类型的事件"
pub async fn query_events(state: &AppState, params: QueryEventsParams) -> ToolResult {
    let limit = params.limit.min(200).max(1);
    // SELECT FROM events WHERE device_id=? [AND event_subtype=?] [AND event_level>=?] [AND created_at>?] ORDER BY created_at DESC LIMIT ?
    todo!()
}

/// search_knowledge — 全文检索知识文档
/// 什么时候用: 用户问"这个物的文档/知识/说明书"、搜关键词
pub async fn search_knowledge(state: &AppState, params: SearchKnowledgeParams) -> ToolResult {
    let limit = params.limit.min(200).max(1);
    // SELECT FROM thing_resources WHERE workspace_id=? [AND device_id=?] [AND (name LIKE '%q%' OR content LIKE '%q%')] [AND tags match] LIMIT ?
    // 返回: 文档 ID/标题/所属物/内容片段 (first 500 chars, 高亮关键词)
    todo!()
}

/// read_document — 按需取文档正文
/// 什么时候用: 用户想读某篇文档全文
pub async fn read_document(state: &AppState, params: ReadDocumentParams) -> ToolResult {
    // SELECT FROM thing_resources WHERE id=?
    // 返回: 全文 content/file_path 内容
    todo!()
}
```

- [ ] **Step 3: 注册工具 & 移除 build_context 注入**

在 `agent/tools/mod.rs` 注册 9 个新工具函数。在 `agent/service.rs` 移除 `build_context` 相关 system prompt 注入逻辑。

- [ ] **Step 4: Integration test — 9 工具**

```rust
#[sqlx::test]
async fn test_list_things_pagination_clamp(pool: SqlitePool) {
    // limit=500 → clamp to 200
    // limit=0 → clamp to 1 (floor)
    // offset bound → empty array
}

#[sqlx::test]
async fn test_invoke_action_confirm_flow(pool: SqlitePool) {
    // 1. require_action_confirm=true → 返回 confirmation_required + token
    // 2. 用 token confirm → 下发成功
    // 3. require_action_confirm=false → 直接 dispatched
}

#[sqlx::test]
async fn test_invoke_action_non_device_rejected(pool: SqlitePool) {
    // 空间物调 invoke_action → "该物不支持动作"
}

#[sqlx::test]
async fn test_read_property_no_cache(pool: SqlitePool) {
    // 无缓存 → null + "该属性暂无上报数据"
}

#[sqlx::test]
async fn test_search_knowledge_like_search(pool: SqlitePool) {
    // 挂 2 篇文档 → 搜关键词 → 返回 2 结果
}

#[sqlx::test]
async fn test_all_9_tool_params_validation(pool: SqlitePool) {
    // 每个工具: 缺必填参数 → 400; 未知参数 → ignore; 类型错误 → 400
}
```

- [ ] **Step 5: Commit**

---

### Task 3.2: Open/MCP/Examples 全破切 thing

**Files:**
- Modify: `cloud/src/modules/open/` (endpoint paths + 参数语义 → thing)
- Modify: `cloud/src/modules/mcp/tools/device.rs` (→ tool names: list_devices→list_things, etc.)
- Modify: `examples/bacnet-driver/` (HTTP calls → /api/things)
- Modify: `docs/` (设备侧事件上报契约文档)
- Create: `examples/event-publisher/` (参考 MQTT 事件上报实现)

- [ ] **Step 1: Open API 端点改名**

在 `cloud/src/modules/open/` 路由中：所有 `/api/open/devices/**` → `/api/open/things/**`，返回 thing 语义的 DTO。

- [ ] **Step 2: MCP 工具改名**

在 `cloud/src/modules/mcp/tools/device.rs`：
- `list_devices` → `list_things`
- `get_device` → `get_thing`
- 工具描述从 "List devices in your workspace" → "列出工作区内的物（设备/车间/园区等）"
- 参数从 device_id → thing_id
- 新增 thing 语义工具（如有缺失），删除纯 device 语义的工具别称

- [ ] **Step 3: examples/bacnet-driver 更新**

修改 `examples/bacnet-driver` 中的 HTTP 调用：`/api/devices` → `/api/things`，POST body 字段从 device_type→thing_type 等。

- [ ] **Step 4: 设备侧事件上报契约文档 (+ reference 发布实现)**

写 `docs/device-event-contract.md`:
```markdown
# 设备事件上报契约

## MQTT Topic
`thing/{thing_id}/event/{event_name}`

## Payload
{
  "level": "info" | "warning" | "error" | "critical",
  "data": { ... },  // JSON object, 按模板 events schema
  "ts": "2026-07-23T10:30:00Z"  // RFC3339, optional (server fills if missing)
}

## Behavior
- 未知事件名 → 降级 info 存储 (不报错)
- 畸形 payload → 被丢弃 (设备端应确保 JSON 格式)
- 60/min/物 节流 (仅 info/warning; error/critical 不节流)
```

创建 `examples/event-publisher/`:
```rust
// 简单 MQTT 客户端示例, connect→publish thing/{id}/event/{name}→disconnect
// 使用 rumqttc, 读取 env vars 或 cli args 获取 broker/thing_id/event_name/level/data
```

- [ ] **Step 5: Verify**

```bash
cargo build --workspace  # 确认 examples 编译通过
cargo test --test integration -- open_mcp_things -- --nocapture
```

- [ ] **Step 6: Commit**

---

### Task 3.3: Workspaces require_action_confirm 接线

**Files:**
- Modify: `cloud/src/modules/workspace/handler/settings.rs` (读写 require_action_confirm)
- Modify: `cloud/src/modules/agent/tools/thing.rs` (invoke_action 读此值)

- [ ] **Step 1: workspace settings API**

在 workspace handler 中：GET/PUT workspace settings 响应增加 `require_action_confirm` 字段。

- [ ] **Step 2: invoke_action 确认流**

```rust
// invoke_action handler:
// 1. 读 workspaces.require_action_confirm
// 2. true → 生成 confirm_token (UUID), 存 Redis/memory 30min TTL → 返回 confirmation_required
// 3. false → 直发

// POST /api/things/{id}/actions/{action_name}/confirm
// body: {token: "uuid"}
// 验证 token → 合并 params → 下发
```

- [ ] **Step 3: Integration test**

```rust
#[sqlx::test]
async fn test_require_action_confirm_default_true(pool: SqlitePool) {
    // 新建 workspace → require_action_confirm=true → invoke_action 返回 confirmation_required
    // 更新 workspace settings → require_action_confirm=false → invoke_action 返回 dispatched
}
```

- [ ] **Step 4: Commit**

---

## Phase 4 — Frontend (T18-T21)

**依赖:** Phase 3 complete (Agent 工具+API 上线)
**验证门:** D7 交互状态表 8 行全五态手测通过

### Task 4.1: 物列表/树双视图 (T18)

**Files:**
- Create: `web/src/ui/views/things.ts` (物列表主视图)
- Create: `web/src/ui/views/thing-tree.ts` (树视图组件)
- Modify: `web/src/ui/navigation.ts` (导航 "设备"→"物")

实现规范：
- 顶部视图切换「列表｜树」tab，共享同一份过滤条件（类型/搜索/批量操作）
- 列表视图：表格（名称/类型/知识徽标（灰/绿点+文字）/状态/更新时间），骨架行 5 条加载态
- 树视图：全量层级，默认展开 2 层，单击节点→跳详情页，拖拽换父（成环实时红框拒绝）
- 迁移首登提示条（localStorage "thing-ontology-upgrade-notice-dismissed"）
- 空态："还没有物——创建第一个物"主按钮
- 过滤无结果："无匹配，清除过滤"链接
- a11y: 色点必配文字标签，hover/focus 可见态

### Task 4.2: 物详情页四 Tab (T19)

**Files:**
- Create: `web/src/ui/views/thing-detail.ts`
- Create: `web/src/ui/views/thing-detail-overview.ts` (概览)
- Create: `web/src/ui/views/thing-detail-events.ts` (事件)
- Create: `web/src/ui/views/thing-detail-actions.ts` (动作)
- Create: `web/src/ui/views/thing-detail-knowledge.ts` (知识)
- Create: `web/src/ui/views/confirm-modal.ts` (通用确认弹窗)

实现规范：
- 概览首屏：D4 裁决三层——头部条(breadcrumb+名称+type badge+在线状态点)→AI摘要卡(置顶,"AI生成"徽标)→属性网格(大数字+单位+时间戳)→事件时间线(级别色点+文字标签)+快捷动作
- 事件 Tab: 时间线渲染，未知事件徽标
- 动作 Tab: 按钮组，"该物无可用动作"非 device 空态，下发成功 toast(可复制 task_id)
- 知识 Tab: 文档列表+摘要，"未指派"横幅+一键指派
- 确认弹窗: D13 居中 modal (动作名→目标物名→参数键值表→取消/确认)，danger 红钮，Esc=取消 Enter=确认 焦点圈定，文案含"可在工作区设置中关闭动作确认"+设置入口链接
- 摘要: 计算中 >3s→进度文案 "AI 正在生成摘要…"
- 建物后: 跳详情页概览 Tab (不停留列表)

### Task 4.3: 模板三段编辑器 (T20)

**Files:**
- Create: `web/src/ui/views/template-editor.ts`

实现规范：顶部 Tab「属性｜事件｜动作」三段，三段独立全宽表格，行内编辑，跨段参照只读摘要条，段校验失败 Tab 红点定位，保存失败 toast + 未保存标记保留。

### Task 4.4: 导航改名 (T21)

**Files:**
- Modify: `web/src/ui/navigation.ts`, `web/src/ui/views/*.ts`, `web/src/router.ts`

全局扫库替换「设备」→「物」。路由 `/devices`→`/things` + 旧 URL 302 重定向。全站文案：导航项/页面标题/面包屑/按钮/面包屑路径中 device→thing。

### Phase 4 Verification

D7 交互状态表逐行手测（8 features × 5 states = 40 cells）。用 mock 数据构造所有状态，实际渲染确认。

---

## Phase 5 — Extensions (E1/E2/E3)

**依赖:** Phase 4 complete
**验证门:** 各扩展独立验收

### Task 5.1: E1 — 模板市场 thing_templates 类目

**Files:**
- Modify: `cloud/src/modules/marketplace/` (增加 thing_templates 类目)
- Modify: 前端 marketplace 页

- [ ] 上架: thing_templates 整包（属性/事件/动作/默认知识）作为一个列表项
- [ ] 安装: 点击安装→CREATE thing_template in workspace (copy 全部字段)→撞名自动加后缀 (模板名+" (来自市场)")
- [ ] 安装后该模板可用于建物
- [ ] 集成测试: install → name conflict → suffix applied

### Task 5.2: E2 — A2UI 本体驱动渲染

**Files:**
- Modify: `cloud/src/modules/agent/tools/canvas.rs` (A2UI push 用 thing profile 数据)
- Modify: A2UI renderer 侧 (DeviceCard/DataChart/ControlPanel)

- [ ] get_thing_profile 返回的数据结构驱动 A2UI canvas 渲染
- [ ] DeviceCard: 物名称+类型+状态点+关键属性 (消费 profile 的 identity+property+status 字段)
- [ ] DataChart: 数值型属性时序 (消费 property 历史值)
- [ ] ControlPanel: 动作按钮组 (消费 actions 定义)
- [ ] 渲染失败降级 JSON 折叠块
- [ ] D9: props 契约由实现时渲染侧自行定义

### Task 5.3: E3 — DTDL/WoT TD 导入导出

**Files:**
- Create: `cloud/src/modules/thing/service/import.rs`
- Create: `cloud/src/modules/thing/service/export.rs`

- [ ] import_dtdl: 解析 DTDL JSON → thing_template (Properties→properties, Telemetry→events 可能有 gap, Commands→actions)
- [ ] export_dtdl: 产物 model → DTDL JSON (format_version: 2, actions→commands 反向映射)
- [ ] import 兼容旧 commands 键 (自动 map→actions)
- [ ] round-trip 集成测试: import(Azure 样例 DTDL) → export → import again → assert 等价

---

## 回滚策略

- Phase 1 迁移阶段: `cp` SQLite 备份文件恢复 → 重启
- 代码级: `git revert` 各 Phase commit
- 预发布数据量极小 (8 设备/3 资源)，回滚成本低

---

## 总计工时估算

| Phase | Tasks | Human | CC |
|-------|-------|-------|-----|
| Phase 1: Foundation | 1.1-1.4 | ~6d | ~6h |
| Phase 2: Event & Summary | 2.1-2.4 | ~4d | ~4h |
| Phase 3: Agent & API | 3.1-3.3 | ~3.5d | ~3.5h |
| Phase 4: Frontend | 4.1-4.4 | ~5.5d | ~5.5h |
| Phase 5: Extensions | 5.1-5.3 | ~5d | ~5h |
| **Total** | **21 tasks** | **~24d (human)** | **~24h (CC)** |
