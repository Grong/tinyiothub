# ZeroClaw MCP 集成 + API Key 管理

## 背景

ZeroClaw Gateway 已运行，但未接入 TinyIoTHub MCP 工具。Agent 问设备情况时无法调用任何工具。

目标：让 ZeroClaw 通过 MCP 协议调用 TinyIoTHub 的 45+ 工具，同时提供 API Key 管理界面。

## 架构概览

```
ZeroClaw Agent (静态配置)
    │ X-API-Key: sk_live_xxxx（绑定到 ws-001）
    │
TinyIoTHub /mcp (MCP Server)
    │ validate X-API-Key
    │ resolve workspace_id from api_keys.workspace_id
    │ set MCP_CONTEXT.workspace_id = "ws-001"
    ▼
MCP Tools (45+ tools, workspace-scoped)
    │ workspace isolation: WHERE workspace_id = ?
    │
前端 chat 页面 — 不直接调 /mcp，走 Agent 间接调用
```

## 改动清单

### 第一层: Auth Foundation

**必须先做。所有后续改动依赖第一层。**

#### 1.1 数据库迁移

**新增 migration: `YYYYMMDDHHMMSS_add_workspace_id_to_api_keys.sql`**

```sql
-- api_keys：加 workspace_id，移除冗余的 tenant_id
ALTER TABLE api_keys ADD COLUMN workspace_id TEXT;
ALTER TABLE api_keys DROP COLUMN tenant_id;
CREATE INDEX idx_api_keys_workspace ON api_keys(workspace_id);
CREATE INDEX idx_api_keys_prefix ON api_keys(prefix);

-- alarm 表加 workspace_id
ALTER TABLE alarms ADD COLUMN workspace_id TEXT;
CREATE INDEX idx_alarms_workspace ON alarms(workspace_id);

-- alarm_rules 表加 workspace_id
ALTER TABLE alarm_rules ADD COLUMN workspace_id TEXT;
CREATE INDEX idx_alarm_rules_workspace ON alarm_rules(workspace_id);

-- batch_commands 表已有 workspace_id（确认存在）
-- job_schedules 表加 workspace_id
ALTER TABLE job_schedules ADD COLUMN workspace_id TEXT;
CREATE INDEX idx_job_schedules_workspace ON job_schedules(workspace_id);
```

说明：
- `api_keys`：移除 `tenant_id`，只保留 `workspace_id`。tenant_id 从 workspace 关联获取，不再冗余存储
- `alarms.workspace_id`、`alarm_rules.workspace_id`、`job_schedules.workspace_id`：数据归属
- alarm 表需要 backfill：已有 alarm 根据关联 device 的 workspace_id 填充

#### 1.2 ApiKey 实体改造

**文件: `dto/entity/tenant.rs`**

`ApiKey` 结构体改造：
```rust
// 移除 tenant_id（从 workspace 关联获取，不再冗余存储）
pub struct ApiKey {
    pub id: String,
    pub workspace_id: String,       // 替换 tenant_id
    pub name: String,
    pub key_hash: String,
    pub prefix: String,
    pub permissions: String,
    pub rate_limit: i32,
    pub is_enabled: bool,
    pub is_revoked: bool,
    pub last_used_at: Option<String>,
    pub last_used_ip: Option<String>,
    pub request_count: i64,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

`ApiKey::find_by_prefix` 改造：
- 从 header `X-API-Key` 提取 key prefix
- 查找后验证 `is_enabled`、`is_revoked`、`expires_at`
- 返回 `Option<ApiKey>` — workspace_id 直接从 ApiKey 获取
- **安全注意**：当前使用 `DefaultHasher`（非 SHA256）。后续应改为 SHA256

`ApiKey` 新增方法：
- `create_with_workspace(db, workspace_id, req)` — 传入 workspace_id，自动关联 tenant
- `find_by_workspace(db, workspace_id)` — 按 workspace 查询 keys
- `update_expiry(db, id, expires_at)` — 更新过期时间

#### 1.3 MCP_CONTEXT 重构

**文件: `api/mcp/handlers.rs`**

MCP 端点只支持 API Key 认证。

```rust
// 新增结构体
struct McpAuthContext {
    workspace_id: String,
    // 注意：不保留 user_id。alarm_acknowledge 等记录操作用 "api_key" 而非 user
}

