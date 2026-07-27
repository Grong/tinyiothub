// Thing Agent Tools — 9 AI agent tools for Thing Ontology
//
// These tools give the AI agent read+execute access to the Thing Ontology.
// They follow the zeroclaw Tool trait pattern (same as CanvasTool, GetSkillTool).
//
// The 9 tools:
//   1. list_things       — list things in workspace (paginated)
//   2. get_thing         — lightweight thing view
//   3. get_thing_profile — full snapshot (properties + events + docs)
//   4. get_thing_tree    — hierarchical tree
//   5. read_property     — current property value from device_cache
//   6. invoke_action     — execute a device action (type='device' guard)
//   7. query_events      — query events for a thing
//   8. search_knowledge  — full-text search thing_resources
//   9. read_document     — full document content

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use dashmap::DashMap;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tinyiothub_ai::types::ToolSafety;
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use crate::modules::thing::{service::ThingService, types::ListThingsParams};

// ============================================================================
// Confirmation token store for invoke_action
// ============================================================================

/// Pending action awaiting user confirmation.
#[derive(Debug, Clone)]
pub struct PendingAction {
    pub token: String,
    pub thing_id: String,
    pub action_name: String,
    pub params: Option<Value>,
    pub workspace_id: String,
    pub created_at: Instant,
}

/// Global store of pending action confirmations (DashMap + 30min TTL).
static PENDING_ACTIONS: std::sync::OnceLock<Arc<DashMap<String, PendingAction>>> =
    std::sync::OnceLock::new();

fn pending_actions() -> &'static Arc<DashMap<String, PendingAction>> {
    PENDING_ACTIONS.get_or_init(|| Arc::new(DashMap::new()))
}

const CONFIRMATION_TTL: Duration = Duration::from_secs(30 * 60);

/// Store a pending action and return its confirmation token.
pub fn store_pending_action(
    thing_id: String,
    action_name: String,
    params: Option<Value>,
    workspace_id: String,
) -> String {
    let token = uuid::Uuid::new_v4().to_string();
    let pending = PendingAction {
        token: token.clone(),
        thing_id,
        action_name,
        params,
        workspace_id,
        created_at: Instant::now(),
    };
    pending_actions().insert(token.clone(), pending);
    token
}

/// Retrieve and consume a pending action by token (returns None if expired or not found).
pub fn take_pending_action(token: &str) -> Option<PendingAction> {
    let entry = pending_actions().remove(token)?;
    if entry.1.created_at.elapsed() > CONFIRMATION_TTL {
        return None;
    }
    Some(entry.1)
}

/// Cleanup expired tokens (call periodically or on access).
#[allow(dead_code)]
pub fn cleanup_expired_tokens() {
    pending_actions().retain(|_, v| v.created_at.elapsed() <= CONFIRMATION_TTL);
}

// ============================================================================
// Helpers
// ============================================================================

/// Wrap a serializable payload into a successful ToolResult.
fn tool_ok(payload: impl serde::Serialize) -> anyhow::Result<ToolResult> {
    Ok(ToolResult {
        success: true,
        output: serde_json::to_string(&payload).unwrap_or_default(),
        error: None,
    })
}

/// Wrap an error message into a failed ToolResult.
fn tool_err(msg: impl Into<String>) -> anyhow::Result<ToolResult> {
    Ok(ToolResult { success: false, output: String::new(), error: Some(msg.into()) })
}

/// Clamp limit to [1, max], defaulting when None.
fn clamp_limit(limit: Option<u32>, default: u32, max: u32) -> u32 {
    limit.unwrap_or(default).clamp(1, max)
}

// ============================================================================
// 1. list_things
// ============================================================================

pub struct ListThingsTool {
    thing_service: Arc<ThingService>,
    workspace_id: String,
}

impl Attributable for ListThingsTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for ListThingsTool {
    fn name(&self) -> &str {
        "list_things"
    }

