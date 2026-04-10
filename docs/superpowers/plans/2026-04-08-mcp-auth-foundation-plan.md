# MCP Phase 1: API Key Auth Foundation + API Key Management UI

## Status

**Implementation Complete** — committed on `feature/edge-agent-phase2`.
**Pending**: API Key 管理界面前端开发（后端已就绪，UI 未实现）。

## 背景

Phase 1 分两个部分：

1. **后端**: API Key 认证 + workspace 隔离 ✅ 已完成
2. **前端**: API Key 管理界面（创建、查看、复制、删除）❌ 未实现

本文档记录已完成的实现，并规划待完成的前端工作。

---

## Part A: 已完成 — API Key Auth Foundation

### Architecture Decisions

**AD-1**: `McpAuthContext` 只有 `workspace_id`，无 `user_id`

AI Agent 是 workspace 级别的实体，API Key 直接绑定 workspace。`actor_identifier()` 返回 `"api_key"` 用于操作日志。

**AD-2**: `alarm_list` 使用 Option B subquery 实现 workspace 隔离

`device_id IN (SELECT id FROM devices WHERE workspace_id = ?)` — 复用设备表已有索引，不改 alarms 表结构。

**AD-3**: `api_keys` 表移除 `tenant_id`，改为 `workspace_id`

API Key 直接绑定 workspace，tenant 从 workspace 反查。

**AD-4**: `validate_api_key` 返回 3-tuple `(ApiKey, Tenant, workspace_id)`

OpenClaw 调用 TinyIoTHub 时需要 tenant（全局资源）+ workspace（数据隔离）。

**AD-5**: 新 workspace 继承 API Key 对应 workspace 的 `tenant_id`

Agent 创建 workspace 时，tenant 从当前 API Key 所在 workspace 继承。

### Auth Flow

```
AI Agent
  ↓ X-API-Key: "tinh_xxxxxxxxxxxxxxxx"
OpenClaw /mcp
  ↓ validate_api_key (prefix lookup, check enabled/revoked/expired)
  ↓ resolve workspace → tenant
  ↓ extract McpAuthContext { workspace_id, api_key_id, api_key_name }
  ↓ thread-local storage (RAII guard)
MCP Tool Handler
  ↓ claims.workspace_id 过滤所有查询
```

### Migration

`migrations/20260409000001_add_workspace_id_for_mcp_tools.sql`：

1. `alarms` / `alarm_rules` / `job_schedules` 加 `workspace_id` 列 + 回填
2. 重建 `api_keys` 表：`tenant_id` → `workspace_id` + FK to workspaces
3. 需要离线执行

### 所有 MCP 工具 workspace 隔离状态

| 工具 | 数量 | 隔离方式 |
|------|------|----------|
| device_* | 12 | `tenant_id` → `workspace_id` |
| driver_* | 7 | 只读，无需隔离 |
| heartbeat_* | 3 | 网关级别，无需隔离 |
| self_heal_* | 3 | async workspace 解析 |
| knowledge_* | 3 | 只读，无需隔离 |
| workspace_* | 5 | 跨 workspace 授权检查 |
| job_* | 3 | `target_device_id` 关联查询 |
| batch_* | 2 | 调用其他已隔离工具 |
| alarm_* | 4 | Option B subquery |
| device_enhanced_* | 3 | 调用已隔离工具 |

### NOT in scope

- AI Agent 管理界面（创建/配置/删除 Agent）
- API Key 的使用量统计
- API Key 的权限细粒度控制（read-only vs full-access）
- API Key 的过期时间管理

### What already exists

- `settings.ts`: Settings 页面框架，Tab bar + Profile/Security tabs
- `web/src/i18n/`: API Key 相关文案（英文、中文、葡文）
- `success`/`toastError`: toast 通知系统
- `.form-group` / `.form-input` / `.submit-btn`: 表单样式
- `.settings-section` / `.settings-section-title`: settings 布局

### 设计审查评分

| Pass | Initial | After Fix | 关键改动 |
|------|---------|-----------|----------|
| Pass 1 (Info Arch) | 5/10 | 8/10 | 添加 key 用途说明文案 |
| Pass 2 (States) | 3/10 | 8/10 | 完整交互状态表 |
| Pass 3 (Journey) | 4/10 | 7/10 | 模态框关闭需 checkbox 确认 |
| Pass 4 (AI Slop) | 7/10 | 8/10 | 确认无 slop 风险 |
| Pass 5 (Design Sys) | 6/10 | 7/10 | 复用现有 settings 模式 |
| Pass 6 (Responsive) | 2/10 | 6/10 | 移动端 Tab→下拉，44px 触摸目标 |
| Pass 7 (Decisions) | — | — | 3 个决策已记录 |



