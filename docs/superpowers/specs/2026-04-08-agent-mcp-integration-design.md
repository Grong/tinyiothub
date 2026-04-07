# ZeroClaw MCP 集成 + API Key 管理

## 背景

ZeroClaw Gateway 已运行，但未接入 TinyIoTHub MCP 工具。Agent 问设备情况时无法调用任何工具。

目标：让 ZeroClaw 通过 MCP 协议调用 TinyIoTHub 的 45+ 工具，同时提供 API Key 管理界面。

## 架构概览

```
ZeroClaw Gateway (MCP Client)
    │ HTTP POST /mcp
    │ X-API-Key: sk_live_xxx (外部 client)
    │
前端浏览器 (chat 页面)
    │ HTTP POST /mcp
    │ Authorization: Bearer <jwt> (浏览器端)
    ▼
TinyIoTHub /mcp (MCP Server)
    │ validate X-API-Key
    │ resolve workspace_id
    │ execute tool with workspace isolation
    ▼
MCP Tools (45+ tools, workspace-scoped)
```

## 改动清单

### Phase 0: Critical 安全修复（必须在 API Key 实现前完成）

**问题**：`read_properties`、`write_properties`、`send_command`、`create_device`、`export_device_report`、`get_device_metrics` 没有任何认证检查。任何能调用 MCP 端点的人都可以操作任意设备。

**修改文件**：`api/src/api/mcp/tools/device.rs`

**改动**：

```rust
// read_properties
let tenant_id = crate::api::mcp::handlers::get_mcp_context()
    .map(|c| c.tenant_id);
let device = Device::find_by_id(state.database(), &input.device_id).await
    .ok_or_else(|| ToolError::NotFound(...))?;
if let Some(ref tid) = tenant_id {
    if device.tenant_id.as_ref() != Some(tid) {
        return Err(ToolError::NotFound("Device not found".into()));
    }
}
```

`write_properties`、`send_command`、`export_device_report`、`get_device_metrics`、`create_device` 同理。

**注意**：Phase 0 只修复到 tenant_id 级别。Phase 3 改为 API Key 认证后，隔离升级到 workspace_id 级别。

### Phase 1: 数据库迁移

**新增 migration: `YYYYMMDDHHMMSS_add_workspace_id_to_api_keys.sql`**

```sql
ALTER TABLE api_keys ADD COLUMN workspace_id TEXT;

CREATE INDEX idx_api_keys_workspace ON api_keys(workspace_id);
```

说明：
- `workspace_id` 可为空（兼容旧的 tenant-only keys）
- 有 `workspace_id` 的 key 按 workspace 隔离
- 无 `workspace_id` 的 key 权限不变（deprecated）

### Phase 2: 后端 - API Key 实体改造

**文件: `dto/entity/tenant.rs`**

`ApiKey` 结构体新增字段：
```rust
pub workspace_id: Option<String>,
```

`ApiKey::find_by_prefix` 改造：
- 从 header `X-API-Key` 提取 key prefix
- 查找后验证 `is_enabled`、`is_revoked`、`expires_at`
- 返回 `(ApiKey, workspace_id)` — 不再返回完整的 `Tenant` 和 `Workspace` 对象（避免额外 DB 查询）
- **安全注意**：当前 `find_by_prefix` 使用 `DefaultHasher`（非 SHA256）。后续应改为 SHA256 哈希存储

`ApiKey` 新增方法：
- `create_with_workspace(db, tenant_id, workspace_id, req)` — 创建时绑定 workspace
- `find_by_workspace(db, workspace_id)` — 按 workspace 查询 keys
- `update_expiry(db, id, expires_at)` — 更新过期时间

**Key 哈希安全性**：现有实现用 `std::collections::hash_map::DefaultHasher`，这不是加密哈希。建议后续改为 SHA256（和设计文档一致），但 Phase 2 保持兼容，先只加 `workspace_id` 字段。

**文件: `dto/request/` — 新建 `api_key.rs`**

```rust
pub struct CreateApiKeyRequest {
    pub name: String,                    // 必填，标识用途
    pub workspace_id: String,            // 绑定到哪个 workspace
    pub expires_in_days: Option<i32>,    // 可选过期天数
}
```