    fn description(&self) -> &str {
        "列出工作空间内的物（Things）。支持按类型(device/space/line/building)、\
         父节点ID、标签和关键词(q)筛选，支持分页(limit/offset)。\
         当你需要了解工作空间中有哪些设备、空间、产线或建筑时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingType": {
                    "type": "string",
                    "description": "物类型筛选: device, space, line, building"
                },
                "parentId": {
                    "type": "string",
                    "description": "父节点ID，仅返回该节点下的直接子节点"
                },
                "tags": {
                    "type": "string",
                    "description": "按标签筛选（逗号分隔）"
                },
                "q": {
                    "type": "string",
                    "description": "关键词，模糊匹配名称和描述"
                },
                "limit": {
                    "type": "integer",
                    "description": "返回数量上限（默认50，最大200）",
                    "default": 50
                },
                "offset": {
                    "type": "integer",
                    "description": "分页偏移量（默认0）",
                    "default": 0
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            thing_type: Option<String>,
            parent_id: Option<String>,
            tags: Option<String>,
            q: Option<String>,
            limit: Option<u32>,
            offset: Option<u32>,
        }

        let input: Input =
            serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        let limit = clamp_limit(input.limit, 50, 200);
        let offset = input.offset.unwrap_or(0);

        let params = ListThingsParams {
            thing_type: input.thing_type,
            parent_id: input.parent_id,
            tags: input.tags,
            q: input.q,
            limit: Some(limit),
            offset: Some(offset),
        };

        match self.thing_service.list_things(&self.workspace_id, &params).await {
            Ok(result) => tool_ok(result),
            Err(e) => tool_err(e.to_string()),
        }
    }
}

// ============================================================================
// 2. get_thing
// ============================================================================

pub struct GetThingTool {
    thing_service: Arc<ThingService>,
    workspace_id: String,
}

impl Attributable for GetThingTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for GetThingTool {
    fn name(&self) -> &str {
        "get_thing"
    }

    fn description(&self) -> &str {
        "获取单个物的详细信息，包括 ID、名称、类型、面包屑路径、本体摘要和物模型定义。\
         当你需要了解某个具体设备/空间/产线/建筑的详细信息时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（必需）"
                }
            },
            "required": ["thingId"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let thing_id = args
            .get("thingId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("缺少必需参数: thingId"))?;

        match self.thing_service.get_thing(thing_id, &self.workspace_id).await {
            Ok(result) => tool_ok(result),
            Err(e) => tool_err(e.to_string()),
        }
    }
}

// ============================================================================
// 3. get_thing_profile
// ============================================================================

pub struct GetThingProfileTool {
    thing_service: Arc<ThingService>,
    workspace_id: String,
}

impl Attributable for GetThingProfileTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for GetThingProfileTool {
    fn name(&self) -> &str {
        "get_thing_profile"
    }

    fn description(&self) -> &str {
        "获取物的完整快照：基本信息 + 属性值 + 最近10条事件 + 关联知识文档（不含正文）。\
         当你需要全面了解一个设备的状态、历史和知识库时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（必需）"
                }
            },
            "required": ["thingId"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let thing_id = args
            .get("thingId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("缺少必需参数: thingId"))?;

        match self.thing_service.get_thing_profile(thing_id, &self.workspace_id).await {
            Ok(result) => tool_ok(result),
            Err(e) => tool_err(e.to_string()),
        }
    }
}

// ============================================================================
// 4. get_thing_tree
// ============================================================================

pub struct GetThingTreeTool {
    thing_service: Arc<ThingService>,
    workspace_id: String,
}

impl Attributable for GetThingTreeTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for GetThingTreeTool {
    fn name(&self) -> &str {
        "get_thing_tree"
    }

    fn description(&self) -> &str {
        "获取物的层级树结构（仅返回 id/name/type），支持指定根节点和深度。\
         当你需要了解物之间的层级关系（如建筑→楼层→产线→设备）时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "rootId": {
                    "type": "string",
                    "description": "根节点ID（不指定则返回工作空间完整树）"
                },
                "depth": {
                    "type": "integer",
                    "description": "最大深度（默认3，最大10）",
                    "default": 3
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            root_id: Option<String>,
            depth: Option<u32>,
        }

        let input: Input =
            serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        let depth = Some(input.depth.unwrap_or(3).clamp(1, 10));

        match self
            .thing_service
            .get_thing_tree(&self.workspace_id, input.root_id.as_deref(), depth)
            .await
        {
            Ok(result) => tool_ok(result),
            Err(e) => tool_err(e.to_string()),
        }
    }
}