| Commit | 内容 |
|--------|------|
| `bc609ce` | feat(migration): add workspace_id for MCP tool isolation |
| `9448d52` | feat(mcp): Phase 1 - API Key auth foundation (15 files) |
| `42042dd` | chore(mcp): mark Phase 2 tools as fully implemented in skills.yaml |

---

## Part B: 待完成 — API Key 管理界面

### 现状

- i18n 文案已就绪：英文、中文（简/繁）、葡萄牙语
- 后端 REST API 已就绪：`/{workspace_id}/api-keys`（GET list, POST create）
- 前端 UI **不存在**

### i18n 文案（已有）

```typescript
myApiKeys: "My API Keys"
myApiKeysSub: "Private channels you configured yourself"
noApiKeys: "No API keys"
addApiKey: "Add API Key"
apiKeyName: "API Key Name"
createApiKey: "Create API Key"
```

### 待开发

**页面**: `web/src/ui/views/settings.ts` — 在 Settings 页面添加 "API Keys" Tab

**功能**:
- API Keys Tab：切换到 Keys 视图
- 列表展示：key 名称、prefix（`tinh_xxxx`）、创建时间、状态
- 创建：输入名称 → POST `/api/v1/{workspace_id}/api-keys` → **一次性显示完整 key**（后端不存储明文）
- 复制：点击复制完整 key 到剪贴板
- 删除：确认对话框 → DELETE
- 警告：明文 key 只显示一次，刷新后不可再查

**API 调用**:

```typescript
// web/src/service/ 下新增或扩展
GET  /api/v1/{workspace_id}/api-keys
POST /api/v1/{workspace_id}/api-keys  { name: string }
DELETE /api/v1/{workspace_id}/api-keys/{id}
```

### UI 布局参考

```
┌─────────────────────────────────────────────┐
│  Settings                                  │
│  [Profile] [Security] [API Keys]           │
├─────────────────────────────────────────────┤
│  API Keys                        [+ Add]   │
│  ─────────────────────────────────────────│
│  ┌─────────────────────────────────────┐  │
│  │ Agent Key #1                   [📋][🗑]│  │
│  │ tinh_a1b2c3d4...  Created: 2026-04-08│  │
│  │ ● Active                            │  │
│  └─────────────────────────────────────┘  │
│  ┌─────────────────────────────────────┐  │
│  │ Agent Key #2                   [📋][🗑]│  │
│  │ tinh_xxxxxxxx...  Created: 2026-04-07│  │
│  │ ● Active                            │  │
│  └─────────────────────────────────────┘  │
└─────────────────────────────────────────────┘
```

### 创建 Key 对话框

```
┌─────────────────────────────────────┐
│  Create API Key               [x]  │
│  ─────────────────────────────────  │
│  Name                               │
│  [Agent Key for Building A        ] │
│                                     │
│  [Cancel]            [Create Key]  │
└─────────────────────────────────────┘
```

### Key 显示模态框（只出现一次，打开时自动复制）

```
┌─────────────────────────────────────┐
│  ✓ API Key 已创建                  │
│  ─────────────────────────────────  │
│  此 Key 将用于 AI Agent 认证。      │
│  已在打开时自动复制到剪贴板。        │
│                                     │
│  ┌─────────────────────────────┐   │
│  │ tinh_xxxxxxxxxxxxxxxxxxxx   │   │
│  └─────────────────────────────┘   │
│                                     │
│  ⚠ 关闭后将无法再次查看完整 Key。   │
│                                     │
│  [☑ 我已复制，关闭此窗口]           │
└─────────────────────────────────────┘
```

**行为**: 模态框打开时自动执行 `navigator.clipboard.writeText(key)`，用户无需手动点击复制。如果复制失败，显示 toast 提示用户手动复制。关闭必须勾选 checkbox。

### 删除确认对话框

```
┌─────────────────────────────────────┐
│  确认删除 API Key             [x]  │
│  ─────────────────────────────────  │
│  确定要删除 "Agent Key #2" 吗？     │
│  此操作无法撤销。                     │
│                                     │
│  [取消]                [确认删除]  │
└─────────────────────────────────────┘
```

### 交互状态表