### Phase 3: 后端 - MCP 端点认证改造

**文件: `api/mcp/handlers.rs`**

**CEO Review 确认**：当前实现只有 JWT 认证（`extract_jwt_claims`）。需要改为双轨认证：

```rust
fn extract_auth_context(headers: &HeaderMap) -> Result<McpAuthContext, ToolError> {
    // 优先 JWT（前端 chat 页面、浏览器端调用）
    if let Some(bearer) = headers.typed_get::<Authorization<Bearer>>() {
        let claims = validate_jwt(bearer.token())?;
        // JWT 中需要包含 workspace_id（当前 Claims 只有 tenant_id）
        // 需要扩展 JWT 字段
        return Ok(McpAuthContext {
            user_id: claims.user_id,
            tenant_id: claims.tenant_id,
            workspace_id: claims.workspace_id, // 新增，需扩展 JWT
            auth_method: AuthMethod::Jwt,
        });
    }
    // 其次 X-API-Key（ZeroClaw 等外部 MCP client）
    if let Some(key) = headers.get("X-API-Key").and_then(|v| v.to_str().ok()) {
        let (api_key, workspace_id) = validate_api_key(key)?;
        return Ok(McpAuthContext {
            user_id: api_key.tenant_id.clone(), // API Key 无 user_id
            tenant_id: api_key.tenant_id.clone(),
            workspace_id,
            auth_method: AuthMethod::ApiKey,
        });
    }
    Err(ToolError::Unauthorized("Missing Authorization or X-API-Key header".into()))
}
```

`handle_mcp_request` 流程：
1. 提取认证上下文（JWT 或 API Key）
2. 设置 `MCP_CONTEXT`（workspace_id 从 JWT 或 API Key 获取）
3. 执行工具，workspace 隔离由各工具内部保证

**注意**：`Claims` 当前没有 `workspace_id`。需要：
1. JWT payload 扩展增加 `workspace_id` 字段
2. 或在 `McpAuthContext` 中独立存储，不再依赖 `Claims`

`AuthContext` / `McpAuthContext` 为内部结构，不暴露给客户端。

### Phase 4: 后端 - API Key 管理端点

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
    "prefix": "sk_live_abcd1234",
    "raw_key": "sk_live_abcd1234xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
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
        "prefix": "sk_live_abcd****",
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

### Phase 5: 前端 - API Key 管理页面

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

### Phase 6: ZeroClaw 配置文档

**文件: `docs/agent/zeroclaw-mcp-setup.md` — 新建（也在开发文档清单中）**

```toml
[mcp]
enabled = true
deferred_loading = true

[[mcp.servers]]
name = "tinyiothub"
transport = "http"
url = "http://localhost:3002/mcp"
headers = { "X-API-Key" = "sk_live_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" }
tool_timeout_secs = 30
```

说明：内网部署用 `localhost:3002`；docker 环境用 `tinyiothub-api:3002`。

### Phase 7: Workspace 导航配置

**文件: `web/src/ui/app.ts`**

左侧导航配置加一项：
```ts
{ path: '/api-keys', label: 'API Keys', icon: 'key' }
```

仅在已选择 workspace 时显示。

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
X-API-Key: sk_live_xxx
    ↓
find_by_prefix 查找
    ↓
校验 is_enabled, is_revoked, expires_at
    ↓
设置 MCP_CONTEXT = { workspace_id: "ws-001" }
    ↓