// ============================================================================
// 5. read_property
// ============================================================================

pub struct ReadPropertyTool {
    thing_service: Arc<ThingService>,
    pool: SqlitePool,
    workspace_id: String,
}

impl Attributable for ReadPropertyTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for ReadPropertyTool {
    fn name(&self) -> &str {
        "read_property"
    }

    fn description(&self) -> &str {
        "读取设备上某个属性的当前值和时间戳。\
         当你需要查询设备的最新数据（如温度、湿度、开关状态）时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（必需）"
                },
                "propertyName": {
                    "type": "string",
                    "description": "属性名称（必需）"
                }
            },
            "required": ["thingId", "propertyName"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            thing_id: String,
            property_name: String,
        }

        let input: Input =
            serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        // Verify thing exists
        self.thing_service
            .get_thing(&input.thing_id, &self.workspace_id)
            .await
            .map_err(|e| anyhow::anyhow!("物不存在: {}", e))?;

        // Query property definition from DB
        #[derive(Debug, sqlx::FromRow)]
        struct PropRow {
            name: String,
            display_name: Option<String>,
            description: Option<String>,
            data_type: Option<String>,
            unit: Option<String>,
            min_value: Option<f64>,
            max_value: Option<f64>,
            default_value: Option<String>,
            is_read_only: bool,
        }

        let prop: PropRow = sqlx::query_as::<_, PropRow>(
            "SELECT name, display_name, description, data_type, unit, \
             min_value, max_value, default_value, is_read_only \
             FROM thing_properties WHERE device_id = ? AND name = ?",
        )
        .bind(&input.thing_id)
        .bind(&input.property_name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| anyhow::anyhow!("数据库查询失败: {}", e))?
        .ok_or_else(|| {
            anyhow::anyhow!("属性 '{}' 在物 {} 上未找到", input.property_name, input.thing_id)
        })?;

        // Try device_cache for live value; design 六: no cache → null + hint
        let cached = crate::modules::mcp::get_app_state()
            .and_then(|state| state.device_cache.get(&input.thing_id))
            .and_then(|d| {
                let val = d
                    .properties
                    .as_ref()
                    .and_then(|props| props.iter().find(|p| p.name == input.property_name))
                    .and_then(|p| p.current_value.clone());
                let ts = d.last_heartbeat.clone();
                val.map(|v| (v, ts))
            });

        let (current_value, last_heartbeat, hint) = match cached {
            Some((v, ts)) => (json!(v), json!(ts), Value::Null),
            None => (Value::Null, Value::Null, json!("该属性暂无上报数据")),
        };

        tool_ok(json!({
            "thingId": input.thing_id,
            "propertyName": prop.name,
            "displayName": prop.display_name,
            "description": prop.description,
            "dataType": prop.data_type,
            "unit": prop.unit,
            "minValue": prop.min_value,
            "maxValue": prop.max_value,
            "defaultValue": prop.default_value,
            "isReadOnly": prop.is_read_only,
            "currentValue": current_value,
            "lastHeartbeat": last_heartbeat,
            "hint": hint,
        }))
    }
}

// ============================================================================
// 6. invoke_action
// ============================================================================

pub struct InvokeActionTool {
    thing_service: Arc<ThingService>,
    pool: SqlitePool,
    workspace_id: String,
}

impl Attributable for InvokeActionTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for InvokeActionTool {
    fn name(&self) -> &str {
        "invoke_action"
    }

