# Edge Intelligence Agent — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 TinyIoTHub API 中嵌入 MCP 协议端点（`/mcp`），扩展工具从 8 个到 ~27 个，支持 OpenClaw AI Agent 通过 MCP 调用所有平台功能。

**Architecture:**
- MCP Server 作为 `api/src/api/mcp/` 模块，嵌入 Axum Router
- 工具直接调用 `AppState` 中的 service，不走 HTTP
- JWT 验证在 API 层统一处理，MCP 调用直接透传
- 废弃独立的 `mcp/` crate（保留参考或删除）

**Tech Stack:** Rust (tokio, axum, sqlx, jsonrpc-core), MCP 2024-11-05

---

## 最终架构

```
OpenClaw
  └── Skill: tinyiothub (type: mcp, endpoint: http://tinyiothub:3002/mcp)
              ↓ MCP over HTTP
TinyIoTHub API :3002
  ├── /api/v1/*  — REST API（Web UI 用）
  └── /mcp       — MCP 协议端点（OpenClaw 用）
              ├── tools/list  — 返回所有工具定义
              └── tools/call — 执行工具调用（带 JWT）
```

---

## File Map

### API 模块 (`api/src/`)

| File | Role |
|------|------|
| `api/mcp/mod.rs` | **New** — MCP Router，合并到 create_router() |
| `api/mcp/handlers.rs` | **New** — MCP 协议处理（tools/list, tools/call） |
| `api/mcp/tool_registry.rs` | **New** — ToolHandler trait + registry |
| `api/mcp/tools/mod.rs` | **New** — 工具定义（ToolMeta） |
| `api/mcp/tools/device.rs` | **New** — device 类别工具 |
| `api/mcp/tools/driver.rs` | **New** — driver 类别工具 |
| `api/mcp/tools/heartbeat.rs` | **New** — heartbeat 类别工具 |
| `api/mcp/tools/self_heal.rs` | **New** — self_heal 类别工具 |
| `api/mcp/tools/knowledge.rs` | **New** — knowledge 类别工具 |
| `api/mcp/dto.rs` | **New** — MCP 相关 DTO |

### 复用现有模块（不新增）

| 模块 | 用途 |
|------|------|
| `api/src/api/devices/` | 设备 CRUD（复用） |
| `api/src/api/alarms/` | 告警（复用） |
| `api/src/api/drivers/` | 驱动管理（复用） |
| `api/src/api/monitoring/` | 监控（复用） |

### 新增业务端点（复用现有 pattern）

| File | Role |
|------|------|
| `api/src/api/heartbeat/mod.rs` | **New** — 心跳端点（stub） |
| `api/src/api/heartbeat/handlers.rs` | **New** — POST/GET /heartbeat |
| `api/src/api/self_healing/mod.rs` | **New** — 自愈端点（stub） |
| `api/src/api/self_healing/handlers.rs` | **New** — policies/actions/events |
| `api/src/api/knowledge/mod.rs` | **New** — 知识库端点（stub） |
| `api/src/api/knowledge/handlers.rs` | **New** — query/contribute/sync |
| `api/src/dto/entity/heartbeat.rs` | **New** — 心跳 DTO |
| `api/src/dto/entity/self_healing.rs` | **New** — 自愈 DTO |
| `api/src/dto/entity/knowledge.rs` | **New** — 知识库 DTO |

### 废弃

| | |
|---|---|
| `mcp/` crate | 废弃，内容迁移到 `api/src/api/mcp/` |

---

## Task 1: MCP 模块骨架

**Files:**
- Create: `api/src/api/mcp/mod.rs`
- Create: `api/src/api/mcp/handlers.rs`
- Create: `api/src/api/mcp/tool_registry.rs`
- Create: `api/src/api/mcp/dto.rs`
- Modify: `api/src/api/mod.rs`

- [ ] **Step 1: 创建 `api/src/api/mcp/mod.rs`**

```rust
pub mod handlers;
pub mod tool_registry;
pub mod dto;

use axum::Router;
use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/mcp", handlers::handle_mcp)
        .route("/mcp/:method", handlers::handle_mcp_named)
}
```

- [ ] **Step 2: 创建 `api/src/api/mcp/dto.rs`**

```rust
use serde::{Deserialize, Serialize};

/// MCP JSON-RPC 请求
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

/// MCP JSON-RPC 响应
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}
```

- [ ] **Step 3: 创建 `api/src/api/mcp/tool_registry.rs`**

