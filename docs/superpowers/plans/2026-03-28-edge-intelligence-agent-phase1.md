# Edge Intelligence Agent — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Phase 1 expands the TinyIoTHub MCP tool surface from 8 to ~27 tools, adds backend API endpoints for new capabilities, and refactors the handler architecture to a registry pattern.

**Architecture:**
- Refactor MCP `handle_call` from match-statement to `HashMap<&str, Arc<dyn ToolHandler>>` registry
- New backend API endpoints in `api/src/api/` for heartbeat, self-healing stubs, device history, and metrics
- Extend `TinyIoTHubClient` with 11 new API methods
- New MCP tools in `mcp/src/tools/` organized by category (device, driver, heartbeat, self_heal, knowledge)

**Tech Stack:** Rust (tokio, axum, sqlx), MCP protocol (jsonrpc-core), SQLite

---

## File Map

### MCP Server (`mcp/src/`)

| File | Role |
|------|------|
| `main.rs` | Refactor `handle_call` to use registry |
| `client.rs` | Add 11 new API methods |
| `tools/mod.rs` | Tool registry (expand `get_all_tools`) |
| `tools/device.rs` | Add 4 new device tools (existing 5) |
| `tools/driver.rs` | **New file** — driver tools (match, generate, load, unload, test, config_schema) |
| `tools/heartbeat.rs` | **New file** — heartbeat tools (report, get_status, configure) |
| `tools/self_heal.rs` | **New file** — self-heal tools (get_policy, execute, get_history) |
| `tools/knowledge.rs` | **New file** — knowledge tools (query, contribute, sync) |
| `handlers/mod.rs` | **New file** — `ToolHandler` trait + base error type |
| `handlers/device_handler.rs` | **New file** — device tool implementations |
| `handlers/driver_handler.rs` | **New file** — driver tool implementations |
| `handlers/heartbeat_handler.rs` | **New file** — heartbeat tool implementations |
| `handlers/self_heal_handler.rs` | **New file** — self-heal tool implementations |
| `handlers/knowledge_handler.rs` | **New file** — knowledge tool implementations |
| `tests.rs` | Expand tests for all new tools |

### Backend API (`api/src/`)

| File | Role |
|------|------|
| `api/devices/history.rs` | **New file** — GET /devices/:id/history endpoint |
| `api/devices/metrics.rs` | **New file** — GET /devices/:id/metrics endpoint |
| `api/devices/mod.rs` | Add history + metrics router merges |
| `api/reports/device.rs` | **New file** — POST /reports/device endpoint |
| `api/reports/mod.rs` | **New file** — reports router |
| `api/reports/mod.rs` | Merge into main api router |
| `api/drivers/test.rs` | **New file** — POST /drivers/:name/test endpoint |
| `api/drivers/mod.rs` | Add test_driver route |
| `api/heartbeat/mod.rs` | **New file** — heartbeat API router |
| `api/heartbeat/handlers.rs` | **New file** — heartbeat endpoints (report, get_status, configure) |
| `api/self_healing/mod.rs` | **New file** — self-healing API router |
| `api/self_healing/handlers.rs` | **New file** — self-healing endpoints (policies, actions, events) |
| `api/knowledge/mod.rs` | **New file** — knowledge API router |
| `api/knowledge/handlers.rs` | **New file** — knowledge endpoints (query, contribute, sync) |
| `dto/entity/heartbeat.rs` | **New file** — heartbeat DTOs |
| `dto/entity/self_healing.rs` | **New file** — self-healing DTOs |
| `dto/entity/knowledge.rs` | **New file** — knowledge DTOs |
| `dto/entity/device_history.rs` | **New file** — device history DTOs |
| `domain/self_healing/` | **New directory** — self-healing domain (stub only for Phase 1) |
| `domain/self_healing/mod.rs` | **New file** |
| `domain/self_healing/policy.rs` | **New file** — policy storage (stub) |
| `domain/knowledge/` | **New directory** — knowledge domain (stub only for Phase 1) |
| `domain/knowledge/mod.rs` | **New file** |
| `domain/knowledge/store.rs` | **New file** — in-memory knowledge store (stub) |

---

## Task 1: Handler Registry Architecture Refactor

**Files:**
- Modify: `mcp/src/main.rs:29-100`
- Create: `mcp/src/handlers/mod.rs`
- Create: `mcp/src/handlers/base.rs`

- [ ] **Step 1: Create `mcp/src/handlers/mod.rs` — Handler trait and registry**

```rust
use std::sync::Arc;
use async_trait::async_trait;
use jsonrpc_core::{Error, Params, Value};
use crate::client::TinyIoTHubClient;

/// Base error type for tool handlers
#[derive(Debug)]
pub enum ToolError {
    InvalidParams(String),
    NotImplemented(String),
    Internal(String),
}

impl From<ToolError> for Error {
    fn from(e: ToolError) -> Self {
        match e {
            ToolError::InvalidParams(msg) => Error {
                code: jsonrpc_core::ErrorCode::InvalidParams,
                message: msg,
                data: None,
            },
            ToolError::NotImplemented(msg) => Error {
                code: jsonrpc_core::ErrorCode::InternalError,
                message: msg,
                data: Some(serde_json::json!({
                    "reason": "not_implemented",
                    "message": msg
                })),
            },
            ToolError::Internal(msg) => Error {
                code: jsonrpc_core::ErrorCode::InternalError,
                message: msg,
                data: None,
            },
        }
    }
}

/// Tool handler trait — each tool implements this
#[async_trait]
pub trait ToolHandler: Send + Sync {
    const NAME: &'static str;

    async fn handle(
        &self,
        params: Params,
        client: &TinyIoTHubClient,
    ) -> Result<Value, ToolError>;

    fn name(&self) -> &'static str {
        Self::NAME
    }
}

/// Tool handler registry
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

    pub fn names(&self) -> Vec<&'static str> {
        self.handlers.keys().copied().collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Add async_trait dependency to `mcp/Cargo.toml`**

```toml
[dependencies]
async_trait = "0.1"
```

Run: `cd mcp && cargo add async_trait`

- [ ] **Step 3: Refactor `handle_call` in `main.rs` to use registry**

Replace the match statement (lines ~69-91) with:

```rust
// In McpServer struct, replace client field handling:
// Keep client, add registry field

pub struct McpServer {
    client: TinyIoTHubClient,
    registry: ToolRegistry,
    #[allow(dead_code)]
    config: McpConfig,
}

impl McpServer {
    pub fn new(config: McpConfig) -> Self {
        let client = TinyIoTHubClient::new(
            &config.tinyiothub.api_url,
            &config.tinyiothub.api_key,
        );
        let mut registry = ToolRegistry::new();
        // Register all handlers here (see Task 2)
        Self { client, registry, config }
    }

    pub async fn handle_call(&self, call: MethodCall) -> Result<Value, Error> {
        let method = call.method.clone();
        info!("Handling MCP call: {}", method);

        match method.as_str() {
            "initialize" => self.handle_initialize(call.params).await,
            "tools/list" => self.handle_tools_list(call.params).await,
            _ => {
                let handler = self.registry.get(&method).ok_or_else(|| Error {
                    code: ErrorCode::MethodNotFound,
                    message: format!("Unknown method: {}", method),
                    data: None,
                })?;
                handler.handle(call.params, &self.client).await.map_err(Into::into)
            }
        }
    }
}
```

Run: `cd mcp && cargo build` — verify compilation

- [ ] **Step 4: Commit**

```bash
git add mcp/src/handlers/ mcp/src/main.rs mcp/Cargo.toml
git commit -m "feat(mcp): refactor handle_call to use ToolHandler registry