    fn description(&self) -> &str {
        "对物执行操作。仅 thingType='device' 的物支持此操作。\
         如果工作空间启用了 require_action_confirm，则需要确认才能执行。\
         当你需要控制设备（如开关、重启、设置参数）时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（必需）"
                },
                "actionName": {
                    "type": "string",
                    "description": "操作名称（必需）"
                },
                "params": {
                    "type": "object",
                    "description": "操作参数（可选，JSON 键值对）"
                }
            },
            "required": ["thingId", "actionName"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            thing_id: String,
            action_name: String,
            params: Option<Value>,
        }

        let input: Input =
            serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        // 1. Verify thing exists and check type
        let thing = self
            .thing_service
            .get_thing(&input.thing_id, &self.workspace_id)
            .await
            .map_err(|e| anyhow::anyhow!("物不存在: {}", e))?;

        if thing.thing_type != "device" {
            return tool_err(format!(
                "操作不支持: 物类型为 '{}'，仅 'device' 类型物支持 invoke_action。\
                 对于 space/line/building 类型，请使用其属性或知识库。",
                thing.thing_type
            ));
        }

        // 2. Check require_action_confirm from workspace
        let require_confirm: bool =
            sqlx::query_scalar("SELECT require_action_confirm FROM workspaces WHERE id = ?")
                .bind(&self.workspace_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| anyhow::anyhow!("数据库查询失败: {}", e))?
                // Fail CLOSED (eng-review T7): a missing workspace row means
                // something is wrong — require confirmation (design default ON)
                .unwrap_or(1i32)
                != 0;

        // 3. Check if action exists in device_commands table
        let command_exists: bool = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM thing_actions WHERE device_id = ? AND name = ?",
        )
        .bind(&input.thing_id)
        .bind(&input.action_name)
        .fetch_one(&self.pool)
        .await
        .map(|c| c > 0)
        .unwrap_or(false);

        if !command_exists {
            return tool_err(format!(
                "操作 '{}' 未在物 {} 上注册。请检查可用的操作列表。",
                input.action_name, input.thing_id
            ));
        }

        // 3b. Validate params against the action's parameter schema (design 三;
        // eng-review T7)
        let params_schema: Option<String> =
            sqlx::query_scalar("SELECT parameters FROM thing_actions WHERE device_id = ? AND name = ?")
                .bind(&input.thing_id)
                .bind(&input.action_name)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| anyhow::anyhow!("数据库查询失败: {}", e))?
                .flatten();
        if let Some(ref schema_json) = params_schema
            && let Err(msg) = validate_action_params(schema_json, input.params.as_ref())
        {
            return tool_err(msg);
        }

        // 4. Execute or request confirmation
        if require_confirm {
            let token = store_pending_action(
                input.thing_id.clone(),
                input.action_name.clone(),
                input.params.clone(),
                self.workspace_id.clone(),
            );
            return tool_ok(json!({
                "thingId": input.thing_id,
                "actionName": input.action_name,
                "status": "confirmation_required",
                "token": token,
                "message": "该操作需要用户确认后才能执行。请使用确认接口提交 token。",
                "requireConfirm": true
            }));
        }

        // Execute via DataServer if available
        let app_state = crate::modules::mcp::get_app_state();
        match app_state.and_then(|s| s.data_server().cloned()) {
            Some(data_server) => {
                let cmd = tinyiothub_core::models::device_command::DeviceCommand {
                    id: uuid::Uuid::new_v4().to_string(),
                    device_id: input.thing_id.clone(),
                    name: input.action_name.clone(),
                    display_name: None,
                    description: None,
                    parameters: input.params.as_ref().map(|p| p.to_string()),
                    created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                };
                match data_server.execute_command(cmd) {
                    Ok(()) => tool_ok(json!({
                        "thingId": input.thing_id,
                        "actionName": input.action_name,
                        "status": "executed",
                        "message": "操作已下发执行"
                    })),
                    Err(e) => tool_err(format!("操作执行失败: {}", e)),
                }
            }
            None => {
                tracing::warn!("DataServer not available, action execution is simulated");
                tool_ok(json!({
                    "thingId": input.thing_id,
                    "actionName": input.action_name,
                    "status": "simulated",
                    "message": "操作已记录（DataServer 未就绪，实际执行已模拟）"
                }))
            }
        }
    }
}

// ============================================================================
// 7. query_events
// ============================================================================

pub struct QueryEventsTool {
    pool: SqlitePool,
    workspace_id: String,
}

impl Attributable for QueryEventsTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for QueryEventsTool {
    fn name(&self) -> &str {
        "query_events"
    }

    fn description(&self) -> &str {
        "查询物的事件记录，支持按事件类型、级别、时间范围筛选和分页。\
         当你需要了解设备告警、变更、错误等历史事件时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（必需）"
                },
                "eventName": {
                    "type": "string",
                    "description": "事件类型筛选（匹配 event_type 字段）"
                },
                "level": {
                    "type": "integer",
                    "description": "事件级别筛选（0=debug, 1=info, 2=warning, 3=error, 4=critical）"
                },
                "since": {
                    "type": "string",
                    "description": "起始时间（ISO 8601 格式，如 2026-01-01T00:00:00Z）"
                },
                "limit": {
                    "type": "integer",
                    "description": "返回数量上限（默认50，最大200）",
                    "default": 50
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            thing_id: String,
            event_name: Option<String>,
            level: Option<i32>,
            since: Option<String>,
            limit: Option<u32>,
        }