```rust
use std::sync::Arc;
use async_trait::async_trait;
use serde_json::Value;

/// 工具处理错误
#[derive(Debug)]
pub enum ToolError {
    InvalidParams(String),
    NotImplemented(String),
    Internal(String),
    NotFound(String),
}

impl From<ToolError> for jsonrpc_core::Error {
    fn from(e: ToolError) -> Self {
        match e {
            ToolError::InvalidParams(msg) => jsonrpc_core::Error {
                code: jsonrpc_core::ErrorCode::InvalidParams,
                message: msg,
                data: None,
            },
            ToolError::NotImplemented(msg) => jsonrpc_core::Error {
                code: jsonrpc_core::ErrorCode::InternalError,
                message: msg,
                data: Some(serde_json::json!({
                    "reason": "not_implemented",
                    "message": msg
                })),
            },
            ToolError::Internal(msg) => jsonrpc_core::Error {
                code: jsonrpc_core::ErrorCode::InternalError,
                message: msg,
                data: None,
            },
            ToolError::NotFound(msg) => jsonrpc_core::Error {
                code: jsonrpc_core::ErrorCode::ServerError(404),
                message: msg,
                data: None,
            },
        }
    }
}

/// 工具处理器 trait
#[async_trait]
pub trait ToolHandler: Send + Sync {
    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    const INPUT_SCHEMA: &'static str; // JSON Schema as string

    async fn handle(
        &self,
        params: Value,
        state: &AppState,
    ) -> Result<Value, ToolError>;
}

/// 工具元数据
#[derive(Debug, Clone)]
pub struct ToolMeta {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// 工具注册表
pub struct ToolRegistry {
    handlers: std::collections::HashMap<&'static str, Arc<dyn ToolHandler>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { handlers: std::collections::HashMap::new() }
    }

    pub fn register<H: ToolHandler + 'static>(&mut self) {
        self.handlers.insert(H::NAME, Arc::new(H));
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn ToolHandler>> {
        self.handlers.get(name)
    }

    pub fn list_tools(&self) -> Vec<ToolMeta> {
        self.handlers.iter().map(|(name, handler)| {
            ToolMeta {
                name: name.to_string(),
                description: handler DESCRIPTION.to_string(),
                input_schema: serde_json::from_str(handler.INPUT_SCHEMA).unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
            }
        }).collect()
    }
}
```

- [ ] **Step 4: 创建 `api/src/api/mcp/handlers.rs`**

```rust
use axum::{
    extract::State,
    routing::get,
    Json, Router,
};
use crate::dto::response::builder::ApiResponseBuilder;
use super::{dto::*, tool_registry::ToolRegistry};
use std::sync::Arc;
use crate::shared::app_state::AppState;

/// 全局工具注册表
pub static REGISTRY: std::sync::OnceLock<ToolRegistry> = std::sync::OnceLock::new();

fn get_registry() -> &'static ToolRegistry {
    REGISTRY.get_or_init(|| {
        let mut r = ToolRegistry::new();
        // 注册所有工具（见 Task 3-7）
        register_all_tools(&mut r);
        r
    })
}

pub fn register_all_tools(_r: &mut ToolRegistry) {
    // 在 Task 3-7 中填充
}

/// POST/GET /mcp — MCP 协议入口
pub async fn handle_mcp(
    State(state): State<Arc<AppState>>,
    Json(req): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    handle_call(req, &state).await
}

pub async fn handle_mcp_named(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(method): axum::extract::Path<String>,
    Json(req): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let mut req = req;
    req.method = method;
    handle_call(req, &state).await
}

async fn handle_call(req: JsonRpcRequest, state: &Arc<AppState>) -> Json<JsonRpcResponse> {
    // 处理 MCP 协议方法
    match req.method.as_str() {
        "initialize" => Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req.id,
            result: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": { "name": "tinyiothub", "version": "1.0.0" },
                "capabilities": { "tools": {} }
            })),
            error: None,
        }),
        "tools/list" => {
            let tools = get_registry().list_tools();
            Json(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: Some(serde_json::json!({ "tools": tools })),
                error: None,
            })
        }
        "tools/call" => {
            let params = req.params.unwrap_or(serde_json::Value::Null);
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let tool_args = params.get("arguments").cloned().unwrap_or(serde_json::Value::Null);

            match get_registry().get(tool_name) {
                Some(handler) => {
                    match handler.handle(tool_args, state).await {
                        Ok(result) => Json(JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id: req.id,
                            result: Some(result),
                            error: None,
                        }),
                        Err(e) => Json(JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id: req.id,
                            result: None,
                            error: Some(jsonrpc_core::Error::from(e).message.into()),
                        }),
                    }
                }
                None => Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: None,
                    error: Some(jsonrpc_core::Error {
                        code: jsonrpc_core::ErrorCode::MethodNotFound,
                        message: format!("Unknown tool: {}", tool_name),
                        data: None,
                    }.into()),
                }),
            }
        }
        _ => Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req.id,
            result: None,
            error: Some(jsonrpc_core::Error {
                code: jsonrpc_core::ErrorCode::MethodNotFound,
                message: format!("Unknown method: {}", req.method),
                data: None,
            }.into()),
        }),
    }
}
```

