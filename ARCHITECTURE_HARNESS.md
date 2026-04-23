# ARCHITECTURE_HARNESS.md

> **⚠️ 这是一份强制执行的架构契约。所有 AI 代码必须遵守，违者 PR 拒绝合并。**

---

## 零、先查后写（强制步骤）

**每次写代码之前，AI 必须执行以下搜索：**

```bash
# 1. 查找类似的已有实现
grep -r "类似功能名" api/src/shared/
grep -r "类似功能名" api/src/domain/
grep -r "类似功能名" web/service/

# 2. 查找可复用的公共组件
ls api/src/shared/
ls api/src/domain/*/services/
ls web/service/
```

**如果找到复用目标，却不使用 → PR 直接拒绝。**

---

## 一、模块边界（绝对禁止跨边界私自造轮）

**注意：workspace-refactor 分支采用多 Crate 架构，详见 `ARCHITECTURE_CONTRACT.md`。**

### 1.1 Crate 依赖方向（单向不可逆）
```
┌─────────────────────────────────────────────────────────┐
│                         cloud (Binary)                  │
│  - Tenant Logic, Workspace Routes, ServiceOrchestrator  │
└───────────────────────────────┬─────────────────────────┘
                                │ 依赖
                                ▼
┌─────────────────────────────────────────────────────────┐
│                      tinyiothub-web (HTTP)              │
│  - Generic IoT Handlers, Logging Middleware             │
└───────────────────────────────┬─────────────────────────┘
                                │ 依赖
                                ▼
┌─────────────────────────────────────────────────────────┐
│                     tinyiothub-engine (IoT)             │
│  - Collector, Scheduler, Rule, Alarm, Template          │
└───────────────────────────────┬─────────────────────────┘
                                │ 依赖 (Traits only)
                                ▼
┌─────────────────────────────────────────────────────────┐
│                    tinyiothub-storage (Data)            │
│  - DeviceRepo Trait, TelemetryRepo Trait, Cache         │
└───────────────────────────────┬─────────────────────────┘
                                │ 依赖
                                ▼
┌─────────────────────────────────────────────────────────┐
│                     tinyiothub-core (Types)             │
│  - Device, Event, TelemetryPoint, Error                 │
└─────────────────────────────────────────────────────────┘
```

### 1.2 核心规则
- ❌ 禁止反向依赖（如 `core` 依赖 `web`）
- ❌ 禁止循环依赖
- ❌ 禁止跨 Crate 直接访问内部模块（必须通过定义好的接口）
- ✅ 新功能先想清楚属于哪个现有 Crate
- ✅ 跨 Crate 调用必须走定义好的接口

**规则：**
- ❌ 禁止在任何 Crate 中创建散弹式的 `utils/`、`helpers/`、`tools/` 目录（公共组件应放在 `cloud/src/shared/` 或相应 Crate 的 `shared/` 模块）
- ❌ 禁止在 `domain/` 直接调用 DB（必须通过 repository interface）
- ❌ 禁止在 HTTP handlers 里直接写 SQL（用 SQLx query builder）
- ✅ 新功能先想清楚属于哪个现有 Crate
- ✅ 跨 Crate 调用必须走定义好的接口（traits）

---

## 二、必须复用的公共组件（禁止重复实现）

### 2.1 Rust 后端 — 公共组件清单（多 Crate 架构）

**核心原则：先搜索 `cloud/src/shared/` 和相应 Crate 的公共模块，找不到再考虑新建。**

| 组件位置 | 用途 | 禁止做的事 |
|---------|------|-----------|
| `cloud/src/shared/error_handling.rs` | 统一错误处理 | 禁止自定义 `anyhow::Error` |
| `cloud/src/shared/security/` | JWT 工具、加密 | 禁止自己实现 JWT |
| `cloud/src/shared/identifier.rs` | ID 生成 | 禁止自己写 UUID |
| `cloud/src/shared/command.rs` | 命令执行 | 禁止直接 `std::process::Command` |
| `tinyiothub-web::response::ApiResponseBuilder` | API 响应 | 禁止自己拼 JSON 响应 |
| `cloud/src/infrastructure/config/` | 配置管理 | 禁止硬编码配置 |
| `cloud/src/infrastructure/persistence/database.rs` | 数据库连接 | 禁止自己建连接池 |
| `tinyiothub-error` crate | 错误类型（带 `thiserror`） | 禁止重复定义相似错误 |
| `tinyiothub-core` 中的领域类型 | 设备、事件、遥测点等 | 禁止在 SaaS 层重复定义 |

**注意：**
- `tinyiothub-web` crate 提供 `ApiResponseBuilder`，通过 `use tinyiothub_web::response::ApiResponseBuilder` 导入
- `cloud` 二进制可以依赖所有内部 crates，但内部 crates 不能反向依赖 `cloud`
- 通用工具函数应放在 `cloud/src/shared/utils/` 而非各 Crate 重复实现

### 2.2 TypeScript 前端 — 公共组件清单

| 组件位置 | 用途 | 禁止做的事 |
|---------|------|-----------|
| `web/src/api/client.ts` | HTTP 请求 | 禁止直接 `fetch()` |
| `web/src/api/*.ts` | API 调用封装 | 禁止在组件里直接调 API |
| `web/src/stores/*.ts` | nanostore 状态管理 | 禁止在组件里直接管理全局状态 |

---

## 三、API 响应格式（强制格式）

所有 API 必须返回：

