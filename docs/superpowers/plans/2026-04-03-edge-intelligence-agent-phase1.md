# TinyIoTHub Edge Intelligence Agent - Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Phase 1 of TinyIoTHub + OpenClaw integration with 27 MCP tools, 4 OpenClaw Skills, and full E2E verification.

**Architecture:** OpenClaw (AI orchestrator) → MCP over HTTP → TinyIoTHub API :3002 → Rust backend. OpenClaw Skills provide prompt templates that guide the AI to use MCP tools for device onboarding, heartbeat management, device status queries, and alarm handling.

**Tech Stack:** Rust (Axum, Tokio, SQLx), OpenClaw Skills, MCP protocol v2024-11-05, JSON-RPC 2.0

---

## 1. Project Status Overview

### 1.1 Completed (✅)

| Component | Location | Status |
|-----------|----------|--------|
| MCP Module Skeleton | `api/src/api/mcp/` | ✅ Done |
| Tool Registry + Handler trait | `tool_registry.rs` | ✅ Done |
| MCP HTTP Handlers | `handlers.rs` | ✅ Done |
| Device Tools (12) | `tools/device.rs` | ✅ Done |
| Driver Tools (7) | `tools/driver.rs` | ✅ Done |
| Heartbeat Tools (3) | `tools/heartbeat.rs` | ✅ Done |
| Self-Heal Tools (3) | `tools/self_heal.rs` | ✅ Done |
| Knowledge Tools (3) | `tools/knowledge.rs` | ✅ Done |
| Heartbeat REST Endpoints | `api/src/api/heartbeat/` | ✅ Done |
| Self-Healing REST Endpoints | `api/src/api/self_healing/` | ✅ Done |
| JWT Auth Middleware | `handlers.rs:102-111` | ✅ Done |

### 1.2 Remaining Work

| Task | Description | Priority |
|------|-------------|----------|
| Task 7 | OpenClaw Skills (4 skills) | P0 |
| Task 8 | Deprecate old MCP crate | P2 |
| Task 9 | MCP Tool Tests | P0 |
| Task 10 | E2E Verification | P0 |

---

## 2. File Structure

### 2.1 OpenClaw Skills (New)

```
skills/tinyiothub/                          # OpenClaw workspace skills
├── skill.yaml                              # Skill metadata
└── prompts/
    ├── device-onboarding.md                 # Device onboarding引导
    ├── heartbeat-query.md                   # Heartbeat查询引导
    ├── device-status.md                    # Device状态引导
    └── alarm-management.md                 # Alarm管理引导
```

### 2.2 Old MCP Crate Deprecation

```
mcp/                                         # Mark as DEPRECATED
├── Cargo.toml                              # Add [deprecation] note
└── src/main.rs                             # Add deprecation warning
```

### 2.3 Tests

```
api/src/api/mcp/
├── tests.rs                                # Existing unit tests
├── tool_registry.rs                        # Add integration tests
└── tools/
    ├── device.rs                          # Add handler tests
    ├── driver.rs                          # Add handler tests
    ├── heartbeat.rs                       # Add handler tests
    ├── self_heal.rs                      # Add handler tests
    └── knowledge.rs                       # Add handler tests
```

---

## 3. Task Details

### Task 7: OpenClaw Skills (P0)

**Files:**
- Create: `skills/tinyiothub/skill.yaml`
- Create: `skills/tinyiothub/prompts/device-onboarding.md`
- Create: `skills/tinyiothub/prompts/heartbeat-query.md`
- Create: `skills/tinyiothub/prompts/device-status.md`
- Create: `skills/tinyiothub/prompts/alarm-management.md`

- [ ] **Step 1: Create skills directory structure**

```bash
mkdir -p skills/tinyiothub/prompts
```

- [ ] **Step 2: Create skill.yaml**

```yaml
name: tinyiothub
version: 1.0.0
description: TinyIoTHub IoT Gateway management via MCP tools
mcp_endpoint: http://localhost:3002/mcp
prompts:
  - device-onboarding
  - heartbeat-query
  - device-status
  - alarm-management
```

- [ ] **Step 3: Create device-onboarding.md**

```markdown
# Device Onboarding Skill

## Purpose
Guide AI to onboard new IoT devices using natural language.

## Trigger
"设备接入", "添加传感器", "连接新设备"

## Flow
1. Use `list_drivers` to find compatible driver
2. Use `match_driver` for auto-matching
3. Use `create_device` with driver config
4. Use `test_driver` to verify
5. Use `report_heartbeat` to register gateway

## Example
User: "串口1接入温湿度传感器"
AI: match_driver(protocol="modbus", device_type="temperature_humidity_sensor")
    → create_device(name="温湿度传感器", driver="modbus_temp_humidity", config={...})
    → test_driver(device_id="xxx")
    → report_heartbeat(gateway_id="gw-001", device={id: "xxx", status: "online"})
```