- [ ] **Step 5: 在 `api/src/api/mod.rs` 中注册 MCP Router**

```rust
pub mod mcp; // 添加

pub fn create_router() -> Router<AppState> {
    Router::new()
        .merge(devices::create_router())
        .merge(alarms::create_router())
        .merge(drivers::create_router())
        .merge(monitoring::create_router())
        .merge(mcp::create_router()) // 添加
        // ...
}
```

- [ ] **Step 6: 验证编译**

Run: `cd api && cargo build`

- [ ] **Step 7: Commit**

```bash
git add api/src/api/mcp/
git commit -m "feat(api): embed MCP protocol endpoint at /mcp

- MCP handlers for tools/list and tools/call
- ToolHandler trait and registry pattern
- MCP protocol version 2024-11-05
- /mcp route added to API router"
```

---

## Task 2: 设备类别工具（12 个）

**Files:**
- Modify: `api/src/api/mcp/tools/mod.rs`
- Create: `api/src/api/mcp/tools/device.rs`

### 工具列表

| 工具名 | 描述 | 实现 |
|--------|------|------|
| `list_devices` | 设备列表（分页） | 复用 |
| `get_device` | 设备详情 | 复用 |
| `get_device_status` | 在线/离线状态 | 复用 |
| `read_properties` | 读取属性 | 复用 |
| `write_properties` | 写入属性 | 新增 |
| `send_command` | 发送命令 | 复用 |
| `create_device` | 创建设备 | 新增 |
| `update_device` | 更新设备 | 新增 |
| `delete_device` | 删除设备 | 新增 |
| `get_device_history` | 设备历史 | 新增 |
| `get_device_metrics` | 设备指标 | 新增 |
| `export_device_report` | 导出报告 | 新增 |

### 实现要点

复用现有 API Handler，直接调用 `state.device_service` 等。

示例 — `create_device`:

```rust
pub struct CreateDeviceHandler;

#[async_trait]
impl ToolHandler for CreateDeviceHandler {
    const NAME: &'static str = "create_device";
    const DESCRIPTION: &'static str = "Create a new device from structured input";
    const INPUT_SCHEMA: &'static str = r#"{
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "device_type": {"type": "string"},
            "protocol": {"type": "string"},
            "interface": {"type": "string"},
            "config": {"type": "object"},
            "points": {"type": "array"}
        },
        "required": ["name"]
    }"#;

    async fn handle(&self, params: Value, state: &AppState) -> Result<Value, ToolError> {
        #[derive(serde::Deserialize)]
        struct CreateParams {
            name: String,
            device_type: Option<String>,
            protocol: Option<String>,
            interface: Option<String>,
            config: Option<Value>,
            points: Option<Vec<Value>>,
        }

        let params: CreateParams = serde_json::from_value(params)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        // 调用现有 DeviceService
        let device = state.device_service.create_device(/* ... */)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?;

        Ok(serde_json::to_value(device).map_err(|e| ToolError::Internal(e.to_string()))?)
    }
}
```

- [ ] **Commit after Task 2**

```bash
git add api/src/api/mcp/tools/device.rs
git commit -m "feat(mcp): add 12 device category tools

Tools: list_devices, get_device, get_device_status, read_properties,
write_properties, send_command, create_device, update_device,
delete_device, get_device_history, get_device_metrics, export_device_report"
```

---

## Task 3: 驱动类别工具（7 个）

**Files:**
- Create: `api/src/api/mcp/tools/driver.rs`

### 工具列表

| 工具名 | 描述 | 实现 |
|--------|------|------|
| `list_drivers` | 驱动列表 | 复用 |
| `match_driver` | 匹配驱动 | 新增 |
| `generate_driver` | 生成驱动（stub） | stub |
| `load_driver` | 加载驱动 | 新增 |
| `unload_driver` | 卸载驱动 | 新增 |
| `test_driver` | 测试驱动 | 新增 |
| `get_driver_config_schema` | 获取配置 | 复用 |

- [ ] **Commit after Task 3**

---

## Task 4: 心跳类别工具（3 个）

**Files:**
- Create: `api/src/api/mcp/tools/heartbeat.rs`
- Create: `api/src/api/heartbeat/mod.rs`
- Create: `api/src/api/heartbeat/handlers.rs`
- Create: `api/src/dto/entity/heartbeat.rs`

### 工具列表

| 工具名 | 描述 | 实现 |
|--------|------|------|
| `report_heartbeat` | 上报心跳 | 新增 |
| `get_heartbeat_status` | 获取心跳状态 | 新增 |
| `configure_heartbeat` | 配置心跳 | 新增 |

### 心跳端点（stub）

Phase 1 用内存存储，后续持久化。

```rust
static LAST_HEARTBEAT: Lazy<RwLock<Option<HeartbeatReport>>> = Lazy::new(|| RwLock::new(None));
```