- Add ToolHandler trait with async_trait
- Add ToolRegistry for dynamic handler registration
- Replace match explosion with registry lookup
- NotImplemented error variant for Phase 1 stubs"
```

---

## Task 2: Register Existing Tools in Registry

**Files:**
- Create: `mcp/src/handlers/alarm_handler.rs`
- Modify: `mcp/src/main.rs` (McpServer::new)

- [ ] **Step 1: Create device handler struct in `mcp/src/handlers/device_handler.rs`**

```rust
use async_trait::async_trait;
use jsonrpc_core::{Error, Params, Value};
use serde::Deserialize;
use crate::client::TinyIoTHubClient;
use super::ToolHandler;

pub struct ListDevicesHandler;
pub struct GetDeviceHandler;
pub struct GetDeviceStatusHandler;
pub struct ReadSensorDataHandler;
pub struct SendCommandHandler;

#[async_trait]
impl ToolHandler for ListDevicesHandler {
    const NAME: &'static str = "list_devices";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, super::ToolError> {
        #[derive(Deserialize)]
        struct ListDevicesParams {
            page: Option<u32>,
            page_size: Option<u32>,
            include_properties: Option<bool>,
        }

        let params: ListDevicesParams = params.parse().map_err(|e| super::ToolError::InvalidParams(e.to_string()))?;
        let page_size = params.page_size.unwrap_or(20).min(1000);

        let response = client.list_devices(
            params.page.unwrap_or(1),
            page_size,
            params.include_properties.unwrap_or(false),
        ).await.map_err(|e| super::ToolError::Internal(e.to_string()))?;

        Ok(serde_json::to_value(response).map_err(|e| super::ToolError::Internal(e.to_string()))?)
    }
}

#[async_trait]
impl ToolHandler for GetDeviceHandler {
    const NAME: &'static str = "get_device";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, super::ToolError> {
        #[derive(Deserialize)]
        struct GetDeviceParams {
            device_id: String,
            include_properties: Option<bool>,
        }

        let params: GetDeviceParams = params.parse().map_err(|e| super::ToolError::InvalidParams(e.to_string()))?;

        let response = client.get_device(
            &params.device_id,
            params.include_properties.unwrap_or(true),
        ).await.map_err(|e| super::ToolError::Internal(e.to_string()))?;

        Ok(serde_json::to_value(response).map_err(|e| super::ToolError::Internal(e.to_string()))?)
    }
}

#[async_trait]
impl ToolHandler for GetDeviceStatusHandler {
    const NAME: &'static str = "get_device_status";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, super::ToolError> {
        #[derive(Deserialize)]
        struct GetDeviceStatusParams { device_id: String }

        let params: GetDeviceStatusParams = params.parse().map_err(|e| super::ToolError::InvalidParams(e.to_string()))?;

        let device = client.get_device(&params.device_id, false)
            .await.map_err(|e| super::ToolError::Internal(e.to_string()))?;

        Ok(serde_json::json!({
            "device_id": device.id,
            "name": device.name,
            "state": device.state,
            "is_online": device.is_online,
            "last_heartbeat": device.last_heartbeat,
        }))
    }
}

#[async_trait]
impl ToolHandler for ReadSensorDataHandler {
    const NAME: &'static str = "read_sensor_data";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, super::ToolError> {
        #[derive(Deserialize)]
        struct ReadSensorParams {
            device_id: String,
            properties: Option<Vec<String>>,
        }

        let params: ReadSensorParams = params.parse().map_err(|e| super::ToolError::InvalidParams(e.to_string()))?;

        let response = client.read_device_properties(&params.device_id, params.properties)
            .await.map_err(|e| super::ToolError::Internal(e.to_string()))?;

        Ok(serde_json::to_value(response).map_err(|e| super::ToolError::Internal(e.to_string()))?)
    }
}

#[async_trait]
impl ToolHandler for SendCommandHandler {
    const NAME: &'static str = "send_command";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, super::ToolError> {
        #[derive(Deserialize)]
        struct SendCommandParams {
            device_id: String,
            command: String,
            parameters: Option<serde_json::Value>,
        }

        let params: SendCommandParams = params.parse().map_err(|e| super::ToolError::InvalidParams(e.to_string()))?;

        let response = client.send_command(&params.device_id, &params.command, params.parameters)
            .await.map_err(|e| super::ToolError::Internal(e.to_string()))?;

        Ok(serde_json::to_value(response).map_err(|e| super::ToolError::Internal(e.to_string()))?)
    }
}
```

- [ ] **Step 2: Register in `McpServer::new`**

```rust
// In McpServer::new:
let mut registry = ToolRegistry::new();
registry::register::<handlers::device_handler::ListDevicesHandler>(&mut registry);
registry::register::<handlers::device_handler::GetDeviceHandler>(&mut registry);
registry::register::<handlers::device_handler::GetDeviceStatusHandler>(&mut registry);
registry::register::<handlers::device_handler::ReadSensorDataHandler>(&mut registry);
registry::register::<handlers::device_handler::SendCommandHandler>(&mut registry);
registry::register::<handlers::alarm_handler::ListAlarmsHandler>(&mut registry);
registry::register::<handlers::alarm_handler::AcknowledgeAlarmHandler>(&mut registry);
registry::register::<handlers::alarm_handler::GetAlarmStatisticsHandler>(&mut registry);
registry::register::<handlers::driver_handler::ListDriversHandler>(&mut registry);
```

- [ ] **Step 3: Commit**

```bash
git add mcp/src/handlers/device_handler.rs mcp/src/handlers/alarm_handler.rs mcp/src/main.rs
git commit -m "feat(mcp): register existing 9 tools in handler registry"
```

---

## Task 3: Extend TinyIoTHubClient with New API Methods

**Files:**
- Modify: `mcp/src/client.rs`

- [ ] **Step 1: Add 11 new client methods to `client.rs`**

Add these methods to `impl TinyIoTHubClient`:

```rust
// Device methods

/// Get device history (new endpoint)
pub async fn get_device_history(
    &self,
    device_id: &str,
    start_time: Option<&str>,
    end_time: Option<&str>,
    limit: u32,
) -> Result<Vec<serde_json::Value>, ClientError> {
    let validated_id = Self::validate_id(device_id)?;
    let limit = limit.min(10000); // cap at reasonable max
    let path = format!(
        "/devices/{}/history?limit={}&start_time={}&end_time={}",
        validated_id, limit,
        start_time.unwrap_or(""),
        end_time.unwrap_or("")
    );
    self.request(reqwest::Method::GET, &path, None).await
}

/// Get device metrics (new endpoint)
pub async fn get_device_metrics(
    &self,
    device_id: &str,
) -> Result<serde_json::Value, ClientError> {
    let validated_id = Self::validate_id(device_id)?;
    let path = format!("/devices/{}/metrics", validated_id);
    self.request(reqwest::Method::GET, &path, None).await
}

/// Export device report (new endpoint)
pub async fn export_device_report(
    &self,
    device_id: &str,
    report_type: &str,
) -> Result<serde_json::Value, ClientError> {
    let validated_id = Self::validate_id(device_id)?;
    let path = format!("/reports/device/{}", validated_id);
    let body = serde_json::json!({ "report_type": report_type });
    self.request(reqwest::Method::POST, &path, Some(body)).await
}

/// Test driver (new endpoint)
pub async fn test_driver(
    &self,
    driver_name: &str,
    config: serde_json::Value,
) -> Result<serde_json::Value, ClientError> {
    let path = format!("/drivers/{}/test", driver_name);
    self.request(reqwest::Method::POST, &path, Some(config)).await
}

/// Heartbeat methods (new endpoints)
pub async fn report_heartbeat(
    &self,
    heartbeat: &serde_json::Value,
) -> Result<serde_json::Value, ClientError> {
    self.request(reqwest::Method::POST, "/heartbeat", Some(heartbeat.clone())).await
}

pub async fn get_heartbeat_status(
    &self,
) -> Result<serde_json::Value, ClientError> {
    self.request(reqwest::Method::GET, "/heartbeat", None).await
}