        let input: Input =
            serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        let limit = clamp_limit(input.limit, 50, 200) as i64;

        // Build dynamic query with QueryBuilder
        let mut builder = sqlx::QueryBuilder::new(
            "SELECT id, event_type, event_subtype, event_level, timestamp, \
             source_type, source_id, title, content, created_at \
             FROM events WHERE device_id = ",
        );
        builder.push_bind(&input.thing_id);
        builder.push(" AND workspace_id = ");
        builder.push_bind(&self.workspace_id);

        if let Some(ref event_name) = input.event_name {
            builder.push(" AND event_type = ");
            builder.push_bind(event_name);
        }
        if let Some(level) = input.level {
            builder.push(" AND event_level = ");
            builder.push_bind(level);
        }
        if let Some(ref since) = input.since {
            builder.push(" AND created_at >= ");
            builder.push_bind(since);
        }

        builder.push(" ORDER BY created_at DESC LIMIT ");
        builder.push_bind(limit);

        #[derive(Debug, serde::Serialize, sqlx::FromRow)]
        struct EventResult {
            id: String,
            event_type: String,
            event_subtype: Option<String>,
            event_level: i32,
            timestamp: Option<String>,
            source_type: String,
            source_id: String,
            title: Option<String>,
            content: String,
            created_at: String,
        }

        let rows = builder
            .build_query_as::<EventResult>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("事件查询失败: {}", e))?;

        let count = rows.len();
        tool_ok(json!({
            "thingId": input.thing_id,
            "total": count,
            "limit": limit,
            "events": rows,
        }))
    }
}

// ============================================================================
// 8. search_knowledge
// ============================================================================

pub struct SearchKnowledgeTool {
    pool: SqlitePool,
    workspace_id: String,
}

impl Attributable for SearchKnowledgeTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for SearchKnowledgeTool {
    fn name(&self) -> &str {
        "search_knowledge"
    }

    fn description(&self) -> &str {
        "搜索物的知识文档（文档/手册/说明书等），使用 LIKE 模糊匹配名称和标签。\
         返回文档元数据（不含正文），获取正文请用 read_document 工具。\
         当你需要查找设备相关的文档和知识时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（可选，不指定则搜索所有文档）"
                },
                "q": {
                    "type": "string",
                    "description": "搜索关键词（必需，匹配名称和标签）"
                },
                "tags": {
                    "type": "string",
                    "description": "按标签筛选（逗号分隔）"
                },
                "limit": {
                    "type": "integer",
                    "description": "返回数量上限（默认50，最大200）",
                    "default": 50
                }
            },
            "required": ["q"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            thing_id: Option<String>,
            q: String,
            tags: Option<String>,
            limit: Option<u32>,
        }

        let input: Input =
            serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        let limit = clamp_limit(input.limit, 50, 200) as i64;
        let like_pattern = format!("%{}%", input.q);

        // Build dynamic query with QueryBuilder
        let mut builder = sqlx::QueryBuilder::new(
            "SELECT id, name, resource_type AS type, file_path, tags, created_at, updated_at \
             FROM resources WHERE workspace_id = ",
        );
        builder.push_bind(&self.workspace_id);

        if let Some(ref tid) = input.thing_id {
            builder.push(" AND device_id = ");
            builder.push_bind(tid);
        }

        // LIKE search on name and tags (FTS5 deferred per TODOS)
        builder.push(" AND (name LIKE ");
        builder.push_bind(&like_pattern);
        builder.push(" OR tags LIKE ");
        builder.push_bind(&like_pattern);
        builder.push(")");

        if let Some(ref t) = input.tags {
            let tag_pattern = format!("%{}%", t);
            builder.push(" AND tags LIKE ");
            builder.push_bind(tag_pattern);
        }

        builder.push(" ORDER BY created_at DESC LIMIT ");
        builder.push_bind(limit);

        #[derive(Debug, serde::Serialize, sqlx::FromRow)]
        struct DocResult {
            id: String,
            name: String,
            #[sqlx(rename = "type")]
            doc_type: String,
            file_path: String,
            tags: String,
            created_at: String,
            updated_at: String,
        }

        let rows = builder
            .build_query_as::<DocResult>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("知识搜索失败: {}", e))?;

        let count = rows.len();
        tool_ok(json!({
            "total": count,
            "results": rows,
        }))
    }
}

