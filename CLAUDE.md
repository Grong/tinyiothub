# TinyIoTHub — Claude Code 指令


Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

Tradeoff: These guidelines bias toward caution over speed. For trivial tasks, use judgment.

1. Think Before Coding
Don't assume. Don't hide confusion. Surface tradeoffs.

Before implementing:

State your assumptions explicitly. If uncertain, ask.
If multiple interpretations exist, present them - don't pick silently.
If a simpler approach exists, say so. Push back when warranted.
If something is unclear, stop. Name what's confusing. Ask.
2. Simplicity First
Minimum code that solves the problem. Nothing speculative.

No features beyond what was asked.
No abstractions for single-use code.
No "flexibility" or "configurability" that wasn't requested.
No error handling for impossible scenarios.
If you write 200 lines and it could be 50, rewrite it.
Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

3. Surgical Changes
Touch only what you must. Clean up only your own mess.

When editing existing code:

Don't "improve" adjacent code, comments, or formatting.
Don't refactor things that aren't broken.
Match existing style, even if you'd do it differently.
If you notice unrelated dead code, mention it - don't delete it.
When your changes create orphans:

Remove imports/variables/functions that YOUR changes made unused.
Don't remove pre-existing dead code unless asked.
The test: Every changed line should trace directly to the user's request.

4. Goal-Driven Execution
Define success criteria. Loop until verified.

Transform tasks into verifiable goals:

"Add validation" → "Write tests for invalid inputs, then make them pass"
"Fix the bug" → "Write a test that reproduces it, then make it pass"
"Refactor X" → "Ensure tests pass before and after"
For multi-step tasks, state a brief plan:

1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

These guidelines are working if: fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.


> ⚠️ **强制要求**：所有代码必须遵守本文档中的架构规则，违者 PR 拒绝合并。

## 项目概述

TinyIoTHub 是一个 **Rust 后端 + Lit 3 前端** 的云端 SaaS 物联网平台，支持配置和管理边缘网关设备，兼容多协议（Modbus、ONVIF、SNMP、MQTT）。

- **后端**: Rust 2024, Tokio, Axum, Tower middleware, SQLx + SQLite
- **前端**: Lit 3 + Vite + TypeScript, Web Components, nanostore
- **架构**: DDD (Domain-Driven Design) + Clean Architecture
- **分支策略**: `master` (主分支), `saas` (SaaS 云端版)

## 技术栈

**后端架构：** 多 Crate Workspace（Rust 2024）
- **主二进制：** `cloud/` — SaaS 应用编排层
- **核心库：** `crates/tinyiothub-*` — 模块化业务组件
- **依赖方向：** `cloud/edge → runtime → core ← storage`（单向不可逆，contracts 在 core，infrastructure 在 runtime）

```
Rust Backend (Multi‑Crate)   Lit Frontend
─────────────────────────   ───────────────
tokio (async)               Lit 3
axum (HTTP)                 Vite
tower (middleware)          TypeScript
sqlx + rusqlite             Web Components
serde + serde_json          nanostore
jsonwebtoken (JWT)          Signal-based state
tokio-modbus                CSS Modules
onvif                       i18n
snmp / rumqttc
```

## 项目结构（workspace-refactor 分支 — 多 Crate 架构）

**依赖方向（单向不可逆）：**