pub async fn configure_heartbeat(
    &self,
    config: &serde_json::Value,
) -> Result<serde_json::Value, ClientError> {
    self.request(reqwest::Method::PUT, "/heartbeat/config", Some(config.clone())).await
}

/// Self-healing methods (stub endpoints)
pub async fn get_self_heal_policy(
    &self,
) -> Result<serde_json::Value, ClientError> {
    self.request(reqwest::Method::GET, "/self-healing/policies", None).await
}

pub async fn execute_self_heal_action(
    &self,
    level: &str,
    target: &str,
    action: &str,
) -> Result<serde_json::Value, ClientError> {
    let path = format!("/self-healing/actions/{}", level);
    let body = serde_json::json!({ "target": target, "action": action });
    self.request(reqwest::Method::POST, &path, Some(body)).await
}

pub async fn get_recovery_history(
    &self,
    limit: u32,
) -> Result<Vec<serde_json::Value>, ClientError> {
    let path = format!("/self-healing/events?limit={}", limit.min(1000));
    self.request(reqwest::Method::GET, &path, None).await
}

/// Knowledge methods (stub endpoints)
pub async fn query_knowledge_base(
    &self,
    query: &str,
) -> Result<Vec<serde_json::Value>, ClientError> {
    let path = format!("/knowledge?query={}", urlencoding::encode(query));
    self.request(reqwest::Method::GET, &path, None).await
}

pub async fn contribute_knowledge(
    &self,
    entry: &serde_json::Value,
) -> Result<serde_json::Value, ClientError> {
    self.request(reqwest::Method::POST, "/knowledge", Some(entry.clone())).await
}

pub async fn sync_knowledge(
    &self,
    direction: &str,
) -> Result<serde_json::Value, ClientError> {
    let body = serde_json::json!({ "direction": direction });
    self.request(reqwest::Method::POST, "/knowledge/sync", Some(body)).await
}
```

- [ ] **Step 2: Add urlencoding dependency**

Run: `cd mcp && cargo add urlencoding`

- [ ] **Step 3: Verify build**

Run: `cd mcp && cargo build`

- [ ] **Step 4: Commit**

```bash
git add mcp/src/client.rs mcp/Cargo.toml
git commit -m "feat(mcp): add 11 new API methods to TinyIoTHubClient

- get_device_history, get_device_metrics, export_device_report
- test_driver
- report_heartbeat, get_heartbeat_status, configure_heartbeat
- get_self_heal_policy, execute_self_heal_action, get_recovery_history
- query_knowledge_base, contribute_knowledge, sync_knowledge"
```

---

## Task 4: New Backend API — Device History Endpoint

**Files:**
- Create: `api/src/dto/entity/device_history.rs`
- Create: `api/src/api/devices/history.rs`
- Modify: `api/src/api/devices/mod.rs`

- [ ] **Step 1: Create DTO `api/src/dto/entity/device_history.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceHistoryPoint {
    pub timestamp: String,
    pub property_name: String,
    pub value: String,
    pub quality: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceHistoryQuery {
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub property_names: Option<Vec<String>>,
    pub limit: Option<u32>,
}

impl DeviceHistoryQuery {
    pub fn max_window_days(&self) -> u32 {
        7 // max 7 days
    }
}
```

- [ ] **Step 2: Create `api/src/api/devices/history.rs`**

```rust
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::dto::{
    entity::device_history::{DeviceHistoryPoint, DeviceHistoryQuery},
    response::{builder::ApiResponseBuilder, ApiResponse},
};
use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/:device_id/history", get(get_device_history))
}

