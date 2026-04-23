# TinyIoTHub Workspace Architecture Contract

> **版本：** 1.0.0
> **状态：** 生效中
> **最后更新：** 2026-04-20

本文档定义 TinyIoTHub Workspace 中各个 Crate 的**不可协商**的边界与职责。任何代码变更、新增依赖或模块迁移都必须遵守此契约。

---

## 1. 核心原则 (Core Principles)

| 原则 | 描述 |
| :--- | :--- |
| **依赖方向单向性** | `cloud/edge/marketplace` → `web` → `engine` → `storage` → `core`。禁止反向依赖，禁止循环依赖。 |
| **零租户污染** | `core`、`storage`、`engine`、`web` 中**绝对禁止**出现 `tenant_id`、`workspace_id`、`user_id` 字段或相关逻辑。多租户是 `cloud` 的特有职责。 |
| **核心域无基础设施** | `core` 只能依赖 `serde`、`chrono`、`uuid` 等基础序列化库，禁止依赖 `tokio`、`axum`、`sqlx`、`thiserror`。 |
| **特征倒置** | `engine` 只依赖 `storage` 中定义的 **traits**，而非具体实现。`cloud` 负责在运行时注入具体实现。 |

---

## 2. Crate 职责边界 (Crate Boundaries)

### 2.1 `tinyiothub-core`
**角色：** 通用基础类型与领域模型。

**✅ 允许：**
- `Device`、`DeviceId`、`TelemetryPoint`、`Event`、`EventType`、`ErrorCode`。
- 纯数据结构定义，可包含 `serde` 序列化注解。
- 简单的校验逻辑（如 `validate()` 方法）。

**❌ 禁止：**
- **任何 SaaS 模型**：`Tenant`、`User`、`Workspace`、`Role`、`Permission`、`Product`、`Tag`。
- **任何数据库映射**：`sqlx::FromRow`、`diesel` 注解。
- **任何 Web 框架依赖**：`axum::Json`、`rocket` 等。
- **包含 `workspace_id` 字段的结构体**（`Event` 例外：仅作为透传路由数据，不参与任何逻辑）。

---

### 2.2 `tinyiothub-storage`
**角色：** 纯 IoT 数据存取抽象与轻量级缓存。

**✅ 允许：**
- **Repository Traits**：`DeviceRepository`、`TelemetryRepository`、`EventRepository`（仅 IoT 实体）。
- **IoT 模型实现**：`Device`、`TelemetryPoint` 的 `sqlx::FromRow` 实现。
- **通用缓存**：`DeviceCacheManager`（内存缓存，不感知租户）。
- **SQLite 实现**：`SqliteDeviceRepository`（仅包含单设备 CRUD）。

**❌ 禁止：**
- **SaaS 仓储 Trait 或实现**：`TenantRepository`、`UserRepository`、`WorkspaceRepository`。
- **包含 `WHERE workspace_id = ?` 的 SQL 查询**。
- **任何业务逻辑**（如：采集调度、告警计算、规则评估）。
- **依赖 `cloud` 或 `web` crate**。

---

### 2.3 `tinyiothub-engine`
**角色：** 可独立部署的通用 IoT 业务引擎。

**✅ 允许：**
- **采集引擎**：`DeviceCollectorEngine`、`DriverRegistry`、`CommandQueue`。
- **规则与告警**：`RuleEngine`、`AlarmEngine`、`AutomationEngine`。
- **模板与自愈**：`TemplateEngine`、`SelfHealingEngine`。
- **通用调度**：`CronScheduler`（调度执行器，不包含租户信息）。
- **依赖注入**：通过 `Arc<dyn DeviceRepository>` 使用 storage traits。

**❌ 禁止：**
- **多租户逻辑**：不处理 `workspace_id`，不校验用户权限。
- **HTTP 依赖**：禁止依赖 `axum`、`hyper`、`tower`。
- **SaaS 服务**：不包含 `MarketplaceService`、`AgentService`、`NotificationService`（对用户的邮件/短信推送）。

---

### 2.4 `tinyiothub-web`
**角色：** 共享的 HTTP 基础设施层。