// 替换 extract_jwt_claims
fn extract_auth_context(headers: &HeaderMap) -> Result<McpAuthContext, ToolError> {
    let key = headers.get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ToolError::Unauthorized("Missing X-API-Key header".into()))?;

    let api_key = validate_api_key(key)?; // 返回 ApiKey，workspace_id 直接取自 ApiKey.workspace_id
    Ok(McpAuthContext {
        workspace_id: api_key.workspace_id,
    })
}
```

`validate_api_key` 实现：
1. 提取 prefix，查询 `api_keys` 表
2. 验证 `is_revoked = 0`、`is_enabled = 1`
3. 验证 `expires_at` 未过期（如果设置了）
4. 返回 `ApiKey`（workspace_id 直接在对象里）

**注意**：删除现有的 JWT 认证逻辑（`extract_jwt_claims`）。

---

### 第二层: Critical 高危修复（可与第一层并行）

**这些工具完全没有认证或认证严重不足。**

#### 2.1 batch_command / get_batch_status

**文件: `api/src/api/mcp/tools/batch.rs`**

当前问题：workspace_id 直接取自输入参数，没有任何验证。

**修复**：
```rust
async fn execute(&self, args: Value) -> Result<Value, ToolError> {
    let input: BatchCommandInput = serde_json::from_value(args)
        .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

    // 从 MCP context 获取 workspace_id，禁止使用输入参数
    let ctx = get_mcp_context()
        .ok_or_else(|| ToolError::Unauthorized("MCP context not initialized".into()))?;

    let state = crate::api::mcp::get_app_state()
        .ok_or_else(|| ToolError::Internal("AppState not initialized".into()))?;
    let db = state.database.clone();

    // 使用 context 中的 workspace_id，忽略输入参数
    let workspace_id = &ctx.workspace_id;

    // 验证输入的 device_ids 属于当前 workspace
    for device_id in &input.device_ids {
        let device = Device::find_by_id(&db, device_id).await
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Device {} not found", device_id)))?;
        if device.workspace_id.as_ref() != Some(workspace_id) {
            return Err(ToolError::NotFound("Device not found".into()));
        }
    }

    // ... 后续逻辑不变
}
```

`get_batch_status` 同理：查询 batch 后验证 batch.workspace_id 匹配。

#### 2.2 read_properties / write_properties / send_command / create_device / export_device_report / get_device_metrics

**文件: `api/src/api/mcp/tools/device.rs`**

当前问题：没有任何认证检查。

**修复**（所有 6 个工具统一）：
```rust
async fn execute(&self, args: Value) -> Result<Value, ToolError> {
    // 解析参数...
    let ctx = get_mcp_context()
        .ok_or_else(|| ToolError::Unauthorized("MCP context not initialized".into()))?;

    // 查询设备
    let device = Device::find_by_id(state.database(), &input.device_id).await
        .ok_or_else(|| ToolError::NotFound(...))?;

    // 验证 workspace 匹配
    if device.workspace_id.as_ref() != Some(&ctx.workspace_id) {
        tracing::warn!("MCP {}: access denied to device {} for workspace {}",
            self.name(), input.device_id, ctx.workspace_id);
        return Err(ToolError::NotFound("Device not found".into()));
    }
    // ... 后续逻辑不变
}
```

`create_device` 特殊处理：从 context 获取 workspace_id，自动绑定。
```rust
let request = CreateDeviceRequest {
    // ... 其他字段
    workspace_id: Some(ctx.workspace_id.clone()), // 自动绑定到当前 workspace
};
```

---

### 第三层: 其余 22 个工具（可独立并行修复）

#### 3.1 list_devices

**文件: `api/src/api/mcp/tools/device.rs`**

当前：tenant_id 过滤，workspace_id = None。

修复：
```rust
let ctx = get_mcp_context().ok_or_else(...)?;
let params = DeviceQueryParams {
    // ...
    tenant_id: None,
    workspace_id: Some(ctx.workspace_id.clone()), // 使用 context workspace_id
};
```

#### 3.2 get_device / get_device_status / update_device / delete_device / get_device_history

**文件: `api/src/api/mcp/tools/device.rs`**

当前：验证 tenant_id。

修复：替换为验证 workspace_id。
```rust
let ctx = get_mcp_context().ok_or_else(...)?;
if device.workspace_id.as_ref() != Some(&ctx.workspace_id) {
    tracing::warn!("MCP {}: access denied to device {} for workspace {}",
        self.name(), input.id, ctx.workspace_id);
    return Err(ToolError::NotFound("Device not found".into()));
}
```

#### 3.3 compare_devices / diagnose_device

**文件: `api/src/api/mcp/tools/device_enhanced.rs`**

当前：_claims 验证通过但无 workspace 检查。

修复：遍历所有输入 device_id，验证每个的 workspace_id 匹配。
```rust
for device_id in &input.device_ids {
    let device = Device::find_by_id(state.database(), device_id).await...?;
    if device.workspace_id.as_ref() != Some(&ctx.workspace_id) {
        return Err(ToolError::NotFound("Device not found".into()));
    }
}
```

#### 3.4 alarm_list / alarm_statistics

**文件: `api/src/api/mcp/tools/alarm_mcp.rs`**

当前：接受 workspace_id 参数但不验证。

修复：
- 忽略输入参数中的 workspace_id
- 使用 context 中的 workspace_id
- 改动 `alarm_service.get_alarm_history` 和 `alarm_service.get_alarm_statistics`：加 workspace_id 过滤参数
- 可能需要改 alarm_repository 加 workspace_id 条件

#### 3.5 alarm_acknowledge

**文件: `api/src/api/mcp/tools/alarm_mcp.rs`**

当前：用 user_id 做 ack。

修复：
1. 验证 alarm 所属 device.workspace_id 匹配
2. ack 操作记录 "api_key" 而非 user_id（因为 API Key 没有 user 概念）
3. 需要改 `alarm_service.acknowledge_alarm` 的签名，或在 tool 里直接写 DB

#### 3.6 alarm_rule_add

**文件: `api/src/api/mcp/tools/alarm_mcp.rs`**

当前：无认证。

修复：忽略输入参数中的 workspace_id，使用 context.workspace_id。

#### 3.7 list_schedules / create_schedule / delete_schedule

**文件: `api/src/api/mcp/tools/job.rs`**

当前：无认证。

修复：
- `list_schedules`：加 workspace_id 过滤
- `create_schedule`：从 context 获取 workspace_id，自动绑定
- `delete_schedule`：验证 schedule.workspace_id 匹配

#### 3.8 workspace_list / workspace_get / workspace_update / workspace_delete / workspace_create

**文件: `api/src/api/mcp/tools/workspace.rs`**

当前：用 tenant_id 隔离。

修复：API Key 认证场景下，Agent 只能访问绑定 workspace 本身。
- `workspace_get`/`workspace_update`/`workspace_delete`：验证 workspace.id == context.workspace_id
- `workspace_list`：只返回当前 workspace（直接返回 context.workspace_id 对应的 workspace）
- `workspace_create`：Tenant 可通过 API Key 创建新 workspace（新建 workspace 的 tenant_id 继承自 API Key 对应 workspace 的 tenant_id）

**说明**：由于 `api_keys` 表移除了 `tenant_id`，创建 API Key 时需要传入 workspace_id，再通过 workspace 反查 tenant_id。API Key 管理端点在创建 key 时需要这个关联。

#### 3.9 self_heal_* / knowledge_*

**文件: `api/src/api/mcp/tools/self_heal.rs`、`knowledge.rs`

当前：用 tenant_id。

修复：改为 workspace_id 隔离。

#### 3.10 heartbeat_* / list_drivers / scan_serial

这些工具不涉及数据隔离或只读系统状态，不需要 workspace 隔离。

---

### 第四层: API Key 管理端点

**文件: `api/api_keys.rs` — 新建**

| Method | Path | 描述 |
|--------|------|------|
| GET | `/api-keys?workspace_id=` | 列出 workspace 下的 keys（不返回 key_hash） |
| POST | `/api-keys` | 创建新 key（返回 raw_key 一次） |
| DELETE | `/api-keys/:id` | 删除 key |
| PATCH | `/api-keys/:id` | 更新 key（name、expires_at、is_enabled） |

响应格式统一用 `{code:0, result:T}`。

**Create 响应：**
```json
{
  "code": 0,
  "result": {
    "id": "xxx",
    "name": "ZeroClaw Agent",
    "prefix": "sk_live_yyyy_yyyy",
    "raw_key": "sk_live_yyyy_yyyy_yyyy_yyyy_yyyy_yyyy_yyyy_yyyy",
    "workspace_id": "ws-001",
    "expires_at": "2026-07-08",
    "created_at": "2026-04-08T10:00:00Z"
  }
}
```

**List 响应：**
```json
{
  "code": 0,
  "result": {
    "data": [
      {
        "id": "xxx",
        "name": "ZeroClaw Agent",
        "prefix": "sk_live_yyyy****",
        "workspace_id": "ws-001",
        "expires_at": "2026-07-08",
        "is_enabled": true,
        "last_used_at": "2026-04-08T10:30:00Z",
        "request_count": 1523,
        "created_at": "2026-04-08T10:00:00Z"
      }
    ],
    "pagination": { "page": 1, "page_size": 20, "total_count": 3 }
  }
}
```

---

### 第五层: 前端 - API Key 管理页面

**文件: `web/src/ui/views/api-keys.ts`**

左侧导航新增菜单项「API Keys」，路径 `/api-keys`。

页面内容：
- 顶部：「创建 API Key」按钮
- 表格：Name | Prefix | 创建时间 | 过期时间 | 状态 | 操作
- 操作：复制 Key | 删除

**创建 Key 弹窗：**
- Name 输入框（必填）
- 绑定 Workspace 下拉框（当前 workspace）
- 过期时间：下拉（30天 / 90天 / 180天 / 永不过期）
- 「创建」按钮

**创建成功弹窗（关键交互）：**
- 显示完整 `raw_key`
- 醒目提示「此 Key 仅显示一次，关闭后无法找回」
- 一键复制按钮
- 关闭按钮

---

### 第六层: ZeroClaw 配置文档

**文件: `docs/agent/zeroclaw-mcp-setup.md` — 新建**

```toml
[mcp]
enabled = true
deferred_loading = true