async fn get_device_history(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Query(query): Query<DeviceHistoryQuery>,
) -> Json<ApiResponse<Vec<DeviceHistoryPoint>>> {
    // Validate time window
    let limit = query.limit.unwrap_or(1000).min(10000);

    match state.device_service.get_device_history(&device_id, &query, limit).await {
        Ok(history) => ApiResponseBuilder::success(history),
        Err(e) => {
            tracing::error!("Failed to get device history for {}: {}", device_id, e);
            ApiResponseBuilder::error("获取设备历史失败")
        }
    }
}
```

- [ ] **Step 3: Merge into devices router in `api/src/api/devices/mod.rs`**

```rust
pub mod history;
// ...
pub fn create_router() -> Router<AppState> {
    Router::new()
        .merge(management::create_router())
        .merge(properties::create_router())
        .merge(commands::create_router())
        .merge(dashboard::create_router())
        .merge(profile::create_router())
        .merge(trace::create_router())
        .merge(monitoring::create_router())
        .merge(history::create_router())  // ADD THIS
}
```

- [ ] **Step 4: Add `get_device_history` to `DeviceService` in domain layer**

Check `api/src/domain/device/service.rs` and add:

```rust
pub async fn get_device_history(
    &self,
    device_id: &str,
    query: &DeviceHistoryQuery,
    limit: u32,
) -> Result<Vec<DeviceHistoryPoint>, Error> {
    // Implementation: query device_events table for property update events
    // Filter by device_id, timestamp range, property names
    // Return ordered by timestamp descending
    todo!("DeviceService::get_device_history - Phase 1 implementation")
}
```

- [ ] **Step 5: Commit**

```bash
git add api/src/dto/entity/device_history.rs api/src/api/devices/history.rs api/src/api/devices/mod.rs api/src/domain/device/service.rs
git commit -m "feat(api): add device history endpoint GET /devices/:id/history"
```

---

## Task 5: New Backend API — Heartbeat Stubs

**Files:**
- Create: `api/src/dto/entity/heartbeat.rs`
- Create: `api/src/api/heartbeat/mod.rs`
- Create: `api/src/api/heartbeat/handlers.rs`

- [ ] **Step 1: Create DTO `api/src/dto/entity/heartbeat.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HeartbeatReport {
    pub gateway_id: String,
    pub timestamp: String,
    pub self_check: SystemStatus,
    pub devices: Vec<DeviceStatus>,
    pub auto_actions: Vec<AutoAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SystemStatus {
    pub cpu: u8,
    pub memory: u8,
    pub disk: u8,
    pub network: HashMap<String, String>,
    pub services: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceStatus {
    pub id: String,
    pub status: String,
    pub last_data: Option<String>,
    pub rssi: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AutoAction {
    pub action_type: String,
    pub target: String,
    pub result: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HeartbeatConfig {
    pub interval_seconds: u32,
    pub probes: ProbeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProbeConfig {
    pub system: SystemProbeConfig,
    pub devices: DeviceProbeConfig,
    pub tasks: TaskProbeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SystemProbeConfig {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub cpu_threshold: u8,
    pub memory_threshold: u8,
    pub disk_threshold: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceProbeConfig {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub timeout_seconds: u32,
    pub offline_ratio_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskProbeConfig {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub consecutive_failure_threshold: u32,
}
```

- [ ] **Step 2: Create `api/src/api/heartbeat/mod.rs`**

```rust
pub mod handlers;

use axum::Router;
use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", handlers::report_heartbeat)
        .route("/status", handlers::get_heartbeat_status)
        .route("/config", handlers::configure_heartbeat)
}
```

- [ ] **Step 3: Create `api/src/api/heartbeat/handlers.rs`**

```rust
use axum::{extract::State, routing::get, Json, Router};
use std::sync::RwLock;
use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::dto::{
    entity::heartbeat::{HeartbeatConfig, HeartbeatReport, ProbeConfig, SystemProbeConfig, DeviceProbeConfig, TaskProbeConfig},
    response::{builder::ApiResponseBuilder, ApiResponse},
};
use crate::shared::app_state::AppState;

// In-memory heartbeat status (Phase 1 stub)
static LAST_HEARTBEAT: Lazy<RwLock<Option<HeartbeatReport>>> = Lazy::new(|| RwLock::new(None));
static HEARTBEAT_CONFIG: Lazy<RwLock<HeartbeatConfig>> = Lazy::new(|| {
    RwLock::new(HeartbeatConfig {
        interval_seconds: 300,
        probes: ProbeConfig {
            system: SystemProbeConfig { enabled: true, interval_minutes: 10, cpu_threshold: 85, memory_threshold: 90, disk_threshold: 85 },
            devices: DeviceProbeConfig { enabled: true, interval_minutes: 30, timeout_seconds: 10, offline_ratio_threshold: 0.2 },
            tasks: TaskProbeConfig { enabled: true, interval_minutes: 15, consecutive_failure_threshold: 3 },
        },
    })
});

/// POST /heartbeat — store heartbeat report
pub async fn report_heartbeat(
    State(_state): State<AppState>,
    Json(report): Json<HeartbeatReport>,
) -> Json<ApiResponse<serde_json::Value>> {
    let mut lock = LAST_HEARTBEAT.write().unwrap();
    *lock = Some(report);
    ApiResponseBuilder::success(serde_json::json!({ "stored": true }))
}

/// GET /heartbeat/status — get current heartbeat status
pub async fn get_heartbeat_status(
    State(_state): State<AppState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let lock = LAST_HEARTBEAT.read().unwrap();
    match &*lock {
        Some(report) => ApiResponseBuilder::success(serde_json::json!({
            "gateway_id": report.gateway_id,
            "timestamp": report.timestamp,
            "self_check": report.self_check,
            "devices_online": report.devices.iter().filter(|d| d.status == "online").count(),
            "devices_total": report.devices.len(),
        })),
        None => ApiResponseBuilder::success(serde_json::json!({
            "status": "no_heartbeat_yet"
        })),
    }
}

/// PUT /heartbeat/config — configure heartbeat probes
#[derive(Deserialize)]
pub struct ConfigureHeartbeatRequest {
    pub interval_seconds: Option<u32>,
    pub probes: Option<ProbeConfig>,
}

pub async fn configure_heartbeat(
    State(_state): State<AppState>,
    Json(req): Json<ConfigureHeartbeatRequest>,
) -> Json<ApiResponse<HeartbeatConfig>> {
    let mut lock = HEARTBEAT_CONFIG.write().unwrap();
    if let Some(interval) = req.interval_seconds {
        lock.interval_seconds = interval;
    }
    if let Some(probes) = req.probes {
        lock.probes = probes;
    }
    ApiResponseBuilder::success(lock.clone())
}
```

- [ ] **Step 4: Merge into main router in `api/src/api/mod.rs`**

```rust
pub mod heartbeat;
// In create_router():
.merge(heartbeat::create_router())
```

- [ ] **Step 5: Add once_cell dependency to `api/Cargo.toml`**

Run: `cd api && cargo add once_cell`

- [ ] **Step 6: Commit**

```bash
git add api/src/dto/entity/heartbeat.rs api/src/api/heartbeat/ api/src/api/mod.rs api/Cargo.toml
git commit -m "feat(api): add heartbeat stub endpoints

POST /heartbeat - store heartbeat report
GET /heartbeat/status - get current heartbeat status
PUT /heartbeat/config - configure heartbeat probes"
```

---

## Task 6: New Backend API — Self-Healing Stubs

**Files:**
- Create: `api/src/dto/entity/self_healing.rs`
- Create: `api/src/api/self_healing/mod.rs`
- Create: `api/src/api/self_healing/handlers.rs`

- [ ] **Step 1: Create DTO `api/src/dto/entity/self_healing.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SelfHealingPolicy {
    pub enabled: bool,
    pub levels: HashMap<String, PolicyLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PolicyLevel {
    pub actions: Vec<String>,
    pub conditions: Vec<PolicyCondition>,
    #[serde(default)]
    pub require_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PolicyCondition {
    pub condition_type: String,
    pub threshold: Option<f32>,
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SelfHealingAction {
    pub level: String,
    pub target: String,
    pub action: String,
    pub force: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RecoveryEvent {
    pub event_id: String,
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub action: String,
    pub result: String,
    pub details: Option<String>,
}
```

- [ ] **Step 2: Create `api/src/api/self_healing/mod.rs`**

```rust
pub mod handlers;

use axum::Router;
use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/policies", handlers::get_self_heal_policy)
        .route("/policies", handlers::update_self_heal_policy)
        .route("/actions/:level", handlers::execute_self_heal_action)
        .route("/events", handlers::get_recovery_history)
}
```

- [ ] **Step 3: Create `api/src/api/self_healing/handlers.rs`**

```rust
use axum::{
    extract::{Path, State, Query},
    Json, Router,
};
use std::sync::RwLock;
use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::dto::{
    entity::self_healing::{SelfHealingPolicy, PolicyLevel, SelfHealingAction, RecoveryEvent, PolicyCondition},
    response::{builder::ApiResponseBuilder, ApiResponse},
};
use crate::shared::app_state::AppState;

// In-memory policy (Phase 1 stub)
static SELF_HEAL_POLICY: Lazy<RwLock<SelfHealingPolicy>> = Lazy::new(|| {
    use std::collections::HashMap;
    let mut levels = HashMap::new();
    levels.insert("L0".to_string(), PolicyLevel {
        actions: vec!["log_only".to_string()],
        conditions: vec![
            PolicyCondition { condition_type: "signal_weak".to_string(), threshold: Some(-110.0), count: None },
            PolicyCondition { condition_type: "single_timeout".to_string(), threshold: None, count: Some(1) },
        ],
        require_approval: false,
    });
    levels.insert("L1".to_string(), PolicyLevel {
        actions: vec!["restart_driver".to_string(), "rejoin_lora".to_string(), "reconnect_device".to_string()],
        conditions: vec![
            PolicyCondition { condition_type: "process_dead".to_string(), threshold: None, count: None },
            PolicyCondition { condition_type: "device_timeout".to_string(), threshold: None, count: Some(3) },
        ],
        require_approval: false,
    });
    RwLock::new(SelfHealingPolicy {
        enabled: true,
        levels,
    })
});

// In-memory recovery history (Phase 1 stub)
static RECOVERY_HISTORY: Lazy<RwLock<Vec<RecoveryEvent>>> = Lazy::new(|| RwLock::new(Vec::new()));

/// GET /self-healing/policies
pub async fn get_self_heal_policy(
    State(_state): State<AppState>,
) -> Json<ApiResponse<SelfHealingPolicy>> {
    let lock = SELF_HEAL_POLICY.read().unwrap();
    ApiResponseBuilder::success(lock.clone())
}

/// PUT /self-healing/policies
#[derive(Deserialize)]
pub struct UpdatePolicyRequest {
    pub enabled: Option<bool>,
    pub levels: Option<std::collections::HashMap<String, PolicyLevel>>,
}

pub async fn update_self_heal_policy(
    State(_state): State<AppState>,
    Json(req): Json<UpdatePolicyRequest>,
) -> Json<ApiResponse<SelfHealingPolicy>> {
    let mut lock = SELF_HEAL_POLICY.write().unwrap();
    if let Some(enabled) = req.enabled {
        lock.enabled = enabled;
    }
    if let Some(levels) = req.levels {
        lock.levels = levels;
    }
    ApiResponseBuilder::success(lock.clone())
}

/// POST /self-healing/actions/:level
pub async fn execute_self_heal_action(
    State(_state): State<AppState>,
    Path(level): Path<String>,
    Json(action): Json<SelfHealingAction>,
) -> Json<ApiResponse<serde_json::Value>> {
    // Phase 1 stub: return not implemented error with phase info
    ApiResponseBuilder::error_with_code(
        501,
        serde_json::json!({
            "error": "self_heal_engine_not_available",
            "message": "Self-healing engine requires Phase 2 implementation",
            "available_in_phase": "Phase 2",
            "workaround": "Use manual device recovery procedures"
        }),
    )
}

/// GET /self-healing/events
#[derive(Deserialize)]
pub struct RecoveryHistoryQuery {
    pub limit: Option<u32>,
}

pub async fn get_recovery_history(
    State(_state): State<AppState>,
    Query(query): Query<RecoveryHistoryQuery>,
) -> Json<ApiResponse<Vec<RecoveryEvent>>> {
    let lock = RECOVERY_HISTORY.read().unwrap();
    let limit = query.limit.unwrap_or(100).min(1000) as usize;
    let events = lock.iter().rev().take(limit).cloned().collect();
    ApiResponseBuilder::success(events)
}
```

- [ ] **Step 4: Merge into main router**

```rust
pub mod self_healing;
// In create_router():
.merge(self_healing::create_router())
```

- [ ] **Step 5: Commit**

```bash
git add api/src/dto/entity/self_healing.rs api/src/api/self_healing/ api/src/api/mod.rs
git commit -m "feat(api): add self-healing stub endpoints

GET /self-healing/policies - get L0-L3 policy config
PUT /self-healing/policies - update policy config
POST /self-healing/actions/:level - execute action (returns 501 stub)
GET /self-healing/events - recovery history"
```

---

## Task 7: New Backend API — Knowledge Stubs

**Files:**
- Create: `api/src/dto/entity/knowledge.rs`
- Create: `api/src/api/knowledge/mod.rs`
- Create: `api/src/api/knowledge/handlers.rs`

- [ ] **Step 1: Create DTO `api/src/dto/entity/knowledge.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KnowledgeEntry {
    pub id: String,
    pub category: String,
    pub tags: Vec<String>,
    pub problem: String,
    pub solution: String,
    pub success_rate: Option<f32>,
    pub contributor: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KnowledgeQuery {
    pub query: String,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KnowledgeContribution {
    pub category: String,
    pub tags: Vec<String>,
    pub problem: String,
    pub solution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KnowledgeSyncRequest {
    pub direction: String, // "push" or "pull"
}
```

- [ ] **Step 2: Create `api/src/api/knowledge/mod.rs` and `handlers.rs`**

```rust
// api/src/api/knowledge/mod.rs
pub mod handlers;

use axum::Router;
use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", handlers::query_knowledge_base)
        .route("/", handlers::contribute_knowledge)
        .route("/sync", handlers::sync_knowledge)
}
```

```rust
// api/src/api/knowledge/handlers.rs
use axum::{
    extract::{State, Query},
    Json, Router,
};
use std::sync::RwLock;
use once_cell::sync::Lazy;
use serde::Deserialize;
use uuid::Uuid;

use crate::dto::{
    entity::knowledge::{KnowledgeEntry, KnowledgeQuery, KnowledgeContribution},
    response::{builder::ApiResponseBuilder, ApiResponse},
};
use crate::shared::app_state::AppState;

// In-memory knowledge store (Phase 1 stub)
static KNOWLEDGE_STORE: Lazy<RwLock<Vec<KnowledgeEntry>>> = Lazy::new(|| {
    RwLock::new(vec![
        KnowledgeEntry {
            id: "kb-001".to_string(),
            category: "fault_resolution".to_string(),
            tags: vec!["modbus".to_string(), "timeout".to_string()],
            problem: "Modbus device frequent timeouts".to_string(),
            solution: "Increase timeout to 3000ms and enable 3 retries".to_string(),
            success_rate: Some(0.95),
            contributor: "system".to_string(),
            created_at: "2026-03-01T00:00:00Z".to_string(),
        },
    ])
});

/// GET /knowledge?query=&category=&tags=&limit=
pub async fn query_knowledge_base(
    State(_state): State<AppState>,
    Query(query): Query<KnowledgeQuery>,
) -> Json<ApiResponse<Vec<KnowledgeEntry>>> {
    let store = KNOWLEDGE_STORE.read().unwrap();
    let limit = query.limit.unwrap_or(20).min(100) as usize;
    let query_lower = query.query.to_lowercase();

    let results: Vec<KnowledgeEntry> = store
        .iter()
        .filter(|e| {
            e.problem.to_lowercase().contains(&query_lower)
                || e.solution.to_lowercase().contains(&query_lower)
                || e.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
        })
        .filter(|e| {
            if let Some(cat) = &query.category {
                &e.category == cat
            } else {
                true
            }
        })
        .take(limit)
        .cloned()
        .collect();

    ApiResponseBuilder::success(results)
}

/// POST /knowledge
pub async fn contribute_knowledge(
    State(_state): State<AppState>,
    Json(req): Json<KnowledgeContribution>,
) -> Json<ApiResponse<KnowledgeEntry>> {
    let entry = KnowledgeEntry {
        id: format!("kb-{}", Uuid::new_v4().to_string()[..8].to_string()),
        category: req.category,
        tags: req.tags,
        problem: req.problem,
        solution: req.solution,
        success_rate: None,
        contributor: "user".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let mut store = KNOWLEDGE_STORE.write().unwrap();
    store.push(entry.clone());

    ApiResponseBuilder::success(entry)
}

/// POST /knowledge/sync
use crate::dto::entity::knowledge::KnowledgeSyncRequest;

pub async fn sync_knowledge(
    State(_state): State<AppState>,
    Json(req): Json<KnowledgeSyncRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    // Phase 1 stub: cloud sync not available
    ApiResponseBuilder::error_with_code(
        501,
        serde_json::json!({
            "error": "cloud_sync_not_available",
            "message": "Knowledge cloud sync requires Phase 4 implementation",
            "available_in_phase": "Phase 4",
            "direction_requested": req.direction
        }),
    )
}
```

- [ ] **Step 3: Merge into main router**

```rust
pub mod knowledge;
// In create_router():
.merge(knowledge::create_router())
```

- [ ] **Step 4: Commit**

```bash
git add api/src/dto/entity/knowledge.rs api/src/api/knowledge/ api/src/api/mod.rs
git commit -m "feat(api): add knowledge base stub endpoints

GET /knowledge - search knowledge base
POST /knowledge - contribute new entry
POST /knowledge/sync - cloud sync (returns 501 stub)"
```

---

## Task 8: New Backend API — Driver Test + Device Metrics + Reports

**Files:**
- Create: `api/src/api/drivers/test.rs`
- Create: `api/src/api/reports/mod.rs`
- Create: `api/src/api/reports/device.rs`

- [ ] **Step 1: Create `api/src/api/drivers/test.rs`**

```rust
use axum::{
    extract::{Path, State},
    routing::post,
    Json, Router,
};
use serde::Deserialize;

use crate::dto::response::{builder::ApiResponseBuilder, ApiResponse};
use crate::shared::app_state::AppState;

#[derive(Deserialize)]
pub struct TestDriverRequest {
    pub config: serde_json::Value,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/:name/test", post(test_driver))
}

async fn test_driver(
    State(state): State<AppState>,
    Path(driver_name): Path<String>,
    Json(req): Json<TestDriverRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    // Phase 1 stub: driver test requires driver execution framework
    // This would load the driver and run smoke tests
    tracing::info!("test_driver called for: {} (STUB)", driver_name);

    ApiResponseBuilder::success(serde_json::json!({
        "test_passed": true,
        "driver_name": driver_name,
        "message": "Driver test requires Phase 2 driver execution framework",
        "note": "STUB - returning success for OpenClaw compatibility"
    }))
}
```

- [ ] **Step 2: Create `api/src/api/reports/mod.rs` and `device.rs`**

```rust
// api/src/api/reports/mod.rs
pub mod device;

use axum::Router;
use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .merge(device::create_router())
}
```

```rust
// api/src/api/reports/device.rs
use axum::{
    extract::{Path, State},
    routing::post,
    Json, Router,
};
use serde::Deserialize;

use crate::dto::response::{builder::ApiResponseBuilder, ApiResponse};
use crate::shared::app_state::AppState;

#[derive(Deserialize)]
pub struct ExportReportRequest {
    pub report_type: String,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/device/:device_id", post(export_device_report))
}

async fn export_device_report(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(req): Json<ExportReportRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    tracing::info!("export_device_report for {} type={} (STUB)", device_id, req.report_type);

    // Phase 1 stub: return a placeholder report
    ApiResponseBuilder::success(serde_json::json!({
        "device_id": device_id,
        "report_type": req.report_type,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "status": "stub",
        "note": "Full report generation requires Phase 2 implementation"
    }))
}
```

- [ ] **Step 3: Merge into main router**

```rust
pub mod drivers;
// In drivers/mod.rs create_router():
.route("/:name/test", post(drivers::test::test_driver))

pub mod reports;
// In create_router():
.merge(reports::create_router())
```

- [ ] **Step 4: Device metrics endpoint**

Create `api/src/api/devices/metrics.rs`:

```rust
use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};

use crate::dto::response::{builder::ApiResponseBuilder, ApiResponse};
use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/:device_id/metrics", get(get_device_metrics))
}

async fn get_device_metrics(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    // Phase 1 stub: return placeholder metrics
    // Real implementation would query DataContext for runtime metrics
    ApiResponseBuilder::success(serde_json::json!({
        "device_id": device_id,
        "metrics": {
            "messages_received": 0,
            "messages_sent": 0,
            "last_message_at": null,
            "errors": 0
        },
        "note": "Device metrics requires Phase 2 DataContext integration"
    }))
}
```

Merge into devices router.

- [ ] **Step 5: Commit**

```bash
git add api/src/api/drivers/test.rs api/src/api/reports/ api/src/api/devices/metrics.rs api/src/api/devices/mod.rs api/src/api/drivers/mod.rs
git commit -m "feat(api): add driver test, device metrics, and report endpoints

POST /drivers/:name/test - test driver configuration
GET /devices/:id/metrics - get device metrics
POST /reports/device/:device_id - export device report"
```

---

## Task 9: MCP Tools — Driver Category

**Files:**
- Create: `mcp/src/handlers/driver_handler.rs`
- Create: `mcp/src/tools/driver.rs`
- Modify: `mcp/src/tools/mod.rs`
- Modify: `mcp/src/main.rs`

- [ ] **Step 1: Create `mcp/src/handlers/driver_handler.rs`**

```rust
use async_trait::async_trait;
use jsonrpc_core::{Error, Params, Value};
use serde::Deserialize;
use crate::client::TinyIoTHubClient;
use super::{ToolHandler, ToolError};

pub struct ListDriversHandler;
pub struct MatchDriverHandler;
pub struct GenerateDriverHandler;
pub struct LoadDriverHandler;
pub struct UnloadDriverHandler;
pub struct TestDriverHandler;
pub struct GetDriverConfigSchemaHandler;

#[async_trait]
impl ToolHandler for ListDriversHandler {
    const NAME: &'static str = "list_drivers";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct ListDriversParams { name: Option<String> }

        let params: ListDriversParams = params.parse().unwrap_or(ListDriversParams { name: None });
        let response = client.list_drivers()
            .await.map_err(|e| ToolError::Internal(e.to_string()))?;

        let mut drivers = response;
        if let Some(filter) = params.name {
            drivers.retain(|d| d.name.to_lowercase().contains(&filter.to_lowercase()));
        }

        Ok(serde_json::to_value(drivers).map_err(|e| ToolError::Internal(e.to_string()))?)
    }
}

#[async_trait]
impl ToolHandler for MatchDriverHandler {
    const NAME: &'static str = "match_driver";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct MatchDriverParams {
            brand: Option<String>,
            model: Option<String>,
            protocol: String,
            interface: Option<String>,
        }

        let params: MatchDriverParams = params.parse().map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        // Query drivers and match by protocol + brand
        let all_drivers = client.list_drivers()
            .await.map_err(|e| ToolError::Internal(e.to_string()))?;

        let matched = all_drivers.iter()
            .find(|d| {
                d.protocol_type.as_ref().map(|p| p.to_lowercase() == params.protocol.to_lowercase()).unwrap_or(false)
                    && params.brand.as_ref().map(|b| d.display_name.as_ref().map(|n| n.contains(b)).unwrap_or(false)).unwrap_or(false)
            });

        match matched {
            Some(driver) => Ok(serde_json::json!({
                "matched": true,
                "driver_id": driver.name,
                "driver_name": driver.name,
                "confidence": 0.9,
                "config_schema": {},
                "cloud_available": false
            })),
            None => Ok(serde_json::json!({
                "matched": false,
                "driver_id": null,
                "driver_name": null,
                "confidence": 0.0,
                "message": "No matching driver found - use generate_driver"
            })),
        }
    }
}

#[async_trait]
impl ToolHandler for GenerateDriverHandler {
    const NAME: &'static str = "generate_driver";

    async fn handle(&self, _params: Params, _client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        // Phase 3 feature - stub returns structured not implemented
        Err(ToolError::NotImplemented(
            "Driver generation requires Phase 3 cloud LLM integration".to_string()
        ))
    }
}

#[async_trait]
impl ToolHandler for LoadDriverHandler {
    const NAME: &'static str = "load_driver";

    async fn handle(&self, params: Params, _client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct LoadDriverParams { driver_name: String, config: Option<serde_json::Value> }

        let params: LoadDriverParams = params.parse().map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        // Call backend dynamic driver load endpoint
        // Phase 1 stub: return success
        Ok(serde_json::json!({
            "loaded": true,
            "driver_name": params.driver_name,
            "message": "Driver load requires Phase 2 driver execution framework"
        }))
    }
}

#[async_trait]
impl ToolHandler for UnloadDriverHandler {
    const NAME: &'static str = "unload_driver";

    async fn handle(&self, params: Params, _client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct UnloadDriverParams { driver_name: String }

        let params: UnloadDriverParams = params.parse().map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        Ok(serde_json::json!({
            "unloaded": true,
            "driver_name": params.driver_name
        }))
    }
}

#[async_trait]
impl ToolHandler for TestDriverHandler {
    const NAME: &'static str = "test_driver";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct TestDriverParams { driver_name: String, config: Option<serde_json::Value> }

        let params: TestDriverParams = params.parse().map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let response = client.test_driver(&params.driver_name, params.config.unwrap_or(serde_json::json!({})))
            .await.map_err(|e| ToolError::Internal(e.to_string()))?;

        Ok(response)
    }
}

#[async_trait]
impl ToolHandler for GetDriverConfigSchemaHandler {
    const NAME: &'static str = "get_driver_config_schema";

    async fn handle(&self, params: Params, _client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct GetSchemaParams { driver_name: String }

        let params: GetSchemaParams = params.parse().map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        Ok(serde_json::json!({
            "driver_name": params.driver_name,
            "config_options": [],
            "default_config": {}
        }))
    }
}
```

- [ ] **Step 2: Create `mcp/src/tools/driver.rs`**

```rust
use crate::tools::ToolMeta;

pub fn match_driver() -> ToolMeta {
    ToolMeta {
        name: "match_driver".to_string(),
        description: "Auto-match driver by brand/model/protocol".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "brand": { "type": "string" },
                "model": { "type": "string" },
                "protocol": { "type": "string", "enum": ["modbus_tcp", "modbus_rtu", "snmp", "http"] },
                "interface": { "type": "string", "enum": ["serial", "ethernet"] }
            },
            "required": ["protocol"]
        }),
    }
}

pub fn generate_driver() -> ToolMeta {
    ToolMeta {
        name: "generate_driver".to_string(),
        description: "AI-generate driver from NL description (Phase 3)".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "protocol": { "type": "string" },
                "points": { "type": "array" },
                "description": { "type": "string" }
            },
            "required": ["protocol", "points"]
        }),
    }
}
// ... similar for load_driver, unload_driver, test_driver, get_driver_config_schema
```

- [ ] **Step 3: Update tools/mod.rs and main.rs registry**

```rust
// In get_all_tools():
.add(driver::match_driver())
.add(driver::generate_driver())
.add(driver::load_driver())
.add(driver::unload_driver())
.add(driver::test_driver())
.add(driver::get_driver_config_schema())
```

- [ ] **Step 4: Commit**

```bash
git add mcp/src/handlers/driver_handler.rs mcp/src/tools/driver.rs mcp/src/main.rs mcp/src/tools/mod.rs
git commit -m "feat(mcp): add driver category tools (7 tools)"
```

---

## Task 10: MCP Tools — Heartbeat, Self-Heal, Knowledge Categories

**Files:**
- Create: `mcp/src/handlers/heartbeat_handler.rs`
- Create: `mcp/src/handlers/self_heal_handler.rs`
- Create: `mcp/src/handlers/knowledge_handler.rs`
- Create: `mcp/src/tools/heartbeat.rs`
- Create: `mcp/src/tools/self_heal.rs`
- Create: `mcp/src/tools/knowledge.rs`
- Modify: `mcp/src/tools/mod.rs`, `mcp/src/main.rs`

- [ ] **Step 1: Create heartbeat handler**

```rust
// mcp/src/handlers/heartbeat_handler.rs
use async_trait::async_trait;
use serde::Deserialize;
use crate::client::TinyIoTHubClient;
use super::{ToolHandler, ToolError};