- [ ] **Step 4: Create heartbeat-query.md**

```markdown
# Heartbeat Query Skill

## Purpose
Guide AI to query and configure gateway heartbeat.

## Trigger
"心跳状态", "网关健康", "系统状态"

## Flow
1. Use `report_heartbeat` to report current state
2. Use `get_heartbeat_status` to query
3. Use `configure_heartbeat` to adjust thresholds

## Example
User: "检查网关心跳"
AI: get_heartbeat_status()
    → { cpu: 45%, memory: 62%, devices: 12, alarms: 2 }
```
```

- [ ] **Step 5: Create device-status.md**

```markdown
# Device Status Skill

## Purpose
Guide AI to query device status and read properties.

## Trigger
"设备状态", "读取数据", "查看传感器"

## Tools
- `list_devices` - 列出所有设备
- `get_device_status` - 获取设备状态
- `read_properties` - 读取属性
- `get_device_history` - 获取历史数据
```

- [ ] **Step 6: Create alarm-management.md**

```markdown
# Alarm Management Skill

## Purpose
Guide AI to handle alarms and notifications.

## Trigger
"告警", "报警处理", "故障"

## Flow
1. Query active alarms via API (not MCP)
2. Use `execute_self_heal_action` for auto-remediation
3. Report resolution via `report_heartbeat`
```

- [ ] **Step 7: Commit**

```bash
git add skills/tinyiothub/
git commit -m "feat(skills): add OpenClaw skills for TinyIoTHub"
```

---

### Task 8: Deprecate Old MCP Crate (P2)

**Files:**
- Modify: `mcp/Cargo.toml` - add deprecation notice
- Modify: `mcp/src/main.rs` - add deprecation warning

- [ ] **Step 1: Add deprecation notice to Cargo.toml**

```toml
# DEPRECATED: This crate is replaced by the embedded MCP server
# in api/src/api/mcp/. All new MCP tools should be added there.
[package]
name = "tinyiothub-mcp"
version = "0.1.0"
authors = ["TinyIoTHub Team"]
deprecated = true
```

- [ ] **Step 2: Add deprecation warning to main.rs**

Add at top of main.rs:
```rust
// DEPRECATED: This binary is replaced by the embedded MCP server
// in api/src/api/mcp/. Please use the API server instead.
// This crate will be removed in v2.0.
fn main() {
    eprintln!("WARNING: This MCP server is deprecated. Use the embedded MCP in api/src/api/mcp/ instead.");
    // ... existing code
}
```

- [ ] **Step 3: Commit**

```bash
git add mcp/
git commit -m "chore(mcp): mark old standalone MCP crate as deprecated"
```

---

### Task 9: MCP Tool Tests (P0)

**Files:**
- Create: `api/src/api/mcp/tests/integration_tests.rs`
- Modify: `api/src/api/mcp/tool_registry.rs` - add test helpers
- Create: `api/src/api/mcp/tests/device_handler_tests.rs`
- Create: `api/src/api/mcp/tests/heartbeat_handler_tests.rs`

- [ ] **Step 1: Create tests directory and integration test file**

```bash
mkdir -p api/src/api/mcp/tests
```

- [ ] **Step 2: Write tool registry completeness test**

```rust
#[tokio::test]
async fn test_all_tools_registered() {
    // Initialize registry
    crate::api::mcp::register_tools().await;

    let registry = crate::api::mcp::get_mcp_registry()
        .expect("Registry not initialized");

    let tools = registry.read().await.list_tools();

    // Expected count: 12 device + 7 driver + 3 heartbeat + 3 self_heal + 3 knowledge = 28
    // Note: generate_driver returns NotImplemented in Phase 1
    assert_eq!(tools.len(), 28, "Expected 28 tools registered");

    // Verify critical tools exist
    let tool_names: Vec<_> = tools.iter().map(|t| t.name.clone()).collect();
    assert!(tool_names.contains(&"list_devices".to_string()));
    assert!(tool_names.contains(&"create_device".to_string()));
    assert!(tool_names.contains(&"report_heartbeat".to_string()));
    assert!(tool_names.contains(&"get_self_heal_policy".to_string()));
}
```

- [ ] **Step 3: Write NotImplemented error format test**

```rust
#[tokio::test]
async fn test_generate_driver_returns_not_implemented() {
    // Initialize
    crate::api::mcp::register_tools().await;
    let registry = crate::api::mcp::get_mcp_registry().unwrap();
    let handler = registry.read().await.get("generate_driver").unwrap();

    let result = handler.execute(json!({})).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    match err {
        ToolError::NotImplemented(msg) => {
            assert!(msg.contains("Phase 3"));
        }
        _ => panic!("Expected NotImplemented error"),
    }
}
```

- [ ] **Step 4: Write pagination clamp test**

```rust
#[tokio::test]
async fn test_list_devices_respects_pagination() {
    let registry = crate::api::mcp::get_mcp_registry().unwrap();
    let handler = registry.read().await.get("list_devices").unwrap();

    // Test over-limit page_size gets clamped
    let result = handler.execute(json!({"page_size": 1000 })).await;
    // Should succeed (implementation clamps to MAX_PAGE_SIZE)
}
```

- [ ] **Step 5: Write device handler tests**

```rust
#[tokio::test]
async fn test_get_device_not_found() {
    let registry = crate::api::mcp::get_mcp_registry().unwrap();
    let handler = registry.read().await.get("get_device").unwrap();

    let result = handler.execute(json!({"device_id": "nonexistent-id"})).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(matches!(err, ToolError::NotFound(_)));
}
```

- [ ] **Step 6: Run tests**

```bash
cd api && cargo test --lib mcp::tests -- --nocapture
```

Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add api/src/api/mcp/tests/
git commit -m "test(mcp): add MCP tool integration tests"
```