[[mcp.servers]]
name = "tinyiothub"
transport = "http"
url = "http://localhost:3002/mcp"
headers = { "X-API-Key" = "sk_live_yyyy_yyyy_yyyy_yyyy_yyyy_yyyy_yyyy_yyyy" }
tool_timeout_secs = 30
```

说明：内网部署用 `localhost:3002`；docker 环境用 `tinyiothub-api:3002`。

---

### 第七层: Workspace 导航配置

**文件: `web/src/ui/app.ts`**

左侧导航配置加一项：
```ts
{ path: '/api-keys', label: 'API Keys', icon: 'key' }
```

仅在已选择 workspace 时显示。

---

## 数据流

```
用户创建 API Key
    ↓
POST /api-keys { name: "ZeroClaw", workspace_id: "ws-001" }
    ↓
生成 raw_key = "sk_live_" + uuid
    ↓
存 key_hash = SHA256(raw_key)
    ↓
返回 raw_key（一次性）
    ↓
用户填入 ZeroClaw config.toml
    ↓
ZeroClaw 请求 POST /mcp
    ↓
X-API-Key: sk_live_xxxx
    ↓
find_by_prefix 查找
    ↓
校验 is_enabled, is_revoked, expires_at
    ↓
设置 MCP_CONTEXT = { workspace_id: "ws-001" }
    ↓