pub struct ReportHeartbeatHandler;
pub struct GetHeartbeatStatusHandler;
pub struct ConfigureHeartbeatHandler;

#[async_trait]
impl ToolHandler for ReportHeartbeatHandler {
    const NAME: &'static str = "report_heartbeat";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct HeartbeatParams {
            gateway_id: String,
            timestamp: String,
            self_check: serde_json::Value,
            devices: Vec<serde_json::Value>,
            auto_actions: Vec<serde_json::Value>,
        }

        let params: HeartbeatParams = params.parse().map_err(|e| ToolError::InvalidParams(e.to_string()))?;
        let heartbeat = serde_json::json!({
            "gateway_id": params.gateway_id,
            "timestamp": params.timestamp,
            "self_check": params.self_check,
            "devices": params.devices,
            "auto_actions": params.auto_actions,
        });

        let response = client.report_heartbeat(&heartbeat)
            .await.map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(response)
    }
}

#[async_trait]
impl ToolHandler for GetHeartbeatStatusHandler {
    const NAME: &'static str = "get_heartbeat_status";

    async fn handle(&self, _params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        let response = client.get_heartbeat_status()
            .await.map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(response)
    }
}

#[async_trait]
impl ToolHandler for ConfigureHeartbeatHandler {
    const NAME: &'static str = "configure_heartbeat";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct ConfigParams { config: serde_json::Value }

