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

```
┌─────────────────────────────────────────────────────────┐
│  api/src/domain/        # 核心业务逻辑，绝对不可侵犯    │
│  api/src/application/  # 应用编排，禁止放业务逻辑      │
│  api/src/infrastructure/  # 外部依赖（DB/MQTT/GPIO）   │
│  api/src/shared/       # 公共组件，所有人共享          │
│  api/src/dto/          # 数据传输对象，禁止放业务逻辑  │
│  api/src/api/          # HTTP handlers + 业务逻辑      │
└─────────────────────────────────────────────────────────┘
```

**规则：**
- ❌ 禁止在 `api/` 中创建 `utils/`、`helpers/`、`tools/` 散弹文件
- ❌ 禁止在 `domain/` 直接调用 DB（必须通过 repository interface）
- ❌ 禁止在 `api/` handlers 里直接写 SQL（用 SQLx query builder）
- ✅ 新功能先想清楚属于哪个现有模块
- ✅ 跨模块调用必须走定义好的接口

---

## 二、必须复用的公共组件（禁止重复实现）

### 2.1 Rust 后端 — 公共组件清单

| 组件位置 | 用途 | 禁止做的事 |
|---------|------|-----------|
| `shared/error.rs` | 统一错误类型 | 禁止自定义 `anyhow::Error` |
| `shared/security/jwt.rs` | JWT 工具 | 禁止自己实现 JWT |
| `shared/identifier.rs` | ID 生成 | 禁止自己写 UUID |
| `shared/command.rs` | 命令执行 | 禁止直接 `std::process::Command` |
| `dto/response/builder.rs` | API 响应 | 禁止自己拼 JSON 响应 |
| `infrastructure/config/` | 配置管理 | 禁止硬编码配置 |
| `infrastructure/persistence/database.rs` | 数据库连接 | 禁止自己建连接池 |

### 2.2 TypeScript 前端 — 公共组件清单

| 组件位置 | 用途 | 禁止做的事 |
|---------|------|-----------|
| `web/lib/api-client.ts` | HTTP 请求 | 禁止直接 `fetch()` |
| `web/service/*.ts` | API 调用封装 | 禁止在组件里直接调 API |
| `web/hooks/*.ts` | React Query hooks | 禁止在组件里直接用 `useQuery` |
| `web/lib/query-keys.ts` | 查询缓存 key | 禁止自己拼接 query key |

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
组件 → hooks → service → api-client → fetch
```

**禁止：**
- ❌ 组件里直接 `fetch('/api/v1/...')`
- ❌ 组件里直接 `axios.post(...)`
- ❌ 组件里直接用 `useQuery`

**正确：**
```typescript
// 1. 在 service/ 创建或使用现有 service
// web/service/devices.ts
export const deviceService = {
  getList: (params) => apiGet('/api/v1/devices', params),
  create: (data) => apiPost('/api/v1/devices', data),
};

// 2. 在 hooks/ 创建或使用现有 hook
// web/hooks/use-devices.ts
export const useDevices = (params) => useQuery({
  queryKey: queryKeys.devices.list(params),
  queryFn: () => deviceService.getList(params),
});

// 3. 组件只调用 hook
// DeviceList.tsx
const { data } = useDevices(params);
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