执行工具（workspace 隔离由各工具内部保证）
```

---

## 安全性

- raw_key 只在创建时返回一次，之后不存储、不显示
- 存储 SHA256 哈希，无法反推原 key（当前实现为 `DefaultHasher`，建议后续升级）
- 按 workspace 隔离，不同 workspace 的 key 无法互相访问
- 支持禁用/删除（is_revoked = 1）
- 支持过期时间自动失效
- MCP 端点只接受 API Key 认证，删除 JWT 路径

---

## 完整工具清单与隔离方式

| 工具 | 文件 | 隔离方式 | 优先级 |
|------|------|---------|--------|
| **batch_command** | batch.rs | 忽略输入 workspace_id，验证所有 device_ids 属于当前 ws | CRITICAL |
| **get_batch_status** | batch.rs | 验证 batch.workspace_id == context.workspace_id | CRITICAL |
| **read_properties** | device.rs | 验证 device.workspace_id == context.workspace_id | CRITICAL |
| **write_properties** | device.rs | 同上 | CRITICAL |
| **send_command** | device.rs | 同上 | CRITICAL |
| **create_device** | device.rs | 从 context 获取 workspace_id，自动绑定 | CRITICAL |
| **export_device_report** | device.rs | 验证 device.workspace_id == context.workspace_id | CRITICAL |
| **get_device_metrics** | device.rs | 同上 | CRITICAL |
| **alarm_list** | alarm_mcp.rs | 忽略输入参数，使用 context.workspace_id，加 repository 过滤 | HIGH |
| **alarm_statistics** | alarm_mcp.rs | 同上 | HIGH |
| **alarm_rule_add** | alarm_mcp.rs | 忽略输入参数，使用 context.workspace_id | HIGH |
| **compare_devices** | device_enhanced.rs | 验证所有 device_ids 属于当前 ws | HIGH |
| **diagnose_device** | device_enhanced.rs | 验证 device.workspace_id == context.workspace_id | HIGH |
| **list_schedules** | job.rs | 加 workspace_id 过滤 | HIGH |
| **create_schedule** | job.rs | 从 context 获取 workspace_id，自动绑定 | HIGH |
| **delete_schedule** | job.rs | 验证 schedule.workspace_id == context.workspace_id | HIGH |
| **alarm_acknowledge** | alarm_mcp.rs | 验证 alarm 所属 device.workspace_id 匹配 | MEDIUM |
| **list_devices** | device.rs | 加 WHERE workspace_id = ? | MEDIUM |
| **get_device** | device.rs | 替换 tenant_id 为 workspace_id 验证 | MEDIUM |
| **get_device_status** | device.rs | 同上 | MEDIUM |
| **update_device** | device.rs | 同上 | MEDIUM |
| **delete_device** | device.rs | 同上 | MEDIUM |
| **get_device_history** | device.rs | 同上 | MEDIUM |
| **workspace_list** | workspace.rs | 只返回当前 workspace | MEDIUM |
| **workspace_get** | workspace.rs | 验证 workspace.id == context.workspace_id | MEDIUM |
| **workspace_create** | workspace.rs | 待定（见决策点） | MEDIUM |
| **workspace_update** | workspace.rs | 验证 workspace.id == context.workspace_id | MEDIUM |
| **workspace_delete** | workspace.rs | 同上 | MEDIUM |
| **self_heal_*** | self_heal.rs | 改为 workspace_id 隔离 | LOW |
| **knowledge_*** | knowledge.rs | 同上 | LOW |
| **list_drivers** | driver.rs | 不涉及数据隔离 | NONE |
| **heartbeat_*** | heartbeat.rs | 只读系统状态 | NONE |
| **scan_serial** | device_enhanced.rs | 只读系统状态 | NONE |

---

## 决策点

~~1. **workspace_create**：API Key 绑定到某个 workspace，能通过工具创建新 workspace 吗？如果能，新 workspace 属于哪个 tenant？~~ **已确认：继承 API Key 对应 workspace 的 tenant_id（见 2.7.4 `workspace_create`）**

2. **backfill alarm.workspace_id**：已有 alarm 需要根据关联 device 填充 workspace_id，需要一次性 migration 脚本：
   ```sql
   -- alarm_worspace_backfill.sql
   UPDATE alarms
   SET workspace_id = (
       SELECT d.workspace_id FROM devices d WHERE d.id = alarms.device_id
   )
   WHERE workspace_id IS NULL;
   ```
   执行时机：alarm 表加 workspace_id 列之后、主系统上线之前。

---

## 现状（What Already Exists）

在开始实施前，确认以下部分已存在，无需重复开发：

| 组件 | 文件 | 状态 |
|------|------|------|
| MCP Server HTTP Transport | `api/src/api/mcp/` | 已有框架，需要改造认证 |
| ToolHandler trait | `api/src/api/mcp/tool_registry.rs` | 已有完整定义 |
| 所有 27 个工具实现 | `api/src/api/mcp/tools/*.rs` | 已有实现，需要加 workspace 隔离 |
| devices 表 workspace_id 列 | SQL schema | **已有**，无需 migration |
| batch_commands 表 workspace_id 列 | SQL schema | **已有**（已在 migration 确认） |
| workspaces 表 | SQL schema | 已有 |
| tenants 表 | SQL schema | 已有 |
| ApiKey 实体 | `dto/entity/tenant.rs` | 已有，需改造成 `workspace_id` |
| tools/list 实现 | `api/src/api/mcp/tools/mod.rs` | 已有 |

**需要新增的**：仅 API Key 认证层（`validate_api_key`、MCP_CONTEXT、`X-API-Key` header 解析），以及各工具内的 workspace 隔离逻辑。

---

## 并行化策略

七层之间部分可以并行实施：

```
第一层（Auth Foundation）— 必须先行
  └─ 第二层（Critical 高危）— 依赖第一层产出
        └─ 第三层（Medium/High）
              └─ 第四层（Low + self_heal/knowledge）
                    └─ 第五层（API Key 管理 CRUD）
                          └─ 第六层（ZeroClaw 配置文档）
                                └─ 第七层（Workspace 导航 UI）

第三层可拆分并行：
  - A 组：alarm_list、alarm_statistics、alarm_rule_add（alarm 隔离）
  - B 组：diagnose_device、compare_devices（device 增强隔离）
  - C 组：list_schedules、create_schedule、delete_schedule（job 隔离）
  - D 组：list_devices、get_device、get_device_status、update_device、delete_device、get_device_history（基础 device 隔离）

第一层 + 第二层 之后，可以开 4 个并行 worktree 各自做 A/B/C/D 组。
```

---

## 测试计划

### 第一层 + 第二层验证

1. 用 API Key 调用 `batch_command(ws-B 的设备)` 应返回 404
2. 用 ws-A 的 Key 调用 `read_properties(ws-B 设备)` 应返回 404
3. 用 ws-A 的 Key 调用 `write_properties(ws-B 设备, value)` 应返回 404
4. 用 ws-A 的 Key 调用 `create_device` 应绑定到 ws-A

### 第三层验证

1. 用 ws-A 的 Key 调用 `list_devices`，验证只返回 ws-A 的设备
2. 用 ws-A 的 Key 调用 `get_device(ws-B 设备ID)` 应返回 404
3. 用 ws-A 的 Key 调用 `alarm_list`，验证只返回 ws-A 的告警
4. 用 ws-A 的 Key 调用 `list_schedules`，验证只返回 ws-A 的任务

### 第四层验证

1. 创建 API Key，验证 raw_key 一次性显示
2. 验证过期：设置过期时间为过去，调用返回 401
3. 验证禁用：`is_enabled = false` 返回 401

### 端到端验证

1. 用 Key 调用 `POST /mcp`，验证 tools/list 返回 45+ 工具
2. ZeroClaw 配置后，Agent 问「列出所有设备」能正确调用 list_devices 并返回结果

---

## 开发文档

除设计文档外，额外产出以下开发文档：

### 文档清单

| 文档 | 路径 | 内容 |
|------|------|------|
| MCP 工具开发指南 | `docs/agent/mcp-tools-guide.md` | 如何新增 MCP 工具：目录结构、ToolHandler trait、实现示例（以 `list_devices` 为例）、workspace 隔离规范 |
| API Key 认证说明 | `docs/agent/api-key-auth.md` | X-API-Key 认证流程、workspace 上下文传递、工具隔离原理 |
| ZeroClaw MCP 配置 | `docs/agent/zeroclaw-mcp-setup.md` | config.toml 配置示例、HTTP transport、header 认证、内网/Docker 环境 URL、常见问题 |
| 工具列表参考 | `docs/agent/tools-reference.md` | 所有 MCP 工具的 name/description/inputSchema，便于 Agent 理解可用能力 |

---

## Performance Review

### 严重程度分级

| 级别 | 定义 | 影响 |
|------|------|------|
| CRITICAL | 安全漏洞，无认证或认证可绕过 | 跨 workspace 数据泄露 |
| HIGH | 有认证但验证错误对象，或缺失关键验证 | 部分隔离失效 |
| MEDIUM | 代码可工作但使用了错误字段，或 DRY 违规 | 可运行但逻辑错误 |
| LOW | 代码风格、非关键问题 | 可运行，逻辑基本正确 |
| INFO | 后续需注意的技术债 | 不影响上线 |

### Issue 1: handlers.rs — 三处 JWT 认证需替换为 API Key

**文件**: `api/src/api/mcp/handlers.rs`

| 位置 | 当前问题 | 严重程度 |
|------|---------|---------|
| L25-28 | `MCP_CONTEXT` 存 `Claims`（含 user_id/tenant_id），应改为 `McpAuthContext`（仅 workspace_id） | CRITICAL |
| L103-112 | `extract_jwt_claims` — 需替换为 API Key 提取 | CRITICAL |
| L119-130 | 三个 handler 各有 JWT 提取 + context 设置，重复代码 3 处 | MEDIUM |

**验证**：`get_mcp_context()` 在 3 个 handler 中调用方式一致，统一替换可行。

---

### Issue 2: batch.rs — workspace_id 完全无验证

**文件**: `api/src/api/mcp/tools/batch.rs`

| 工具 | 当前问题 | 严重程度 |
|------|---------|---------|
| `batch_command` | workspace_id 直接取 `input.workspace_id`（L149），没有任何验证。agent 可对任意 workspace 发命令 | CRITICAL |
| `get_batch_status` | 调用 `get_mcp_context()` 获取 `_claims`（L232），但 `_claims` 未被使用，batch_id 无任何 workspace 验证 | CRITICAL |

```rust
// L123-124 当前注释说明完全是错的
// Note: workspace_id access is verified via tenant ownership in the database
// The MCP context provides tenant_id from JWT for authorization

// L155 submitted_by 用 user_id，迁移后 API Key 无 user_id
submitted_by: Some(claims.user_id.clone()),
```

**额外发现**：L857 测试有 typo：`vec!["read".to_string()]` → `vec!["read"]`

---

### Issue 3: device.rs — 8 个工具缺失 workspace 隔离

**文件**: `api/src/api/mcp/tools/device.rs`

| 工具 | 当前问题 | 严重程度 |
|------|---------|---------|
| `read_properties` (L472) | 无任何 workspace/tenant 验证，agent 可读任意设备属性 | CRITICAL |
| `write_properties` (L546) | 无任何 workspace/tenant 验证，agent 可写任意设备属性 | CRITICAL |
| `send_command` (L656) | 无任何 workspace/tenant 验证，agent 可控制任意设备 | CRITICAL |
| `get_device_metrics` (L1056) | 无任何 workspace/tenant 验证 | CRITICAL |
| `export_device_report` (L1117) | 无任何 workspace/tenant 验证 | CRITICAL |
| `list_devices` (L281-295) | 用 `tenant_id` 过滤，`workspace_id = None`，应改为 `workspace_id = Some(ctx.workspace_id)` | HIGH |
| `create_device` (L795) | `workspace_id: None`，新设备未绑定到任何 workspace | HIGH |
| `get_device`/`get_device_status`/`update_device`/`delete_device`/`get_device_history` | 用 `tenant_id` 验证（LOW impact，因为 device 已有 tenant_id），但应改为 workspace_id | MEDIUM |

**Device 字段确认**：`Device` 实体同时有 `tenant_id` 和 `workspace_id` 字段。设计要求使用 workspace_id 隔离。

---

### Issue 4: alarm_mcp.rs — 4 个工具 workspace 验证缺失

**文件**: `api/src/api/mcp/tools/alarm_mcp.rs`

| 工具 | 当前问题 | 严重程度 |
|------|---------|---------|
| `alarm_list` (L190-201) | `AlarmQueryCriteria` 无 workspace_id，alarm_service 无 workspace 过滤 | HIGH |
| `alarm_statistics` (L285) | 无 workspace 过滤 | HIGH |
| `alarm_rule_add` (L54) | input 接受 workspace_id 但工具内完全忽略输入（好），但也无 context.workspace_id 使用 | HIGH |
| `alarm_acknowledge` (L341) | 用 `claims.user_id` 记录 ack 操作人，迁移后应改为固定字符串 "api_key" | MEDIUM |

**alarm_service 依赖**：alarm_list/statistics 调用 `alarm_service.get_alarm_history/count_alarms/statistics`，这些方法签名需要加 workspace_id 参数。需确认 domain 层支持。

---

### Issue 5: job.rs — 3 个工具 workspace 验证缺失

**文件**: `api/src/api/mcp/tools/job.rs`

| 工具 | 当前问题 | 严重程度 |
|------|---------|---------|
| `list_schedules` (L106-112) | 调用 `Job::find_all` 传 `JobQueryParams`，但 params 无 workspace_id，find_all 可能不过滤 | HIGH |
| `create_schedule` (L231-246) | 新建 schedule 未绑定 workspace_id | HIGH |
| `delete_schedule` (L293-315) | 获取了 existing job（用于返回 name），但删前未验证 workspace_id | HIGH |

**TODO 注释**：`L299` 有 `// TODO: Verify tenant ownership via claims.tenant_id when jobs have tenant_id`，说明作者已意识到问题但未修复。

---

### Issue 6: tenant.rs — ApiKey 实体需改造

**文件**: `api/src/dto/entity/tenant.rs`

| 项目 | 当前问题 | 严重程度 |
|------|---------|---------|
| `ApiKey` struct (L395-411) | `tenant_id: String`，应改为 `workspace_id: String` | CRITICAL |
| `create()` 方法 (L437) | 传入 `tenant_id`，应改为 `workspace_id`，通过 workspace 反查 tenant_id | CRITICAL |
| `find_by_tenant()` 方法 (L561) | 应改为 `find_by_workspace()` | MEDIUM |
| `find_by_prefix()` (L528) | 返回 `Option<ApiKey>` 含 tenant_id，迁移后需返回含 workspace_id | MEDIUM |
| `record_usage()` (L635) | 依赖 tenant_id，更新逻辑需同时更新 workspace_id | INFO |
| 测试 L857 | `vec!["read".to_string()]` typo，应为 `vec!["read"]` | LOW |

---

### Issue 7: DeviceQueryParams 和 JobQueryParams 需加 workspace_id

**文件**: `api/src/dto/entity/device.rs` 和 `api/src/dto/entity/job.rs`

| 文件 | 当前状态 | 需改动 |
|------|---------|--------|
| `DeviceQueryParams` | 有 `workspace_id: Option<String>`（L295），但 list_devices 传 `None` | 改为 `Some(ctx.workspace_id)` |
| `JobQueryParams` | 无 workspace_id 字段 | 需加 `workspace_id: Option<String>` |

---

### Issue 8: domain/service 层需支持 workspace_id 过滤

以下服务方法需要加 workspace_id 参数或改用 workspace 隔离：

| 方法 | 文件 | 需改动 |
|------|------|--------|
| `alarm_service.get_alarm_history` | `domain/alarm/` | `AlarmQueryCriteria` 加 workspace_id |
| `alarm_service.count_alarms` | `domain/alarm/` | 同上 |
| `alarm_service.get_alarm_statistics` | `domain/alarm/` | 同上 |
| `alarm_service.acknowledge_alarm` | `domain/alarm/` | user_id 参数改为固定 "api_key" |
| `alarm_service.create_rule` | `domain/alarm/` | 加 workspace_id 绑定 |
| `Job::find_all` | `dto/entity/job.rs` | `JobQueryParams` 需支持 workspace_id |
| `Job::create` | `dto/entity/job.rs` | 需接受 workspace_id |
| `BatchCommandRepository::find_by_idempotency_key` | `infrastructure/batch_command/` | 需用 workspace_id 隔离 |

---

### Issue 9: AlarmRule::new 无 workspace_id 绑定

**文件**: `domain/alarm/rule.rs`

`AlarmRule::new` 签名不接受 workspace_id。alarm_rule_add 工具创建 rule 时无法绑定 workspace。需要确认 domain 层是否需要改造。

---

### Issue 10: API Key 认证的 rate_limit / request_count 更新

**文件**: `tenant.rs` L635-708

`record_usage()` 需要更新 api_key 的 `request_count`、`last_used_at`。迁移后需改为按 `workspace_id` 索引。更重要的是：认证流程中 validate_api_key 需要更新 `last_used_at` 和 `request_count`。

---

### 代码风格观察

1. **DRY 违规**：`handlers.rs` 中 3 个 handler 各自复制 JWT 提取逻辑（~15 行重复 × 3）
2. **Error 一致性**：handler 用 `ToolError`，但部分地方返回 `Internal("...")` 字符串拼接，可读性差
3. **Test coverage**：`handlers.rs` 只有 JSON-RPC deserialize 测试，无集成测试

---

## Performance Review

### N+1 查询风险

| 工具 | 场景 | 问题 | 影响 |
|------|------|------|------|
| `batch_command` | device_ids 验证 | 每个 device_id 单独查询 `Device::find_by_id` | O(n) 查询，n = device_ids 数量 |
| `compare_devices` | 多设备比较 | 每个 device_id 单独查询 | O(n) 查询 |
| `list_devices` | 实时状态同步 | 遍历每个 device 查 DataContext | O(n) 查询 |

**建议**：考虑批量查询 `Device::find_by_ids(&[device_ids])` 替代循环单查。

### alarm_service 重复查询

`alarm_list` 工具（L190-201, L203-228）先调用 `get_alarm_history` 再调用 `count_alarms`，两次查询。可以合并为一次（使用 `SELECT ... WITH ROLLUP` 或 `SELECT COUNT(*)` 作为子查询）。

### Thread-Local MCP_CONTEXT 开销

handlers.rs L26-28 使用 `thread_local!` + `RefCell`。在 async 上下文中，每个请求都克隆 `Claims`。虽然实现正确，但 `McpContextGuard` 依赖 RAII 模式在 Drop 时清理。

**潜在风险**：如果 async task 在 set 后 panic 但 guard 未能正确 drop（极端情况），context 可能泄露。不过 Rust async runtime 通常能处理这种情况。

### API Key 认证路径性能

| 步骤 | 当前实现 | 预期性能 |
|------|---------|---------|
| header 解析 | `X-API-Key` header | < 1ms |
| prefix 查库 | `SELECT * FROM api_keys WHERE prefix = ? AND is_revoked = 0` | 索引命中，< 5ms |
| key hash 验证 | `DefaultHasher`（非 SHA256） | < 1ms |
| workspace_id 解析 | 从 ApiKey 对象直接取 | < 1ms |

总开销：< 10ms per request。认证层不是性能瓶颈。

### 数据库索引

设计文档已列出所有 migration 需加的索引。确认现有索引：
- `devices.workspace_id` — 已有
- `batch_commands.workspace_id` — 已有
- `alarms.device_id` — 已有（通过 alarm_list subquery 命中）

**新增索引**：
- `api_keys.workspace_id` — 加速 API Key 按 workspace 查询
- `alarms.workspace_id` — 加速 alarm_list
- `alarm_rules.workspace_id` — 加速 alarm_rule 过滤
- `job_schedules.workspace_id` — 加速 list_schedules

### batch_command auto_execute 风险

`batch_command` 工具（L163-189）在工具执行内部调用 `BatchCommandExecutor::execute`。这是一个同步操作，在 async handler 中直接 await。

**如果批处理很大**（100+ 设备），可能阻塞请求。建议：
1. 保持现状（auto_execute=true 作为默认），因为 ZeroClaw 场景设备数量通常有限
2. 如果需要大规模并行，考虑将 auto_execute 改为异步后台任务（超出本次 scope）

### 并行化策略对性能影响

| 层 | 并行可行性 | 性能影响 |
|----|---------|---------|
| 第一层 | 必须串行 | Auth 层改动影响所有工具 |
| 第二层 | 可并行（batch + device read/write） | 无交叉依赖 |
| 第三层 A/B/C/D | 四组完全独立 | 可 4 个 worktree 并行开发 |
| 第五层 | 前端独立 | 不影响后端性能 |

---

## Review Log

| 日期 | 评审 | 评审人 | 主要发现 |
|------|------|--------|---------|
| 2026-04-08 | Architecture Review | plan-eng-review | 3 个架构问题：McpAuthContext 只需 workspace_id；alarm_list 选 subquery；api_keys 移除 tenant_id |
| 2026-04-08 | Code Quality Review | plan-eng-review | 10 个问题：6 个 CRITICAL（auth bypass、workspace 隔离缺失），5 个 HIGH，4 个 MEDIUM，2 个 LOW，3 个 INFO |
| 2026-04-08 | Performance Review | plan-eng-review | 3 个 N+1 查询风险，alarm_service 重复查询，batch auto_execute 潜在阻塞，认证路径 <10ms 无瓶颈 |
| 2026-04-08 | Outside Voice Review | Codex | 9 个新问题：非原子 migration 风险、alarm backfill 孤儿记录、DefaultHasher 未修复、alarm_rules backfill 缺失、batch_command TOCTOU race、rate limit 未实施、并行化按工具而非文件、empty device_ids 未处理、workspace_list 变成 no-op |

---

## Outside Voice Review (Codex)

### CRITICAL 问题

#### O-1: Migration 非原子，分阶段操作存在中间态风险

当前 migration 为多条 ALTER TABLE 顺序执行，非原子操作。流程：添加 `workspace_id` → 回填数据 → 删除 `tenant_id`。若中途崩溃，系统处于半迁移状态。更关键的是：在线迁移时，新 key 已有 `workspace_id`，旧 key 为 NULL，`find_by_prefix` 返回的 `Option<ApiKey>` 可能 workspace_id 为空，导致认证失败。

**修复**：使用 `CREATE TABLE AS SELECT` + rename 原子替换，或明确要求停机窗口内完成迁移。**必须**在迁移前验证：无代码引用 `api_keys.tenant_id`。

#### O-2: Alarm Backfill 产生孤儿记录

backfill 脚本假设每个 alarm 的 `device_id` 指向存在且 `workspace_id` 非空 的设备。但若设备已删除或 `workspace_id` 为 NULL，alarm 的 `workspace_id` 保持 NULL。这些 NULL 记录对所有新查询不可见（被过滤掉），等于静默删除了告警数据。

**修复**：回填前先执行预检查：
```sql
SELECT COUNT(*) FROM alarms a LEFT JOIN devices d ON a.device_id = d.id
WHERE a.workspace_id IS NULL AND (d.id IS NULL OR d.workspace_id IS NULL);
-- 若结果 > 0，需先处理这些孤儿记录
```
---

### HIGH 问题

#### O-3: alarm_rules Backfill 脚本缺失

文档明确列出 alarm_rules 需要加 workspace_id 列，但只提供了 alarms 的回填脚本，alarm_rules 的回填脚本缺失。实施时容易遗漏。

#### O-4: DefaultHasher 未在第一层修复

文档数据流图写"SHA256"，代码注释写"DefaultHasher"，两者矛盾。prefix（`sk_live_xxxx`）在文档中可见，若 DB 被读取，SipHash 输出可被破解。这不应被 defer 到后续迭代。

#### O-5: batch_command 验证存在 TOCTOU Race

验证循环结束后到 `BatchCommandExecutor::execute` 执行之间，设备可能被删除或迁移到其他 workspace。batch 操作不是原子的。

**修复**：在事务内执行验证和命令，或在 execute 内部重新验证。

#### O-6: Rate Limit 存储但未实施

`ApiKey.rate_limit` 字段存在，但 `validate_api_key` 不检查。被盗 key 可绕过所有限流。

---

### MEDIUM 问题

#### O-7: 并行化按工具分组而非文件分组

4 个 worktree 同时修改 `device.rs`（D 组 6 个工具）会产生大量 git 冲突。按工具分组并行不可行，应改为按文件分组。

#### O-8: empty `device_ids` 数组未处理

`batch_command` 验证循环对空数组直接跳过，发送空 batch 给 `BatchCommandExecutor`。行为未定义（返回 success? error?）。

#### O-9: `workspace_list` 返回结果集大小为 1

修复后只返回当前 workspace，等于一个 identity check。API 语义不清晰，若有意为之（"读取自身元数据"模式），需在文档中说明。

#### O-10: 404 vs 403 信息隐藏 trade-off 未说明

所有工具对跨 workspace 访问返回"Device not found"(404) 而非 403。好处是防枚举攻击，坏处是调试困难、审计日志不反映真实原因。文档应明确说明这一设计决策。

#### O-11: 撤销操作存在 Race

`SELECT * FROM api_keys WHERE prefix = ? AND is_revoked = 0` 检查后、实际执行前，并发 DELETE/PATCH 可能设置 `is_revoked = 1`。高安全场景应使用 `SELECT FOR UPDATE`。

#### O-12: 前端"绑定 Workspace 下拉框"语义误导

UI 描述说"下拉框选择"，但 workspace_id 是系统自动绑定的，不允许用户选择。描述应改为"自动绑定到当前 Workspace，不可更改"，避免用户困惑。

---

## NOT in scope

- SHA256 替换 DefaultHasher（安全增强，可后续迭代）
- JWT 认证路径（已删除）
- alarm/history 表加 workspace_id（alarm 表已覆盖，history 按 alarm_id 关联）