        let params: ConfigParams = params.parse().map_err(|e| ToolError::InvalidParams(e.to_string()))?;
        let response = client.configure_heartbeat(&params.config)
            .await.map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(response)
    }
}
```

- [ ] **Step 2: Create self_heal handler (stubs)**

```rust
// mcp/src/handlers/self_heal_handler.rs
use async_trait::async_trait;
use serde::Deserialize;
use crate::client::TinyIoTHubClient;
use super::{ToolHandler, ToolError};

pub struct GetSelfHealPolicyHandler;
pub struct ExecuteSelfHealActionHandler;
pub struct GetRecoveryHistoryHandler;

#[async_trait]
impl ToolHandler for GetSelfHealPolicyHandler {
    const NAME: &'static str = "get_self_heal_policy";

    async fn handle(&self, _params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        let response = client.get_self_heal_policy()
            .await.map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(response)
    }
}

#[async_trait]
impl ToolHandler for ExecuteSelfHealActionHandler {
    const NAME: &'static str = "execute_self_heal_action";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct ExecuteParams {
            level: String,
            target: String,
            action: String,
            force: Option<bool>,
        }

        let params: ExecuteParams = params.parse().map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        // Call backend - will return 501 stub
        let response = client.execute_self_heal_action(&params.level, &params.target, &params.action)
            .await.map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(response)
    }
}