执行工具（workspace 隔离由工具内部保证）
```

## 安全性

- raw_key 只在创建时返回一次，之后不存储、不显示
- 存储 SHA256 哈希，无法反推原 key
- 按 workspace 隔离，不同 workspace 的 key 无法互相访问
- 支持禁用/删除（is_revoked = 1）
- 支持过期时间自动失效

## MCP 工具安全隔离审查

### 审查结论

通过代码审查（`api/src/api/mcp/tools/device.rs`、`alarm_mcp.rs`），发现以下情况：

**已有 tenant_id 验证的工具**（通过 `get_mcp_context()`）：
- `get_device` ✅ — 验证 `device.tenant_id` 匹配
- `get_device_status` ✅ — 验证 tenant_id
- `update_device` ✅ — 验证 tenant_id
- `delete_device` ✅ — 验证 tenant_id
- `get_device_history` ✅ — 验证 tenant_id

**完全无认证的工具**（CRITICAL，必须在 API Key 实现前修复）：
- `read_properties` — 任何人都能读任意设备属性
- `write_properties` — 任何人都能写任意设备属性
- `send_command` — 任何人都能向任意设备发命令
- `create_device` — 任何人都能在任意 tenant 下创建设备
- `export_device_report` — 任何人都能导出任意设备报告
- `get_device_metrics` — 任何人都能查任意设备指标

**参数可伪造的工具**（需加固）：
- `alarm_list` — 接受 `workspace_id` 参数但不验证调用者是否有权访问
- `alarm_rule_add` — 同上

**实现 API Key 后各工具应使用的隔离字段**：

| 工具 | 隔离字段 | 实现方式 |
|------|---------|---------|
| `list_devices` | `workspace_id` | 替换 `tenant_id` 过滤 |
| `get_device` | `workspace_id` | 替换 tenant_id 验证 |
| `read_properties` | `workspace_id` | 验证 device.workspace_id 匹配 |
| `write_properties` | `workspace_id` | 验证 device.workspace_id 匹配 |
| `send_command` | `workspace_id` | 验证 device.workspace_id 匹配 |
| `create_device` | `workspace_id` | 从 context 获取，自动绑定 |
| `export_device_report` | `workspace_id` | 验证 device.workspace_id 匹配 |
| `get_device_metrics` | `workspace_id` | 验证 device.workspace_id 匹配 |
| `alarm_list` | `workspace_id` | 忽略参数，使用 context 中的 workspace_id |
| `alarm_acknowledge` | `workspace_id` | 验证 alarm 所属 device.workspace_id 匹配 |
| `alarm_rule_add` | `workspace_id` | 忽略参数，使用 context 中的 workspace_id |

**注**：当前 `MCP_CONTEXT` 返回 `Claims`，其中只有 `user_id`、`tenant_id`、`username`。API Key 实现后需扩展为包含 `workspace_id`。

## 测试计划

### Phase 0 安全验证（API Key 实现前）

1. 用 A tenant 的 JWT 调用 `read_properties(B device)` 应返回 404
2. 用 A tenant 的 JWT 调用 `write_properties(B 设备, value)` 应返回 404
3. 用 A tenant 的 JWT 调用 `send_command(B 设备)` 应返回 404
4. 用 A tenant 的 JWT 调用 `create_device` 应自动绑定到当前 tenant
5. 用 A tenant 的 JWT 调用 `export_device_report(B 设备)` 应返回 404

### Phase 3+ API Key 验证

1. 创建 API Key，验证 raw_key 一次性显示
2. 用 Key 调用 `POST /mcp`，验证 tools/list 返回 45+ 工具
3. 验证跨 workspace 无法访问（用 ws-A 的 key 访问 ws-B 的设备返回空）
4. 验证过期：设置过期时间为过去，调用返回 401
5. 验证禁用：`is_enabled = false` 返回 401
6. ZeroClaw 配置后，Agent 问「列出所有设备」能正确调用 list_devices 并返回结果

## 开发文档

除设计文档外，额外产出以下开发文档：

### 文档清单

| 文档 | 路径 | 内容 |
|------|------|------|
| MCP 工具开发指南 | `docs/agent/mcp-tools-guide.md` | 如何新增 MCP 工具：目录结构、ToolHandler trait、实现示例（以 `list_devices` 为例）、workspace 隔离规范 |
| API Key 认证说明 | `docs/agent/api-key-auth.md` | X-API-Key 认证流程、双轨认证（JWT + API Key）、workspace 上下文传递、工具隔离原理 |
| ZeroClaw MCP 配置 | `docs/agent/zeroclaw-mcp-setup.md` | config.toml 配置示例、HTTP transport、header 认证、内网/Docker 环境 URL、常见问题 |
| 工具列表参考 | `docs/agent/tools-reference.md` | 所有 MCP 工具的 name/description/inputSchema，便于 Agent 理解可用能力 |