```
tinyiothub/
├── cloud/              # SaaS 应用编排层（主二进制）
│   ├── src/
│   │   ├── api/        # HTTP handlers（SaaS 路由 + 业务逻辑）
│   │   ├── modules/    # 业务模块（标准三层架构：types → service → handler）
│   │   ├── shared/     # 跨层共享（persistence, security, error_handling, utils）
│   │   └── server.rs   # Axum 服务启动
│   └── Cargo.toml
├── crates/             # 内部库 Crate
│   ├── tinyiothub-core/    # 契约层：traits + 领域模型 + repository 接口
│   ├── tinyiothub-runtime/ # 基础设施：EventBus, DataServer, drivers, executors
│   ├── tinyiothub-storage/ # 数据层：SQLite 实现（re-export core traits）
│   ├── tinyiothub-web/     # HTTP 基础设施层
│   ├── tinyiothub-error/   # 错误类型（带 `thiserror` 派生）
│   └── ...（其他支持库）
├── web/                # Lit 3 + Vite 前端（独立项目）
│   ├── src/ui/         # Lit Web Components 组件和页面
│   ├── src/api/        # API 客户端
│   ├── src/i18n/       # 国际化
│   ├── src/styles/     # CSS 样式
│   └── src/stores/     # nanostore 状态管理
├── docs/               # 技术文档
├── .kiro/steering/     # 开发规范（命名、API、架构）
├── .kiro/specs/        # 特性设计文档
└── docs/superpowers/   # AI 辅助设计文档（plans / specs）
```

### Crate 职责与依赖规则

| Crate | 职责 | 禁止 |
|-------|------|------|
| `core` | traits、领域模型、repository 接口、规则引擎 | 不得包含 I/O、数据库访问 |
| `runtime` | EventBus, DataServer, drivers, executors | 不得依赖 cloud/web |
| `storage` | SQLite 实现（re-export core traits） | 不得依赖 runtime/cloud |
| `web` | HTTP 中间件、ApiResponseBuilder、安全提取器 | 不得包含业务逻辑 |
| `cloud` | 应用编排、路由、业务模块 | handler 中不得直接写 SQL |

**禁止的依赖：** core/storage 不得依赖 runtime；任何 crate 不得反向依赖上层。

### cloud/ 模块结构

```
modules/<module>/
├── types.rs     # 请求/响应结构体（禁止用 dto.rs）
├── service.rs   # 业务逻辑
└── handler/     # HTTP handler（调用 service，返回 ApiResponse）
shared/          # 跨模块共享（persistence, security, middleware）
```

## 开发规范

### API 规范

**统一响应格式：** 所有 API 必须返回 `{ "code": 0, "msg": "", "result": T | null }` 格式。

**使用 `ApiResponseBuilder`：** 从 `tinyiothub-web::response::ApiResponseBuilder` 导入，禁止手动拼接 JSON。

**路径规范：** `/api/v1/` 前缀，RESTful 设计。

**详细规范见本文档 API 规范部分。**

### 命名规范

| 上下文 | 格式 | 示例 |
|--------|------|------|
| Rust 文件/模块 | snake_case | `device_service.rs` |
| Rust 结构体/枚举 | PascalCase | `DeviceStatus` |
| Rust 函数 | snake_case | `get_device_by_id` |
| TypeScript 文件 | kebab-case | `device-list.ts` |
| Lit 组件类 | PascalCase | `DeviceList` |
| 自定义元素名 | kebab-case | `<device-list>` |
| TypeScript 变量 | camelCase | `deviceData` |
| nanostore 变量 | `$` 前缀 | `$currentRoute` |


## 工作流（Superpowers）

使用 `/brainstorming` 开始任何新功能或特性开发，不准跳过 brainstorming 直接写代码。

使用 `/plan-ceo-review` 审查设计文档和重大计划。

使用 `/plan-eng-review` 审查架构和实现方案。

使用 `/review` 在 PR 之前做代码审查。

使用 `/qa` 测试和修复 bug。

## ⚠️ AI 编码约束（必须遵守）

**核心原则：先搜索，后实现；找不到复用再新建。**

每次写代码前必须：
1. 在 `cloud/src/shared/` 搜索是否有可复用组件（persistence、security、error_handling 等）
2. 在 `cloud/src/modules/` 搜索是否有同类业务模块可参考标准结构
3. 在相应 Crate 的公共模块搜索（如 `tinyiothub-web/src/` 的共享工具）
4. 在 `web/src/api/` 搜索是否有可复用 API 封装
5. 在 `web/src/stores/` 搜索是否有可复用状态管理
6. 确认要新建模块/文件时，说明理由