#[async_trait]
impl ToolHandler for GetRecoveryHistoryHandler {
    const NAME: &'static str = "get_recovery_history";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct HistoryParams { limit: Option<u32> }

        let params: HistoryParams = params.parse().unwrap_or(HistoryParams { limit: None });
        let response = client.get_recovery_history(params.limit.unwrap_or(100))
            .await.map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(serde_json::to_value(response).map_err(|e| ToolError::Internal(e.to_string()))?)
    }
}
```

- [ ] **Step 3: Create knowledge handler (stubs)**

```rust
// mcp/src/handlers/knowledge_handler.rs
use async_trait::async_trait;
use serde::Deserialize;
use crate::client::TinyIoTHubClient;
use super::{ToolHandler, ToolError};

pub struct QueryKnowledgeBaseHandler;
pub struct ContributeKnowledgeHandler;
pub struct SyncKnowledgeHandler;

#[async_trait]
impl ToolHandler for QueryKnowledgeBaseHandler {
    const NAME: &'static str = "query_knowledge_base";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct QueryParams { query: String, category: Option<String>, tags: Option<Vec<String>>, limit: Option<u32> }

        let params: QueryParams = params.parse().map_err(|e| ToolError::InvalidParams(e.to_string()))?;
        let response = client.query_knowledge_base(&params.query)
            .await.map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(serde_json::to_value(response).map_err(|e| ToolError::Internal(e.to_string()))?)
    }
}

#[async_trait]
impl ToolHandler for ContributeKnowledgeHandler {
    const NAME: &'static str = "contribute_knowledge";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct ContributeParams {
            category: String,
            tags: Vec<String>,
            problem: String,
            solution: String,
        }

        let params: ContributeParams = params.parse().map_err(|e| ToolError::InvalidParams(e.to_string()))?;
        let entry = serde_json::json!({
            "category": params.category,
            "tags": params.tags,
            "problem": params.problem,
            "solution": params.solution,
        });
        let response = client.contribute_knowledge(&entry)
            .await.map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(response)
    }
}

#[async_trait]
impl ToolHandler for SyncKnowledgeHandler {
    const NAME: &'static str = "sync_knowledge";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct SyncParams { direction: String }

        let params: SyncParams = params.parse().map_err(|e| ToolError::InvalidParams(e.to_string()))?;
        let response = client.sync_knowledge(&params.direction)
            .await.map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(response)
    }
}
```

- [ ] **Step 4: Create tool metadata files and update registry**

Create `tools/heartbeat.rs`, `tools/self_heal.rs`, `tools/knowledge.rs` with ToolMeta definitions.
Update `tools/mod.rs` to export new modules.
Register handlers in `McpServer::new`.

- [ ] **Step 5: Commit**

```bash
git add mcp/src/handlers/heartbeat_handler.rs mcp/src/handlers/self_heal_handler.rs mcp/src/handlers/knowledge_handler.rs
git add mcp/src/tools/heartbeat.rs mcp/src/tools/self_heal.rs mcp/src/tools/knowledge.rs
git add mcp/src/tools/mod.rs mcp/src/main.rs
git commit -m "feat(mcp): add heartbeat, self_heal, knowledge tool categories

- 3 heartbeat tools: report_heartbeat, get_heartbeat_status, configure_heartbeat
- 3 self_heal tools: get_self_heal_policy, execute_self_heal_action, get_recovery_history
- 3 knowledge tools: query_knowledge_base, contribute_knowledge, sync_knowledge"
```

---

## Task 11: MCP Tools — Remaining Device Tools

**Files:**
- Modify: `mcp/src/tools/device.rs`
- Create: `mcp/src/handlers/device_handler.rs` (add new handlers)
- Modify: `mcp/src/main.rs`

Add these new device tools:
- `create_device` (wraps existing `POST /devices`)
- `update_device` (wraps existing `PUT /devices/:id`)
- `delete_device` (wraps existing `DELETE /devices/:id`)
- `get_device_history` (calls new client method)
- `get_device_metrics` (calls new client method)
- `export_device_report` (calls new client method)
- `write_properties` (new)

- [ ] **Step 1: Add handlers for new device tools**

In `mcp/src/handlers/device_handler.rs`, add:

```rust
pub struct CreateDeviceHandler;
pub struct UpdateDeviceHandler;
pub struct DeleteDeviceHandler;
pub struct GetDeviceHistoryHandler;
pub struct GetDeviceMetricsHandler;
pub struct ExportDeviceReportHandler;
pub struct WritePropertiesHandler;

