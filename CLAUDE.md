# TinyIoTHub — Claude Code 指令

> ⚠️ **强制要求**：在开始任何开发工作之前，必须阅读 `ARCHITECTURE_HARNESS.md`。该文件是架构宪法，所有代码必须遵守，违者 PR 拒绝合并。

## 项目概述

TinyIoTHub 是一个 **Rust 后端 + Next.js 前端** 的 IoT 边缘网关系统，支持多协议（Modbus、ONVIF、SNMP、MQTT）。

- **后端**: Rust 2021, Tokio (10 workers), Axum, Tower middleware, SQLx + SQLite
- **前端**: Next.js (App Router), React Query, TailwindCSS, Zustand
- **架构**: DDD (Domain-Driven Design) + Clean Architecture
- **分支策略**: `master` (边缘网关), `saas` (SaaS 云端版)

## 技术栈

```
Rust Backend          Next.js Frontend
─────────────        ───────────────
tokio (async)        React 19
axum (HTTP)          Next.js 15
tower (middleware)    React Query
sqlx + rusqlite      Zustand (state)
serde + serde_json    TailwindCSS
jsonwebtoken (JWT)    TypeScript
tokio-modbus         shadcn/ui
onvif                zod (validation)
snmp / rumqttc
```

## 项目结构

```
tinyiothub/
├── api/src/
│   ├── domain/          # 业务实体、值对象、领域服务
│   ├── application/     # 应用服务、数据上下文
│   ├── api/            # HTTP handlers（路由 + 业务逻辑）
│   ├── infrastructure/ # 外部依赖（DB、消息、网关）
│   ├── shared/         # 跨层共享（error、security）
│   └── dto/            # 数据传输对象
├── mcp/                # MCP Server (已废弃 - 集成到 api/src/api/mcp/)
├── web/
│   ├── app/            # Next.js App Router 页面
│   ├── service/        # API 调用层（必须用这个，不准直接 fetch）
│   ├── hooks/          # React Query hooks
│   ├── lib/            # 工具（api-client、query-keys）
│   └── store/          # Zustand 状态管理
├── docs/               # 技术文档
├── .kiro/steering/     # 开发规范（命名、API、架构）
└── .kiro/specs/        # 特性设计文档
```

## 开发规范

### API 规范

所有 API 必须返回统一格式：

```json
{ "code": 0, "msg": "", "result": T | null }
```

使用 `ApiResponseBuilder`：
```rust
ApiResponseBuilder::success(data)           // code: 0
ApiResponseBuilder::error("message")        // code: -1
ApiResponseBuilder::error_with_code(400, "bad request") // 自定义 code
```

路径规范：`/api/v1/` 前缀，RESTful。

### 命名规范

| 上下文 | 格式 | 示例 |
|--------|------|------|
| Rust 文件/模块 | snake_case | `device_service.rs` |
| Rust 结构体/枚举 | PascalCase | `DeviceStatus` |
| Rust 函数 | snake_case | `get_device_by_id` |
| TypeScript 文件 | kebab-case | `device-list.tsx` |
| React 组件 | PascalCase | `DeviceList` |
| TypeScript 变量 | camelCase | `deviceData` |

### 前端必须遵循

1. **API 调用必须走 `web/service/`**，不准在组件里直接 fetch
2. **必须用 `web/lib/api-client.ts`**（`apiGet`、`apiPost`、`apiPut`、`apiDelete`）
3. **React Query 数据获取必须走 hooks**，不准在组件里直接用 `useQuery`

## 工作流（Superpowers）

使用 `/brainstorming` 开始任何新功能或特性开发，不准跳过 brainstorming 直接写代码。

使用 `/plan-ceo-review` 审查设计文档和重大计划。

使用 `/plan-eng-review` 审查架构和实现方案。

使用 `/review` 在 PR 之前做代码审查。

使用 `/qa` 测试和修复 bug。

## ⚠️ AI 编码约束（必须遵守）

**核心原则：先搜索，后实现；找不到复用再新建。**

每次写代码前必须：
1. 在 `api/src/shared/` 搜索是否有可复用组件
2. 在 `web/service/` 搜索是否有可复用 API 封装
3. 在 `web/hooks/` 搜索是否有可复用数据 hooks
4. 确认要新建模块/文件时，说明理由并引用 ARCHITECTURE_HARNESS.md 对应条款

**禁止行为：**
- ❌ 不搜索就直接创建重复功能
- ❌ 在 `api/src/` 创建散弹式的 `utils/` 或 `helpers/`
- ❌ 前端组件里直接 `fetch()` 或 `useQuery()`
- ❌ API handler 里直接写 SQL
- ❌ 绕过 `ApiResponseBuilder` 拼装自定义 JSON 响应

详细规则见 `ARCHITECTURE_HARNESS.md`。

## 关键模式

### 后端

- **Repository Pattern**: 数据访问在 infrastructure 层
- **Async 所有 I/O**: tokio async/await，不准在 async fn 里用 blocking 代码
- **错误处理**: 用 `thiserror` 定义自定义错误，`Result<T, E>` 传播
- **中间件**: Tower (CORS、tracing、rate limit)

### 前端

- **Service Layer**: 所有 API 调用走 `web/service/`
- **React Query**: 数据获取走 hooks（`web/hooks/`）
- **API Client**: 统一用 `web/lib/api-client.ts`
- **表单验证**: zod schemas

## 当前功能状态

### 已实现