---

### Task 10: E2E Verification (P0)

**Files:**
- Create: `docs/superpowers/plans/2026-04-03-e2e-verification.md` (verification script)

- [ ] **Step 1: Verify API server runs**

```bash
# Start API server in background
cd api && cargo run &
sleep 5

# Test MCP endpoint exists
curl -s http://localhost:3002/mcp -X POST \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <test-token>" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' \
  | jq '.result.tools | length'
```

Expected output: `28`

- [ ] **Step 2: Test tools/list returns all tools**

```bash
curl -s http://localhost:3002/mcp -X POST \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <test-token>" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' \
  | jq '.result.tools[].name' | wc -l
```

Expected: `28`

- [ ] **Step 3: Test tools/call for list_devices**

```bash
curl -s http://localhost:3002/mcp/tools/call -X POST \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <test-token>" \
  -d '{"name":"list_devices","arguments":{"page":1,"page_size":10}}' \
  | jq '.result'
```

Expected: JSON array of devices

- [ ] **Step 4: Test heartbeat tool**

```bash
curl -s http://localhost:3002/mcp/tools/call -X POST \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <test-token>" \
  -d '{"name":"report_heartbeat","arguments":{"gateway_id":"test-gw"}}' \
  | jq '.result'
```

Expected: `{ "success": true, "timestamp": "..." }`

- [ ] **Step 5: Commit verification results**

```bash
git add docs/superpowers/plans/2026-04-03-e2e-verification.md
git commit -m "test(e2e): add MCP E2E verification results"
```

---

## 4. Task Dependency Graph

```
Task 7 (OpenClaw Skills)
    │
    ├── Task 9 (MCP Tests) ──────────────────────┐
    │         (can run in parallel after skills)   │
    │                                               │
    └── Task 10 (E2E Verification) ◄────────────────┘
              │
              └── All Phase 1 Complete ✓
```

---

## 5. Phase 1 Delivery Checklist

| Deliverable | Status |
|-------------|--------|
| MCP endpoint `/mcp` | ✅ Complete |
| 27 MCP tools | ✅ Complete |
| 4 OpenClaw Skills | ⬜ Pending |
| Old MCP crate deprecated | ⬜ Pending |
| MCP tool tests | ⬜ Pending |
| E2E verification | ⬜ Pending |

---

## 6. Future Phases (Not in Scope)

### Phase 2: Self-Healing Engine
- Probe scheduler implementation
- L0-L3 recovery strategy evaluation
- Action executor

### Phase 3: Cloud LLM Driver Generation
- `generate_driver` full implementation
- Cloud driver library integration

### Phase 4: Knowledge Base Sync
- `query_knowledge_base` full implementation
- `contribute_knowledge` implementation
- `sync_knowledge` cloud sync

---

## 7. Testing Strategy

### Unit Tests
- Each ToolHandler implementation tested individually
- Error cases (NotFound, InvalidParams, NotImplemented)
- Pagination clamping

### Integration Tests
- Full handler chain: MCP → service → DB
- JWT auth propagation
- Tenant isolation

### E2E Tests
- OpenClaw → MCP → TinyIoTHub full flow
- Natural language device onboarding scenario