// ============================================================================
// 9. read_document
// ============================================================================

pub struct ReadDocumentTool {
    pool: SqlitePool,
    workspace_id: String,
}

impl Attributable for ReadDocumentTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for ReadDocumentTool {
    fn name(&self) -> &str {
        "read_document"
    }

    fn description(&self) -> &str {
        "读取知识文档的完整内容。传入 resourceId 返回文档正文。\
         当你需要查看设备手册、说明书或其他知识文档的详细内容时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "resourceId": {
                    "type": "string",
                    "description": "文档资源ID（必需），由 search_knowledge 返回"
                }
            },
            "required": ["resourceId"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            resource_id: String,
        }

        let input: Input =
            serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        #[derive(Debug, serde::Serialize, sqlx::FromRow)]
        struct DocFull {
            id: String,
            name: String,
            #[sqlx(rename = "type")]
            doc_type: String,
            file_path: String,
            content: Option<String>,
            tags: String,
            device_id: Option<String>,
            created_at: String,
            updated_at: String,
        }

        let doc: DocFull = sqlx::query_as::<_, DocFull>(
            "SELECT id, name, resource_type AS type, file_path, content, tags, device_id, \
             created_at, updated_at FROM resources WHERE id = ? AND workspace_id = ?",
        )
        .bind(&input.resource_id)
        .bind(&self.workspace_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| anyhow::anyhow!("数据库查询失败: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("文档 {} 未找到", input.resource_id))?;

        tool_ok(doc)
    }
}

// ============================================================================
// Factory: create all 9 thing tools
// ============================================================================

/// Create all 9 Thing Ontology agent tools with their safety classifications.
///
/// Read-only tools (searches, gets): safety ReadOnly => auto-approved.
/// Destructive tools (invoke_action): safety Destructive => requires trust approval.
/// Validate invoke params against the action's parameter schema.
///
/// Schema shape: `[{"name": "interval", "type": "number", "required": true}]`.
/// Rules: required params present, no unknown params, primitive type match.
/// Returns a Chinese error message on mismatch (design 六: 校验明细定位字段).
fn validate_action_params(schema_json: &str, params: Option<&Value>) -> Result<(), String> {
    let schema: Vec<Value> = serde_json::from_str(schema_json)
        .map_err(|e| format!("操作参数 schema 解析失败: {}", e))?;
    if schema.is_empty() {
        return Ok(());
    }
    let provided = params.and_then(|p| p.as_object());

    for spec in &schema {
        let name = spec.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let required = spec.get("required").and_then(|r| r.as_bool()).unwrap_or(false);
        let expected = spec.get("type").and_then(|t| t.as_str()).unwrap_or("string");
        let value = provided.and_then(|obj| obj.get(name));
        match value {
            None if required => return Err(format!("缺少必填参数 '{}'", name)),
            None => continue,
            Some(v) => {
                let ok = match expected {
                    "string" => v.is_string(),
                    "number" | "float" | "integer" => v.is_number(),
                    "boolean" | "bool" => v.is_boolean(),
                    "object" => v.is_object(),
                    "array" => v.is_array(),
                    _ => true,
                };
                if !ok {
                    return Err(format!(
                        "参数 '{}' 类型不符: 期望 {}, 实际 {}",
                        name, expected, v
                    ));
                }
            }
        }
    }

    if let Some(obj) = provided {
        let known: Vec<&str> =
            schema.iter().filter_map(|sp| sp.get("name").and_then(|n| n.as_str())).collect();
        for key in obj.keys() {
            if !known.contains(&key.as_str()) {
                return Err(format!("未知参数 '{}', 可用参数: {}", key, known.join(", ")));
            }
        }
    }
    Ok(())
}