**禁止行为：**
- ❌ 不搜索就直接创建重复功能
- ❌ 在 `cloud/src/` 或任何 Crate 中创建散弹式的 `utils/` 或 `helpers/`（公共组件应放在 `cloud/src/shared/`）
- ❌ 模块中使用 `dto.rs` 命名（统一使用 `types.rs`）
- ❌ 模块中创建 `application/` 子目录（业务逻辑放 `service.rs`）
- ❌ 前端组件里直接 `fetch()`
- ❌ API handler 里直接写 SQL
- ❌ 绕过 `ApiResponseBuilder` 拼装自定义 JSON 响应


## 关键模式

### 后端（多 Crate 架构）

- **架构分层**: `cloud/edge` (SaaS) → `runtime` (基础设施) → `core` (契约 + 类型) ← `storage` (数据实现)
- **Repository Pattern**: 数据访问在 `cloud/src/shared/persistence/repositories/`
- **模块三层架构**: 每个模块统一 `types.rs` → `service.rs` → `handler/`（禁止使用 `dto.rs` 命名）
- **Async 所有 I/O**: tokio async/await，不准在 async fn 里用 blocking 代码
- **错误处理**: 用 `thiserror` 定义自定义错误（使用 `tinyiothub-error` crate），`Result<T, E>` 传播
- **中间件**: Tower (CORS、tracing、rate limit) — 在 `tinyiothub-web` crate 中定义
- **API 响应**: 使用 `tinyiothub-web::response::ApiResponseBuilder`

### 前端

- **API Client**: 所有 API 调用走 `web/src/api/client.ts`
- **State Management**: nanostore（`web/src/stores/`）
- **Routing**: 使用 `navigate()` 函数，禁止直接操作 `window.location`
- **Shadow DOM**: CSS 选择器用 `:host`，全局 CSS 不穿透 Shadow DOM
- **Lifecycle**: 首次数据加载用 `firstUpdated()`，`disconnectedCallback()` 清理订阅

## 当前功能状态

### 已实现

- **设备管理**: CRUD、多协议驱动（Modbus/ONVIF/SNMP/MQTT）
- **告警模块**: 规则引擎、告警通知、统计
- **用户认证**: JWT + 会话管理
- **AI Agent 集成**: 内嵌 MCP Server + A2UI 聊天界面（Claude Desktop、Cursor 支持）
- **Tenant/Subscription**: SaaS 多租户
- **自愈引擎**: 探测调度器、自动故障恢复（system/device/task 探针）
- **CI/CD**: GitHub Actions + Docker 多架构构建
- **A2UI 组件库**: IoT 专用 AI 交互组件（设备卡片、告警表格、控制面板）

### 规划中（见 `.kiro/specs/`）

- **event-service-system**: 事件驱动架构升级（SSE 推送、富文本）
- **device-template-system**: JSON 模板简化设备创建
- **harmonyos-jwt-openssl**: HarmonyOS SIGSEGV 修复

## 设计文档位置

```
.kiro/steering/           # 开发规范（命名/API/架构）
.kiro/specs/              # 特性设计文档
docs/superpowers/plans/   # AI 辅助架构设计（当前活跃）
docs/superpowers/specs/   # AI 辅助详细设计（当前活跃）
docs/api/                 # API 文档
docs/guide/               # 用户指南
docs/technical/           # 技术文档
docs/deployment/          # 部署指南
docs/drivers/             # 驱动开发
```

## 数据库