- [ ] **Commit after Task 4**

---

## Task 5: 自愈类别工具（3 个）

**Files:**
- Create: `api/src/api/mcp/tools/self_heal.rs`
- Create: `api/src/api/self_healing/mod.rs`
- Create: `api/src/api/self_healing/handlers.rs`
- Create: `api/src/dto/entity/self_healing.rs`

### 工具列表

| 工具名 | 描述 | 实现 |
|--------|------|------|
| `get_self_heal_policy` | 获取策略 | stub |
| `execute_self_heal_action` | 执行动作 | stub（返回 501） |
| `get_recovery_history` | 恢复历史 | stub |

### 自愈端点（stub）

Phase 1 返回默认策略 + 501 NotImplemented。

- [ ] **Commit after Task 5**

---

## Task 6: 知识库类别工具（3 个）

**Files:**
- Create: `api/src/api/mcp/tools/knowledge.rs`
- Create: `api/src/api/knowledge/mod.rs`
- Create: `api/src/api/knowledge/handlers.rs`
- Create: `api/src/dto/entity/knowledge.rs`

### 工具列表

| 工具名 | 描述 | 实现 |
|--------|------|------|
| `query_knowledge_base` | 查询知识库 | stub |
| `contribute_knowledge` | 贡献知识 | stub |
| `sync_knowledge` | 同步知识 | stub（返回 501） |

### 知识库端点（stub）

Phase 1 用内存存储。

- [ ] **Commit after Task 6**

---

## Task 7: 废弃旧 MCP Server

**Files:**
- Delete: `mcp/` crate（或在 `Cargo.toml` 中移除）

- [ ] **Step 1: 检查 mcp crate 是否有其他依赖**

```bash
grep -r "mcp" api/Cargo.toml
```

- [ ] **Step 2: 移除或标记废弃**

```bash
git rm -rf mcp/
# 或保留只读参考
```

- [ ] **Commit**

---

## Task 8: 测试

**Files:**
- Create: `api/src/api/mcp/tests.rs`

### 测试用例

```rust
#[test]
fn test_tool_registry_contains_all_tools() {
    let registry = get_registry();
    let tool_names: Vec<&str> = registry.handlers.keys().copied().collect();

    assert!(tool_names.contains(&"list_devices"));
    assert!(tool_names.contains(&"create_device"));
    assert!(tool_names.contains(&"match_driver"));
    assert!(tool_names.contains(&"report_heartbeat"));
    assert!(tool_names.contains(&"get_self_heal_policy"));
    assert!(tool_names.contains(&"query_knowledge_base"));
}

#[test]
fn test_mcp_protocol_version() {
    // verify initialize returns correct protocol version
}
```

- [ ] **Run tests**

```bash
cd api && cargo test
```

- [ ] **Commit**

---

## Task 9: 端到端验证

- [ ] **Step 1: 启动 API**

```bash
cd api && cargo run
```

- [ ] **Step 2: 测试 MCP 端点**

```bash
# tools/list
curl -X POST http://localhost:3002/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'

# tools/call (create_device)
curl -X POST http://localhost:3002/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"create_device","arguments":{"name":"test"}}}'
```

- [ ] **Step 3: 验证 OpenClaw 集成**

配置 OpenClaw skill:

```yaml
skills:
  - name: tinyiothub
    type: mcp
    endpoint: http://tinyiothub:3002/mcp
```

- [ ] **Commit**

---

## 任务总结

| Task | 描述 | 文件数 | 优先级 |
|------|------|--------|--------|
| 1 | MCP 模块骨架 | 5 | P0 |
| 2 | 设备类别工具（12个） | 2 | P0 |
| 3 | 驱动类别工具（7个） | 1 | P0 |
| 4 | 心跳类别工具（3个）+ 端点 | 4 | P1 |
| 5 | 自愈类别工具（3个）+ 端点 | 4 | P1 |
| 6 | 知识库类别工具（3个）+ 端点 | 4 | P2 |
| 7 | 废弃旧 mcp/ crate | - | P2 |
| 8 | 测试 | 1 | P0 |
| 9 | 端到端验证 | - | P0 |

**总计：27 个 MCP 工具**

---

## 与之前计划对比

| 项目 | 之前（独立 MCP） | 现在（嵌入式） |
|------|-----------------|--------------|
| 部署 | 独立容器/进程 | 复用 API 容器 |
| 通信 | HTTP 调用 | 直接函数调用 |
| JWT | 需要转发 | API 中间件统一处理 |
| TinyIoTHubClient | 需要 | 不需要 |
| mcp/ crate | 主开发对象 | 废弃 |
| API 端点 | 新增 | 新增（心跳/自愈/知识库） |
| 工具实现 | MCP Tool → HTTP Client → API | ToolHandler → 直接调用 Service |