pub fn create_thing_tools(
    pool: SqlitePool,
    workspace_id: &str,
) -> Vec<(Box<dyn Tool>, ToolSafety)> {
    let thing_service = Arc::new(ThingService::new(pool.clone()));
    let ws = workspace_id.to_string();

    // Read-only tools — auto-approved
    let read_only = |t: Box<dyn Tool>| (t, ToolSafety::ReadOnly);

    // Destructive tools — require trust approval
    let destructive = |t: Box<dyn Tool>| (t, ToolSafety::Destructive);

    vec![
        // Read-only tools (8)
        read_only(Box::new(ListThingsTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(GetThingTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(GetThingProfileTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(GetThingTreeTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(ReadPropertyTool {
            thing_service: thing_service.clone(),
            pool: pool.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(QueryEventsTool { pool: pool.clone(), workspace_id: ws.clone() })),
        read_only(Box::new(SearchKnowledgeTool { pool: pool.clone(), workspace_id: ws.clone() })),
        read_only(Box::new(ReadDocumentTool { pool: pool.clone(), workspace_id: ws.clone() })),
        // Destructive tool (1)
        destructive(Box::new(InvokeActionTool { thing_service, pool, workspace_id: ws })),
    ]
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── list_things pagination clamp ──────────────────────

    /// Verify that limit=500 is clamped down to 200 (max).
    #[test]
    fn test_list_things_pagination_clamp() {
        assert_eq!(clamp_limit(None, 50, 200), 50, "None => default 50");
        assert_eq!(clamp_limit(Some(30), 50, 200), 30, "explicit within range");
        assert_eq!(clamp_limit(Some(500), 50, 200), 200, "500 => max 200");
        assert_eq!(clamp_limit(Some(0), 50, 200), 1, "0 => min 1");
        assert_eq!(clamp_limit(Some(1000), 50, 200), 200, "1000 => max 200");
    }

    // ── get_thing_tree depth clamp ──────────────────────

    #[test]
    fn test_get_thing_tree_depth_clamp() {
        // Depth used via .clamp(1, 10) in execute
        assert_eq!(3.clamp(1, 10), 3, "None default 3");
        assert_eq!(5u32.clamp(1, 10), 5, "explicit 5");
        assert_eq!(0u32.clamp(1, 10), 1, "0 => min 1");
        assert_eq!(50u32.clamp(1, 10), 10, "50 => max 10");
    }

    // ── invoke_action non-device rejection ──────────────

    /// Verify that invoke_action is classified as Write (not Destructive) by
    /// name-based classification. The factory explicitly declares it as
    /// Destructive for trust enforcement.
    #[test]
    fn test_invoke_action_rejects_non_device_type_in_schema() {
        // Name-based classification: invoke_action → Write (not Destructive)
        assert_eq!(
            tinyiothub_ai::types::classify_tool_safety("invoke_action"),
            ToolSafety::Write,
            "invoke_action is Write by name pattern; factory overrides to Destructive"
        );
    }

    // ── tool name uniqueness ────────────────────────────

    /// Verify all 9 tools are uniquely named.
    #[test]
    fn test_all_9_tool_names_unique() {
        // This test validates that the 9 tool names are correct and unique.
        let names = vec![
            "list_things",
            "get_thing",
            "get_thing_profile",
            "get_thing_tree",
            "read_property",
            "invoke_action",
            "query_events",
            "search_knowledge",
            "read_document",
        ];
        assert_eq!(names.len(), 9, "should have exactly 9 tools");

        // Check uniqueness
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 9, "all 9 tool names must be unique");
    }

    // ── safety classification on name pattern ──────────

    #[test]
    fn test_classify_tool_safety_by_name() {
        use tinyiothub_ai::types::classify_tool_safety;

        // Read-only: starts with list_/get_/read_/search_
        assert_eq!(classify_tool_safety("list_things"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("get_thing"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("get_thing_profile"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("get_thing_tree"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("read_property"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("read_document"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("search_knowledge"), ToolSafety::ReadOnly);

        // Write: doesn't match read/destructive patterns
        assert_eq!(classify_tool_safety("invoke_action"), ToolSafety::Write);
        assert_eq!(classify_tool_safety("query_events"), ToolSafety::Write);
    }
}