- **SQLite** 作为主要数据库（`cloud/` 目录）
- **SQLx** 用于编译时查询验证
- **migrations/** 目录存放 SQL 迁移文件

## Docker

- 多架构构建（linux/amd64 + linux/arm64）
- 本地构建脚本在 `scripts/`（如需要）
- Docker Hub: `grong/tinyiothub`

## 代码审查要求

- 所有 PR 必须有测试
- 提交信息格式: `type(scope): description`（参考 Conventional Commits）
- 不准在 `cloud/src/` 直接写 SQL，优先用 SQLx query builder（通过 Repository 模式访问数据）
- 前端组件不准直接调用 API，必须走 `web/src/api/` 层

## 重要约定

1. **Rust edition 2024**，最低支持 Rust 1.82+
2. **Node 18+** for frontend
3. 所有敏感配置通过环境变量，不硬编码
4. JWT secret 在生产环境必须设置，不允许默认密钥
5. API 错误必须带用户可读消息

## 前端开发规范（Lit 3 + nanostore）

> **详细前端分层指南见** `FRONTEND_LAYERING_GUIDE.md`

### 核心原则

1. **API 调用必须走 `web/src/api/`**，不准在组件里直接 `fetch`
2. **状态管理使用 nanostore**，保存 `subscribe()` 返回的 unsubscribe 并在 `disconnectedCallback()` 中清理
3. **Lit 生命周期**：数据加载用 `firstUpdated()`，清理用 `disconnectedCallback()`
4. **事件监听器**：禁止 `addEventListener('x', this.handler.bind(this))`，使用箭头函数属性

### 分层指南

| 场景 | 推荐模式 | 适用情况 |
|------|----------|----------|
| 简单读取 | 组件 → `api/client.ts` | 单一页面，无复用需求 |
| 需要复用 | 组件 → `stores/` → `api/client.ts` | 2+ 地方使用，需要缓存 |
| 设备指令 | 组件 → `stores/` → `api/client.ts`（带状态管理） | 异步操作，需要状态追踪 |

### 关键规则

- **路由**：使用 `navigate()` 函数，禁止直接操作 `window.history`
- **Shadow DOM**：CSS 选择器用 `:host`，全局 CSS 不穿透 Shadow DOM
- **类型定义**：以 `web/src/types/` 为 single source of truth
- **命名规范**：Lit 组件文件用 kebab-case，类名用 PascalCase，自定义元素名用 kebab-case

## 异步与数据访问

- 所有 I/O 必须 `async/await`（`tokio::fs`、`tokio::net`），禁止在 async fn 中用阻塞代码
- 数据库操作必须通过 Repository（`cloud/src/shared/persistence/repositories/`），handler 中禁止直接 SQL
- 共享状态用 `Arc<RwLock<T>>` 或 `DashMap`，禁止 `Rc<RefCell<T>>`
- 迁移文件在 `cloud/migrations/`，命名 `YYYYMMDDHHMMSS_description.sql`，必须可重复执行

## 提交前自查

- [ ] 依赖方向正确？（无反向依赖）
- [ ] 遵循 `types → service → handler` 三层架构？
- [ ] 使用 `ApiResponseBuilder` 构建响应？
- [ ] 通过 Repository 访问数据库？
- [ ] async fn 中无阻塞代码？
- [ ] 有对应测试？
- [ ] 搜索了 `shared/` 确认无重复实现？

## Skill routing

When the user's request matches an available skill, ALWAYS invoke it using the Skill
tool as your FIRST action. Do NOT answer directly, do NOT use other tools first.
The skill has specialized workflows that produce better results than ad-hoc answers.

Key routing rules:
- Product ideas, "is this worth building", brainstorming → invoke office-hours
- Bugs, errors, "why is this broken", 500 errors → invoke investigate
- Ship, deploy, push, create PR → invoke ship
- QA, test the site, find bugs → invoke qa
- Code review, check my diff → invoke review
- Update docs after shipping → invoke document-release
- Weekly retro → invoke retro
- Design system, brand → invoke design-consultation
- Visual audit, design polish → invoke design-review
- Architecture review → invoke plan-eng-review
- Save progress, checkpoint, resume → invoke checkpoint
- Code quality, health check → invoke health