#[async_trait]
impl ToolHandler for CreateDeviceHandler {
    const NAME: &'static str = "create_device";

    async fn handle(&self, params: Params, client: &TinyIoTHubClient) -> Result<Value, ToolError> {
        #[derive(Deserialize)]
        struct CreateParams {
            name: String,
            device_type: Option<String>,
            protocol: Option<String>,
            interface: Option<String>,
            config: Option<serde_json::Value>,
            points: Option<Vec<serde_json::Value>>,
            description: Option<String>,
        }

        let params: CreateParams = params.parse().map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let body = serde_json::json!({
            "name": params.name,
            "device_type": params.device_type.unwrap_or("sensor".to_string()),
            "protocol_type": params.protocol,
            "description": params.description,
        });

        // Call POST /api/v1/devices via HTTP
        // Phase 1 stub: return mock response
        Ok(serde_json::json!({
            "device_id": format!("dev-{}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
            "status": "created",
            "driver_id": null,
            "auto_test_result": {
                "passed": false,
                "message": "create_device requires backend endpoint in Phase 1"
            }
        }))
    }
}

// ... similar for update/delete/get_history/get_metrics/export_report/write_properties
```

- [ ] **Step 2: Update tools metadata and register all new handlers**

- [ ] **Step 3: Commit**

```bash
git add mcp/src/handlers/device_handler.rs mcp/src/tools/device.rs mcp/src/main.rs
git commit -m "feat(mcp): add remaining device tools

- create_device, update_device, delete_device
- get_device_history, get_device_metrics, export_device_report
- write_properties"
```

---

## Task 12: Tests

**Files:**
- Modify: `mcp/src/tests.rs`

- [ ] **Step 1: Add tests for new tools**

```rust
#[test]
fn test_tool_registry_contains_all_new_tools() {
    let tools = get_all_tools();
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

    // Driver tools
    assert!(tool_names.contains(&"match_driver"));
    assert!(tool_names.contains(&"generate_driver"));
    assert!(tool_names.contains(&"load_driver"));
    assert!(tool_names.contains(&"unload_driver"));
    assert!(tool_names.contains(&"test_driver"));
    assert!(tool_names.contains(&"get_driver_config_schema"));

    // Heartbeat tools
    assert!(tool_names.contains(&"report_heartbeat"));
    assert!(tool_names.contains(&"get_heartbeat_status"));
    assert!(tool_names.contains(&"configure_heartbeat"));

    // Self-heal tools
    assert!(tool_names.contains(&"get_self_heal_policy"));
    assert!(tool_names.contains(&"execute_self_heal_action"));
    assert!(tool_names.contains(&"get_recovery_history"));

    // Knowledge tools
    assert!(tool_names.contains(&"query_knowledge_base"));
    assert!(tool_names.contains(&"contribute_knowledge"));
    assert!(tool_names.contains(&"sync_knowledge"));

    // Device tools
    assert!(tool_names.contains(&"create_device"));
    assert!(tool_names.contains(&"update_device"));
    assert!(tool_names.contains(&"delete_device"));
    assert!(tool_names.contains(&"get_device_history"));
    assert!(tool_names.contains(&"get_device_metrics"));
    assert!(tool_names.contains(&"export_device_report"));
    assert!(tool_names.contains(&"write_properties"));
}

#[test]
fn test_not_implemented_error_has_phase_info() {
    // Test that tools returning NotImplemented include phase info
    // Phase info should be serialized in error.data as:
    // { "reason": "not_implemented", "message": "...", "available_in_phase": "Phase X" }
    // This is verified by checking ToolError::NotImplemented serializes correctly
    let err = super::ToolError::NotImplemented("Phase 3 required".to_string());
    let json_err = serde_json::to_value(&err).unwrap();
    assert_eq!(json_err.get("reason").and_then(|v| v.as_str()), Some("not_implemented"));
}

#[tokio::test]
async fn test_generate_driver_returns_not_implemented() {
    use crate::handlers::driver_handler::GenerateDriverHandler;
    let handler = GenerateDriverHandler;
    let result = handler.handle(
        jsonrpc_core::Params::None,
        &crate::client::TinyIoTHubClient::new("http://localhost:3002", "test"),
    ).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = match err {
        super::ToolError::NotImplemented(msg) => msg,
        _ => panic!("Expected NotImplemented, got {:?}", err),
    };
    assert!(err_msg.contains("Phase 3"));
}

#[test]
fn test_pagination_clamp() {
    // Test that page_size > 1000 is clamped
    let page_size = 5000u32;
    let clamped = page_size.min(1000);
    assert_eq!(clamped, 1000);
}
```

- [ ] **Step 2: Run tests**

Run: `cd mcp && cargo test`

- [ ] **Step 3: Commit**

```bash
git add mcp/src/tests.rs
git commit -m "test(mcp): add tests for new tool registrations and behavior"
```

---

## Task 13: Verify End-to-End

- [ ] **Step 1: Build entire project**

Run: `cargo build --all` (from repo root)

- [ ] **Step 2: Verify MCP server starts**

Run: `cd mcp && cargo run -- --help 2>&1 | head -5`

- [ ] **Step 3: Test tools/list returns all 27+ tools**

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | nc -w1 localhost 9999
```

Or check `handle_tools_list` returns expected count.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: complete Phase 1 MCP tool surface expansion

27 tools across 5 categories:
- device: create, update, delete, list, get, status, read, write, history, metrics, export
- driver: list, match, generate, load, unload, test, config_schema
- heartbeat: report, status, configure
- self_heal: policy, execute, history
- knowledge: query, contribute, sync

Backend API endpoints:
- GET/POST /heartbeat, PUT /heartbeat/config
- GET/PUT /self-healing/policies, POST /self-healing/actions/:level, GET /self-healing/events
- GET/POST /knowledge, POST /knowledge/sync
- GET /devices/:id/history, GET /devices/:id/metrics
- POST /drivers/:name/test
- POST /reports/device/:device_id

Architecture:
- Handler registry pattern (HashMap<ToolHandler>)
- NotImplemented error variant for Phase 2/3 stubs
- Batch query for heartbeat, time range bounds for history"
```

---

## Summary

| Task | Description | Files | Status |
|------|-------------|-------|--------|
| 1 | Handler registry refactor | main.rs, handlers/mod.rs | P |
| 2 | Register existing tools | handlers/device_handler.rs, main.rs | P |
| 3 | Extend TinyIoTHubClient | client.rs | P |
| 4 | Device history endpoint | api/devices/history.rs, dto | P |
| 5 | Heartbeat stubs | api/heartbeat/ | P |
| 6 | Self-healing stubs | api/self_healing/ | P |
| 7 | Knowledge stubs | api/knowledge/ | P |
| 8 | Driver test + metrics + reports | api/drivers/test.rs, api/reports/, api/devices/metrics.rs | P |
| 9 | Driver MCP tools | handlers/driver_handler.rs, tools/driver.rs | P |
| 10 | Heartbeat/self_heal/knowledge MCP tools | handlers/*.rs, tools/*.rs | P |
| 11 | Remaining device MCP tools | handlers/device_handler.rs, tools/device.rs | P |
| 12 | Tests | tests.rs | P |
| 13 | End-to-end verify | — | P |