- **设备管理**: CRUD、多协议驱动（Modbus/ONVIF/SNMP/MQTT）
- **告警模块**: 规则引擎、告警通知、统计
- **用户认证**: JWT + 会话管理
- **MCP Server**: AI Agent 集成（Claude Desktop、Cursor）
- **Tenant/Subscription**: SaaS 多租户
- **自愈引擎**: 探测调度器、自动故障恢复（system/device/task 探针）
- **CI/CD**: GitHub Actions + Docker 多架构构建

### 规划中（见 `.kiro/specs/`）

- **event-service-system**: 事件驱动架构升级（SSE 推送、富文本）
- **device-template-system**: JSON 模板简化设备创建
- **harmonyos-jwt-openssl**: HarmonyOS SIGSEGV 修复

## 设计文档位置

```
.kiro/steering/     # 开发规范（命名/API/架构）
.kiro/specs/        # 特性设计（event-service、device-template、harmonyos-jwt）
docs/api/           # API 文档
docs/guide/         # 用户指南
docs/technical/     # 技术文档（当前有效）
docs/deployment/    # 部署指南
docs/drivers/        # 驱动开发
```

## 数据库

- **SQLite** 作为主要数据库（`api/` 目录）
- **SQLx** 用于编译时查询验证
- **migrations/** 目录存放 SQL 迁移文件

## Docker

- 多架构构建（linux/amd64 + linux/arm64）
- 本地构建脚本在 `scripts/`（如需要）
- Docker Hub: `grong/tinyiothub`

## 代码审查要求

- 所有 PR 必须有测试
- 提交信息格式: `type(scope): description`（参考 Conventional Commits）
- 不准在 `api/src/` 直接写 SQL，优先用 SQLx query builder
- 前端组件不准直接调用 API，必须走 service 层

## 重要约定

1. **Rust edition 2021**，最低支持 Rust 1.75+
2. **Node 18+** for frontend
3. 所有敏感配置通过环境变量，不硬编码
4. JWT secret 在生产环境必须设置，不允许默认密钥
5. API 错误必须带用户可读消息

## web-lit 前端开发规范

> web-lit 是基于 Lit 3 的 Web Components 前端，使用 Vite 构建，nanostore 管理状态。

### 1. Lit 组件生命周期

- **首次数据加载**用 `firstUpdated()`，不用 `connectedCallback()`（此时 shadow DOM 尚未渲染，querySelector 返回 null）
- `updated()` 必须是同步的，Lit 不 await 它
- `disconnectedCallback()` 中必须清理 interval、subscription、event listener

### 2. 事件监听器

- **禁止** `addEventListener('x', this.handler.bind(this))`——`.bind()` 每次创建新引用，`removeEventListener` 永远失败
- 使用箭头函数属性或保存 bound 引用：
  ```ts
  // 方式一：箭头函数属性（推荐）
  private handleClick = () => { ... }

  // 方式二：保存 bound 引用
  private _boundHandleClick = this.handleClick.bind(this)
  connectedCallback() {
    el.addEventListener('click', this._boundHandleClick)
  }
  disconnectedCallback() {
    el.removeEventListener('click', this._boundHandleClick)
  }
  ```

### 3. Nanostore 订阅

- 必须保存 `subscribe()` 返回的 unsubscribe 函数
- 在 `disconnectedCallback()` 中调用 unsubscribe
- 模块级订阅（如 auth-store）不需要清理，但加注释说明

### 4. 路由

- 使用 `navigate()` 函数（`import { navigate } from '../lib/navigate'`）
- **禁止**直接操作 `window.history.pushState` 或 `window.location.href`
- 路由解析只取 pathname，query string 用 URLSearchParams 单独处理

### 5. API 调用

- 路径**不带** `/api/v1/` 前缀（由 `buildUrl()` 统一添加）
- 使用 `apiGet`/`apiPost`/`apiPut`/`apiDelete`/`apiPatch`（从 `lib/api-client` 导出）
- 异步操作需防竞态：用 AbortController 或 generation counter
- Token 刷新有内置 mutex，多个并发 401 共享同一个 refresh promise

### 6. Shadow DOM

- CSS 选择器用 `:host` 而非标签名（如 `:host { display: block; }` 而非 `device-card { ... }`）
- 全局 CSS（base.css、layout.css）不穿透 Shadow DOM，组件内必须内联 styles
- `createRenderRoot()` 返回 `this` 表示不使用 Shadow DOM（仅 app-sidebar 等需要全局样式的组件）

### 7. 类型定义

- 以 `types/` 目录为 single source of truth
- services 层禁止重复定义已在 `types/` 中存在的接口
- 需要扩展类型时在 `types/` 中修改并 re-export

### 8. CSS 设计令牌

- 新增 CSS 变量**必须同时**定义在 dark 和 light 主题中（`base.css` 的 `:root` 和 `:root[data-theme-mode="light"]`）
- 主题切换通过 `data-theme-mode` 和 `data-theme` 属性

### 9. 可访问性

- `outline: none` 必须配合视觉焦点指示（`border-bottom-color` 变化、`box-shadow: var(--focus-ring)` 等）
- 全局 `:focus-visible` 规则在 base.css 中已定义，组件内表单控件需额外处理 `:focus` 样式

### 10. 命名规范

| 上下文 | 格式 | 示例 |
|--------|------|------|
| Lit 组件文件 | kebab-case | `device-card.ts` |
| 自定义元素名 | kebab-case | `<device-card>` |
| Lit 类名 | PascalCase | `DeviceCard` |
| CSS 类名 | BEM-like | `.card-header`, `.nav-item__icon` |
| 事件名 | camelCase | `deviceSelected` |
| nanostore 变量 | `$` 前缀 | `$currentRoute`, `$token` |