**✅ 允许：**
- **通用 Handlers**：`GET /devices`、`POST /telemetry`（仅接受纯 IoT 数据）。
- **通用 DTOs**：`DeviceResponse`、`TelemetryRequest`（不包含租户/用户字段）。
- **中间件**：`RequestId`、`Logging`、`Metrics`、`Cors`。

**❌ 禁止：**
- **租户鉴权中间件**：JWT 中解析 `workspace_id` 的代码**必须**留在 `cloud`。
- **SaaS API 路由**：`/workspaces`、`/users`、`/marketplace` 路由**必须**在 `cloud` 中定义。
- **反向依赖 `cloud`**：`web` 不能 import `cloud` 的任何模块。

---

### 2.5 `cloud` (Binary)
**角色：** SaaS 应用编排层。

**✅ 允许：**
- **所有 SaaS 领域逻辑**：`tenant`、`user`、`workspace`、`role`、`permission`、`marketplace`。
- **租户感知的适配器**：实现 `TenantDeviceRepository`（内部附加 `WHERE workspace_id = ?`）。
- **SaaS API 路由**：`/api/v1/workspaces/*`、`/api/v1/users/*`。
- **应用启动与编排**：`ServiceOrchestrator`、配置加载、优雅停机。
- **依赖所有内部 crates**：可以依赖 `web`、`engine`、`storage`、`core`。

**❌ 禁止：**
- **将 SaaS 逻辑下沉**：禁止将 `Tenant` 模型或 `WorkspaceRepository` 移入 `core` 或 `storage`。
- **污染通用库**：禁止为了省事而在 `web` 的 DTO 中直接嵌入 `workspace_id`。

---

## 3. 依赖关系图 (Dependency Graph)

```text
┌─────────────────────────────────────────────────────────────────┐
│                         cloud (Binary)                          │
│  - Tenant Logic, Workspace Routes, ServiceOrchestrator          │
└───────────────────────────────┬─────────────────────────────────┘
                                │ 依赖
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                      tinyiothub-web (HTTP)                      │
│  - Generic IoT Handlers, Logging Middleware                     │
└───────────────────────────────┬─────────────────────────────────┘
                                │ 依赖
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                     tinyiothub-engine (IoT)                     │
│  - Collector, Scheduler, Rule, Alarm, Template                  │
└───────────────────────────────┬─────────────────────────────────┘
                                │ 依赖 (Traits only)
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                    tinyiothub-storage (Data)                    │
│  - DeviceRepo Trait, TelemetryRepo Trait, Cache                 │
└───────────────────────────────┬─────────────────────────────────┘
                                │ 依赖
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                     tinyiothub-core (Types)                     │
│  - Device, Event, TelemetryPoint, Error                         │
└─────────────────────────────────────────────────────────────────┘
```

## 4. 强制执行清单 (Enforcement Checklist)

### 4.1 自动化检查 (CI 必须执行)

| 检查项 | 命令 | 期望结果 |
| :--- | :--- | :--- |
| 依赖方向验证 | `cargo tree -p tinyiothub-core --edges normal \| grep -E "(tokio\|axum\|sqlx)"` | 无输出 (PASS) |
| 租户关键字泄漏 | `grep -r "workspace_id\|tenant_id" crates/tinyiothub-storage/src` | 无输出 (PASS) |
| Engine 编译验证 | `cargo check -p tinyiothub-engine` | 成功 |
| Workspace 全量测试 | `cargo test --workspace` | 全部通过 |

### 4.2 手动 Code Review 检查点

- PR 新增 `pub use` 语句：检查是否意外暴露了内部模块。
- `cloud/src/domain` 变更：检查是否有逻辑被错误地移入了 `crates/`。
- `tinyiothub-storage` 新增 SQL 文件：检查是否包含 `workspace_id` 过滤条件。

## 5. 违规处理与豁免流程

1. **违规发现**：CI 失败或 Code Review 指出。
2. **处理方式**：
   - **即时修复**：如果是明确的越界（如 core 引用了 sqlx），必须立即修复。
   - **豁免申请**：如果因工期问题需要临时妥协，必须在代码中添加 `// HACK: Violates Architecture Contract. Reason: ... Fix by: YYYY-MM-DD`。
   - **债务追踪**：所有 `HACK` 注释将被自动扫描并生成技术债报表。