| Feature | Loading | Empty | Error | Success | Partial |
|---------|---------|-------|-------|---------|---------|
| Key 列表 | 骨架屏（3行）| 欢迎图 + "还没有 API Key" + Add 按钮 | toast error + 重试 | key 列表 | — |
| 创建 Key | Add 按钮 spinner | — | toast error + dialog stays | key 显示模态框 | — |
| 复制 Key | — | — | toast "复制失败，请重试" | toast "已复制" + checkmark 动画 | — |
| 删除 Key | 行内 spinner | — | toast error | 行 fade-out + toast "已删除" | — |
| Key 模态框 | — | — | — | checkbox "我已复制" 才允许关闭 | — |

### 移动端适配

- **375px 以下**: Tab bar 横向滚动（`overflow-x: auto`, `scrollbar-width: none`）
- **Key 列表**: 每行 Info 堆叠为两行（name + prefix 在上，时间/status 在下）
- **复制/删除按钮**: 触摸目标 ≥ 44px
- **模态框**: 全宽，padding 16px

### 技术要点

1. **Key 只显示一次**: 后端返回 `full_key`，前端显示后即不再存储。后续只能看到 prefix（`tinh_xxxx`）。
2. **复制功能**: 使用 `navigator.clipboard.writeText()`
3. **权限**: API Keys 属于 workspace，只有 workspace owner 可管理
4. **workspace_id 来源**: 从当前用户 JWT 的 workspace context 获取
5. **关闭模态框需勾选确认**: 必须勾选 "我已复制" checkbox 才允许关闭，防止 key 丢失
6. **删除需二次确认**: 点击删除按钮 → 弹出确认对话框（显示 key name）→ 确认后才删除

### 无障碍设计

- **键盘**: Tab 在各按钮间切换，Enter 触发按钮，Escape 关闭对话框（需 checkbox 确认才生效）
- **屏幕阅读器**: key 列表用 `role="list"` + `role="listitem"`，模态框用 `role="dialog"` + `aria-labelledby`
- **焦点管理**: 打开模态框时焦点移到 key 输入框，关闭时回到触发按钮
- **颜色**: 状态 badge 使用颜色 + 文字标签（不要只用颜色区分状态）
- **对比度**: 文字与背景对比度 ≥ 4.5:1

### NOT in scope

- AI Agent 管理界面（创建/配置/删除 Agent）
- API Key 使用量统计
- API Key 权限细粒度控制（read-only vs full-access）
- API Key 过期时间管理

### What already exists

- `settings.ts`: Settings 页面框架，Tab bar + Profile/Security tabs
- `web/src/i18n/`: API Key 相关文案（英文、中文、葡文）
- `success`/`toastError`: toast 通知系统
- `.form-group` / `.form-input` / `.submit-btn`: 表单样式
- `.settings-section` / `.settings-section-title`: settings 布局

### 设计审查评分

| Pass | Initial | After Fix | 关键改动 |
|------|---------|-----------|----------|
| Pass 1 (Info Arch) | 5/10 | 8/10 | 添加 key 用途说明文案 |
| Pass 2 (States) | 3/10 | 8/10 | 完整交互状态表 |
| Pass 3 (Journey) | 4/10 | 7/10 | 模态框关闭需 checkbox 确认 |
| Pass 4 (AI Slop) | 7/10 | 8/10 | 确认无 slop 风险 |
| Pass 5 (Design Sys) | 6/10 | 7/10 | 复用现有 settings 模式 |
| Pass 6 (Responsive) | 2/10 | 6/10 | 移动端 Tab→下拉，44px 触摸目标 |
| Pass 7 (Decisions) | — | 3 resolved | 3 个决策已确认 |

### 待确认的设计决策

| Decision | Options | 选择 |
|----------|---------|------|
| 模态框关闭 UX | A) checkbox 强制确认 B) X 关闭但弹出警告 | A) 自动复制 + checkbox 确认 |
| 移动端 Tab Bar | A) 下拉选择器 B) 横向滚动 | B) 横向滚动 |
| 删除确认 | A) 对话框确认 B) 直接删除 | A) 对话框确认 |

### 下一步

1. 在 `web/src/service/` 添加 API Key service 方法（`listApiKeys`, `createApiKey`, `deleteApiKey`）
2. 在 `settings.ts` 添加 API Keys Tab 视图（`renderApiKeysTab`）
3. 实现 Key 列表、骨架屏、空状态、加载状态
4. 实现创建对话框 + 创建后一次性模态框（含 checkbox 确认）
5. 实现复制功能（clipboard API + toast 反馈）
6. 实现删除功能（含确认对话框）
7. 移动端适配（Tab → 下拉选择器）
8. 端到端测试