```json
{
  "code": 0,
  "msg": "success",
  "result": T | null
}
```

**禁止：**
- ❌ `{"success": true, "data": ...}`
- ❌ `{"status": "ok", "payload": ...}`
- ❌ `{"code": 200, "message": ...}` （必须用 msg）
- ❌ 直接返回 `null` 而没有 code

**使用：**
```rust
// ✅ 正确
ApiResponseBuilder::success(data)
ApiResponseBuilder::error("错误信息")

// ❌ 错误
return Json(serde_json::to_value(&data).unwrap());
```

---

## 四、数据库操作规范

### 4.1 SQL 查询（必须用 SQLx）
```rust
// ✅ 正确
let device = sqlx::query_as::<_, Device>("SELECT * FROM devices WHERE id = ?")
    .bind(id)
    .fetch_one(&pool)
    .await?;

// ❌ 错误
let device = conn.query_row("SELECT * FROM devices", [], ...)?;
```

### 4.2 Repository 模式
- 所有 DB 访问必须在 `infrastructure/persistence/repositories/`
- domain 层只定义 trait（interface），不实现
- 禁止在 `api/` 或 `application/` 直接写 SQL

---

## 五、前端强制规范

### 5.1 API 调用流程（禁止绕过）
```
组件 → stores / service → api-client → fetch
```

**禁止：**
- ❌ 组件里直接 `fetch('/api/v1/...')`
- ❌ 组件里直接 `axios.post(...)`
- ❌ 组件里直接管理全局状态（绕过 nanostore）

**正确：**
```typescript
// 1. 在 api/ 创建或使用现有 API 封装
// web/src/api/devices.ts
export const deviceApi = {
  getList: (params?: { page?: number; pageSize?: number }) =>
    apiGet<Device[]>('devices', params),
  create: (data: CreateDeviceRequest) => apiPost<Device>('devices', data),
};

// 2. 在 stores/ 创建或使用现有 store（需要时）
// web/src/stores/devices.ts
import { atom } from 'nanostores'

export const $deviceList = atom<Device[]>([])

export async function loadDevices(params?: { page?: number; pageSize?: number }) {
  const response = await deviceApi.getList(params)
  $deviceList.set(response.result || [])
}

// 3. 组件中调用 store 或 api 层
// web/src/ui/views/device-list.ts
import { $deviceList, loadDevices } from '../../stores/devices'

export class DeviceList extends LitElement {
  @state() private devices: Device[] = []

  connectedCallback() {
    super.connectedCallback()
    this._unsubscribe = $deviceList.subscribe((list) => {
      this.devices = list
    })
    loadDevices()
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    this._unsubscribe?.()
  }
}
```

### 5.2 组件规范
- ✅ 一个功能一个文件（不打包多个功能到一个文件）
- ✅ `PascalCase` 命名组件
- ✅ `kebab-case` 命名非组件文件

---

## 六、测试要求（PR 必须满足）

### 6.1 Rust 后端
- ✅ 核心 domain 逻辑必须有单元测试
- ✅ 新增的 handler 必须有集成测试
- ✅ 运行 `cargo test` 必须通过
- ✅ 运行 `cargo clippy -- -D warnings` 必须无警告

### 6.2 TypeScript 前端
- ✅ 工具函数必须有单元测试
- ✅ 组件必须有基本渲染测试
- ✅ 运行 `pnpm type-check` 必须通过
- ✅ 运行 `pnpm test` 必须通过

---

## 七、Git 提交规范（强制格式）

```
<type>(<scope>): <简短描述>

type: feat | fix | test | chore | docs | refactor | style
scope: 具体模块，如 api, web, alarm, device
```

**例子：**
```
feat(device): 添加设备温度监控功能
fix(auth): 修复 JWT 过期后无法刷新的问题
test(alarm): 添加告警规则引擎单元测试
```

**禁止：**
- ❌ `update stuff`
- ❌ `fix bug`
- ❌ `WIP`
- ❌ 纯中文描述（国际项目应用英文）

---

## 八、代码复制检测（CI 自动检查）

如果 AI 写的代码与现有代码相似度超过 80 行，CI 必须报错。

**复用的正确姿势：**
1. 找到目标文件
2. import / use
3. 如需修改，抽象成公共函数再调用

---

## 九、架构违规处理

| 违规类型 | 处理方式 |
|---------|---------|
| 未查找复用就重复实现 | PR 拒绝，要求先找复用 |
| 绕过 service 层直接 API 调用 | PR 拒绝，重构 |
| API 响应格式不一致 | PR 拒绝，统一格式 |
| 未添加测试 | PR 拒绝，补测试 |
| 提交格式不规范 | PR 拒绝，规范提交信息 |

---

## 十、项目特定：TinyIoTHub 关键约束

### 10.1 设备驱动
- 所有协议驱动必须在 `domain/device/driver/drivers/`
- 禁止在其他位置创建驱动实现
- 新协议（Modbus/ONVIF/SNMP/MQTT）必须注册到 driver registry

### 10.2 告警系统
- 告警规则引擎在 `domain/alarm/services/rule_engine.rs`
- 禁止在 handlers 里写告警逻辑

### 10.3 事件系统
- 实时事件走 SSE（`infrastructure/event/sse_manager.rs`）
- 事件持久化在 `infrastructure/event/handlers/persistence_handler.rs`

---

_本文件是 TinyIoTHub 的架构宪法。所有 AI 代码必须遵守。_
_违反此文件的 PR 不会被接受。_
