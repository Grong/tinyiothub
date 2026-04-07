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
- 查找后返回 `(ApiKey, Tenant, Workspace)` 三元组
- 验证 `is_enabled`、`is_revoked`、`expires_at`

`ApiKey` 新增方法：
- `create_with_workspace(db, tenant_id, workspace_id, req)` — 创建时绑定 workspace
- `find_by_workspace(db, workspace_id)` — 按 workspace 查询 keys
- `update_expiry(db, id, expires_at)` — 更新过期时间

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

保留 JWT 和 API Key 双轨认证，按 header 自动判断：

```rust
fn extract_auth_context(headers: &HeaderMap) -> Result<AuthContext, ToolError> {
    // 优先 JWT（前端 chat 页面、浏览器端调用）
    if let Some(bearer) = headers.typed_get::<Authorization<Bearer>>() {
        let claims = validate_jwt(bearer.token())?;
        return Ok(AuthContext::Jwt { user_id: claims.user_id, workspace_id: claims.workspace_id });
    }
    // 其次 X-API-Key（ZeroClaw 等外部 MCP client）
    if let Some(key) = headers.get("X-API-Key").and_then(|v| v.to_str().ok()) {
        let (api_key, workspace_id) = validate_api_key(key)?;
        return Ok(AuthContext::ApiKey { workspace_id });
    }
    Err(ToolError::Unauthorized("Missing Authorization or X-API-Key header".into()))
}
```

`handle_mcp_request` 流程：
1. 提取认证上下文（JWT 或 API Key）
2. 设置 `MCP_CONTEXT`（workspace_id 从 JWT 或 API Key 获取）
3. 执行工具，workspace 隔离由各工具内部保证

`AuthContext` 为内部结构，不暴露给客户端。

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

**文件: `docs/agent/zeroclaw-mcp-setup.md` — 新建**

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

## MCP 工具 workspace 隔离说明

现有 MCP 工具已按 `tenant_id` 隔离。改为 API Key 后，工具执行时通过 `MCP_CONTEXT` 获取 `workspace_id`。各工具的 workspace 隔离逻辑：

- `list_devices` — 加 `WHERE workspace_id = ?`
- `get_device` — 验证 device 的 `workspace_id` 匹配
- `read_properties` — 同上
- `write_properties` — 同上
- `alarm_list` — 加 workspace 过滤
- 其他工具同理

如果当前工具没有 workspace 隔离逻辑，需要补充（按工具逐个检查）。

## 测试计划

1. 创建 API Key，验证 raw_key 一次性显示
2. 用 Key 调用 `POST /mcp`，验证 tools/list 返回 45+ 工具
3. 验证跨 workspace 无法访问（用 ws-A 的 key 访问 ws-B 的设备返回空）
4. 验证过期：设置过期时间为过去，调用返回 401
5. 验证禁用：`is_enabled = false` 返回 401
6. ZeroClaw 配置后，Agent 问「列出所有设备」能正确调用 list_devices 并返回结果
