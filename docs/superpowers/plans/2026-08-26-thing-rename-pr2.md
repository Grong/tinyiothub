# device→thing 收尾全量实施计划（PR-2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在单个 PR 内完成 device→thing 的全部收尾：tag_bindings/permissions 契约数据迁移、core/db/cloud 的 Device\*→Thing\* 类型与模块改名、驱动 wire 协议直接改名、MCP advertised schema 主键翻转、前端适配、admin 监控路由整理。

**Architecture:** 延续 PR-1 验证过的模式：perl 全词替换（按类型名显式清单 + 函数名模式规则）+ 编译器驱动修正；数据契约用一个小迁移完成（UPDATE 而非 DDL)；每个 crate 一个任务、自带质量门，workspace 在 wire 任务结束时全绿。

**Tech Stack:** Rust + sqlx(SQLite)、axum;前端 TypeScript(Vite,`pnpm type-check && pnpm build`)。

## Global Constraints

- **分支**：从 `refactor/thing-db-rename`（PR-1,HEAD `cbfd1692`）切出 `refactor/thing-rename-pr2`;PR stacked,base 为 `refactor/thing-db-rename`。
- **绝不修改**已应用的迁移文件；新变更只加一个迁移 `crates/db/migrations/20260826000001_thing_contract_data.sql`。
- **crate/lib 名**:package `db` → lib `tinyiothub_storage`;cloud package `tinyiothub-cloud`;core package `core`。
- **契约数据迁移（verbatim SQL,Task 1)**:
  ```sql
  UPDATE tag_bindings SET target_type = 'thing' WHERE target_type = 'device';
  UPDATE role_permissions SET permission_id = 'perm-thing-' || substr(permission_id, 13) WHERE permission_id LIKE 'perm-device-%';
  UPDATE user_permissions SET permission_id = 'perm-thing-' || substr(permission_id, 13) WHERE permission_id LIKE 'perm-device-%';
  UPDATE permissions SET id = 'perm-thing-' || substr(id, 13),
                         name = 'thing:' || substr(name, 8),
                         resource_type = 'thing'
  WHERE resource_type = 'device';
  ```
  （顺序：子表先、permissions 后主表后；迁移器 FK OFF，顺序仅为逻辑清晰。)
- **权限字符串映射**:`device:read`→`thing:read`、`device:write`→`thing:write`、`device:delete`→`thing:delete`、`device:admin`→`thing:admin`;`resource_type 'device'`→`'thing'`;perm id `perm-device-*`→`perm-thing-*`。
- **tag 写入侧裁决**:`BindTagRequest.target_type` 接受 `'device'` 时在 handler 边界归一化为 `'thing'`（防御性，3 行，注释说明）；存储与查询只认 `'thing'`。
- **core 类型改名映射（verbatim,`\b` 边界、大小写敏感）**:
  `Device`→`Thing`、`DeviceStatus`→`ThingStatus`、`DeviceQueryParams`→`ThingQueryParams`、`CreateDeviceRequest`→`CreateThingRequest`、`UpdateDeviceRequest`→`UpdateThingRequest`、`DeviceStats`→`ThingStats`、`DeviceStatusUpdate`→`ThingStatusUpdate`、`DeviceCommand`→`ThingCommand`、`CreateDeviceCommandRequest`→`CreateThingCommandRequest`、`UpdateDeviceCommandRequest`→`UpdateThingCommandRequest`、`DeviceCommandQueryParams`→`ThingCommandQueryParams`、`DeviceCommandStatistics`→`ThingCommandStatistics`、`DeviceTemplate`→`ThingTemplate`、`DeviceInfo`→`ThingInfo`、`CreateDeviceTemplateRequest`→`CreateThingTemplateRequest`、`UpdateDeviceTemplateRequest`→`UpdateThingTemplateRequest`、`DeviceCreationInput`→`ThingCreationInput`、`DevicePreview`→`ThingPreview`、`CreateDeviceFromTemplateRequest`→`CreateThingFromTemplateRequest`、`DeviceProperty`→`ThingProperty`、`DevicePropertyQueryParams`→`ThingPropertyQueryParams`、`CreateDevicePropertyRequest`→`CreateThingPropertyRequest`、`UpdateDevicePropertyRequest`→`UpdateThingPropertyRequest`、`DevicePropertyStats`→`ThingPropertyStats`、`DeviceEventType`→`ThingEventType`、`DeviceConfig`→`ThingConfig`(core/src/config.rs)。
- **db 类型改名映射**:`DeviceCriteria`→`ThingCriteria`、`DeviceSortBy`→`ThingSortBy`、`DeviceSortOrder`→`ThingSortOrder`、`DeviceCriteriaBuilder`→`ThingCriteriaBuilder`、`DeviceStatusDistribution`→`ThingStatusDistribution`、`QuickDevice`→`QuickThing`、`DeviceTrace`→`ThingTrace`、`DeviceTraceStatistics`→`ThingTraceStatistics`、`DeviceStatusSummary`→`ThingStatusSummary`、db 侧 `DeviceTemplate`→`ThingTemplate`、db 侧 `DeviceInfo`→`ThingInfo`、`DeviceCache`→`ThingCache`。已 thing 命名的（`ThingRow`、`OpenThingRow`、`OpenThingDetailRow`、`OpenThingPropertyRow`、`OpenThingCommandRow`）不动。
- **模块/文件改名映射**:
  - core:`models/device.rs`→`models/thing.rs`、`models/device_command.rs`→`models/thing_command.rs`、`models/device_template.rs`→`models/thing_template.rs`、`models/device_property.rs`→`models/thing_property.rs`(core::models 当前无 thing 模块，无冲突）
  - db:`device_row_mapper.rs`→`thing_row_mapper.rs`、`device_command.rs`→`thing_command.rs`、`device_property.rs`→`thing_property.rs`、`cache/device_cache.rs`→`cache/thing_cache.rs`
  - cloud:`domains/admin/device/`→`domains/admin/thing/`、`domains/mcp/tools/device.rs`→`domains/mcp/tools/thing.rs`、`domains/thing/legacy/device_query.rs`→`domains/thing/legacy/thing_query.rs`
- **db 函数改名模式**:70 个 `device_*` 自由函数与对应 `impl Db` 方法——`find_device_by_id`→`find_thing_by_id`、`find_devices`→`find_things`、`count_devices`→`count_things`、`quick_devices`→`quick_things`、`ensure_devices_table`→`ensure_things_table` 等；perl 规则 `s/\b(\w+)_devices\b/$1_things/g; s/\b(\w+)_device\b/$1_thing/g` + 编译驱动兜底。
- **wire 直接改名（用户裁决：破坏式，无兼容期）**:MQTT/插件 payload JSON key `deviceId`→`thingId`、`deviceType`→`category`;heartbeat action content `"deviceId"`→`"thingId"`;LLM heartbeat 契约 `"device_id"`→`"thing_id"`(prompt 与 parser 同改）；plugin-sdk `Device`→`Thing` 及其 wire 字段；edge/drivers 同步；gateway wire `DiscoveredDevice.device_type`→`category`、`PairingAck.device_id`→`thing_id`;plugin storage sink 默认列/tag `device_id`→`thing_id`(postgres.rs/influxdb.rs,CHANGELOG 注明外部 sink 破坏）;MQTT topic 若含 `/devices/` 段一并改 `/things/`(grep 确认后执行）。
- **MCP advertised schema**:input_schema 主键 `deviceId`→`thingId`、`targetDeviceId`→`targetThingId`;serde 翻转为 `rename = "thingId", alias = "deviceId"`(PR-1 的 alias 方向反转，旧客户端仍可用）;PR-1 的 6 个契约测试同步更新（主键断言 + alias 兼容断言）。MCP 工具名（read_properties/write_properties/send_command 等）已是中性的，不改。
- **admin 路由终态（Task 4 定稿，实现时以路由测试验证）**:admin device 路由组从 `/api/v1/devices` 迁移到 `/api/v1/things/admin`(`/{id}/status`、`/{id}/metrics`、`/{id}/performance*`、`/overview`、`/{id}/traces*`、`/system/traces/*`、`/distribution`、`/quick`、`/{id}/profile` 等）；若某端点与 thing 主路由重复，删 admin 副本、以前端指向主路由；410 tombstone 保持挂在 `/api/v1/devices`，文案不变。axum/matchit 静态段 `admin` 优先于 `/{id}`，无双挂载；若有冲突以启动 panic/路由测试为信号调整。
- **前端映射**:`interface Device`→`Thing`、`DeviceProperty`→`ThingProperty`、`DeviceCommand`→`ThingCommand`、`DeviceAlarm`→`ThingAlarm`、`DeviceListParams`→`ThingListParams`；字段 `deviceId`→`thingId`、`deviceType`→`category`;API 路径 `/devices/*`→Task 4 终态路径；SPA 路由 `/devices/${id}`→`/things/${id}`;`stores/device-cache.ts`→`thing-cache.ts`;a2ui catalog `device-card`/`device-table`→`thing-card`/`thing-table`（组件类型字符串同改，LLM 契约，CHANGELOG 注明）;`targetType: 'device'`→`'thing'`(web/src/types/index.ts:494、things.ts:649/672)。
- **每任务质量门**见任务内；最终门禁：`cargo test --workspace` 全绿、`cargo clippy --workspace --exclude zeroclaw --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`pnpm type-check && pnpm build`(web/)。
- **不改**：已应用迁移、baseline;`events.source_type='device'`、`actor='device'` 事件来源语义值；jobs CHECK 的 `'device_command'` job_type 值（业务枚举，随 cron config 键已由 PR-1 改为 thing_id，枚举值本身属后续单独评估——本 PR 不动并在 CHANGELOG 记录）。

---

### Task 1: 契约数据迁移（tag_bindings + permissions)

**Files:**
- Create: `crates/db/migrations/20260826000001_thing_contract_data.sql`
- Create: `crates/db/tests/thing_contract_data_tests.rs`
- Modify: `crates/db/src/seed/system.sql`(permissions 4 行 + role_permissions 引用行）
- Modify: `crates/db/src/seed/demo.sql`(tag_bindings target_type 'device'→'thing')
- Modify: `crates/db/src/thing.rs`(4 处查询 `target_type = 'device'`→`'thing'`，约 1778/1787/1865/1874 行；`:705` 的 `IN ('device','thing')`→`= 'thing'`)
- Modify: `crates/db/src/permission.rs`(测试内 "device:read" 等→thing 系）
- Modify: `apps/cloud/src/domains/thing/tag/handler.rs:399`（写入边界归一化）

**Interfaces:**
- Consumes: PR-1 迁移链（HEAD 已含 `20260825000001`)。
- Produces: 迁移后 `tag_bindings.target_type ∈ {'app','thing'}`(app 行不动）、`permissions` 无 device 系行；后续任务依赖的代码常量 `THING_TARGET_TYPE` 不需要——直接用字面量 `'thing'`。

- [ ] **Step 0: 切分支 + 前置核对**

```bash
git checkout -b refactor/thing-rename-pr2
grep -n "target_type" crates/db/migrations/20260819000001_baseline.sql | head   # tag_bindings 定义,确认无 CHECK
grep -rn "perm-device-" crates/db/src/seed/system.sql                            # role_permissions/user_permissions 引用行全清单
grep -rn "device:read\|device:write\|device:delete\|device:admin" apps/cloud/src crates --include="*.rs" -l   # 期望:permission.rs + role_handler_tests.rs(+可能的其他执行点,全部列出)
grep -rn "'device'" crates/db/src/tag.rs | head   # tag.rs 校验/默认值处
```

- [ ] **Step 1: 写失败测试** `crates/db/tests/thing_contract_data_tests.rs`（沿用 `thing_rename_data_tests.rs` 的 baseline→migrate 模式与 `tinyiothub_storage::migrations::run_migrations`):

```rust
//! 契约数据迁移测试:tag_bindings.target_type 与 permissions device 系行。

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

async fn baseline_pool_with_samples() -> (SqlitePool, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "thing-contract-{}-{}.db",
        std::process::id(),
        std::sync::atomic::AtomicU32::new(0).fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePoolOptions::new().max_connections(1).connect(&url).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/20260819000001_baseline.sql"))
        .execute(&pool).await.unwrap();
    // 标记 baseline + 后续已应用(checksum 取自嵌入迁移集),只跑新迁移。
    // 复用 thing_rename_data_tests.rs 的标记方式;这里直接全量跑 run_migrations 亦可——
    // baseline 已直建,需先标记 baseline+20260824000001+20260824000002+20260825000001 已应用。
    (pool, path)
}

#[tokio::test]
async fn contract_data_migrated() {
    let (pool, path) = baseline_pool_with_samples().await;
    // 样本:tag 绑定 'device' + 'app' 各一;permissions 4 个 device 系 + 1 个 user 系;role_permissions 引用
    sqlx::query("INSERT INTO tags (id, type, name) VALUES ('t1', 'thing', 'tag1')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO things (id, name, thing_type) VALUES ('d1', 'demo', 'device')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO tag_bindings (id, tag_id, target_id, target_type) VALUES ('b1', 't1', 'd1', 'device')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO tag_bindings (id, tag_id, target_id, target_type) VALUES ('b2', 't1', 'd1', 'app')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO permissions (id, name, resource_type, action) VALUES ('perm-device-read', 'device:read', 'device', 'read')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO permissions (id, name, resource_type, action) VALUES ('perm-user-read', 'user:read', 'user', 'read')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO roles (id, name) VALUES ('r1', 'role1')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO role_permissions (id, role_id, permission_id) VALUES ('rp1', 'r1', 'perm-device-read')").execute(&pool).await.unwrap();

    // 标记已应用 + 跑迁移(与 thing_rename_data_tests 相同的 _sqlx_migrations 标记法)
    mark_applied_and_migrate(&pool).await;

    let tt: String = sqlx::query_scalar("SELECT target_type FROM tag_bindings WHERE id='b1'").fetch_one(&pool).await.unwrap();
    assert_eq!(tt, "thing");
    let app: String = sqlx::query_scalar("SELECT target_type FROM tag_bindings WHERE id='b2'").fetch_one(&pool).await.unwrap();
    assert_eq!(app, "app", "非 device 行不得受影响");
    let perm: (String, String, String) = sqlx::query_as("SELECT id, name, resource_type FROM permissions WHERE action='read' AND resource_type='thing'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(perm, ("perm-thing-read".into(), "thing:read".into(), "thing".into()));
    let rp: String = sqlx::query_scalar("SELECT permission_id FROM role_permissions WHERE id='rp1'").fetch_one(&pool).await.unwrap();
    assert_eq!(rp, "perm-thing-read");
    let user_perm: String = sqlx::query_scalar("SELECT name FROM permissions WHERE id='perm-user-read'").fetch_one(&pool).await.unwrap();
    assert_eq!(user_perm, "user:read", "user 系权限不得受影响");
    let fk: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check").fetch_one(&pool).await.unwrap();
    assert_eq!(fk, 0);
    let _ = std::fs::remove_file(path);
}
```

注：`mark_applied_and_migrate` 的实现从 `thing_rename_data_tests.rs` 原样复制（读其源码）；roles INSERT 列清单按 baseline `roles` DDL 实际列调整（编译/运行错误驱动）。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p db --test thing_contract_data_tests`
Expected: FAIL(`no such migration`/断言值为旧名）

- [ ] **Step 3: 写迁移** `crates/db/migrations/20260826000001_thing_contract_data.sql`（内容即 Global Constraints 的 verbatim SQL 块 + 头部注释说明用途）。

- [ ] **Step 4: 代码与 seed 翻转**

1. `crates/db/src/thing.rs`:4 处 `target_type = 'device'`→`'thing'`(search_text/tag_name 过滤）;`:705` `IN ('device','thing')`→`= 'thing'`。
2. `apps/cloud/src/domains/thing/tag/handler.rs`（约 399 行）写入边界归一化：
   ```rust
   let target_type = if request.target_type == "device" { "thing".to_string() } else { request.target_type.clone() };
   ```
   （加注释：过渡期归一化，存储只认 'thing'。)
3. `crates/db/src/permission.rs` 测试字符串 device 系→thing 系（"device:read"→"thing:read" 等，含 `allows_action` 断言参数）。
4. `crates/db/src/seed/system.sql`:`perm-device-*`→`perm-thing-*`、`device:*`→`thing:*`、resource_type、description 中文"设备"文案保留；role_permissions/user_permissions seed 引用行同步。
5. `crates/db/src/seed/demo.sql`:tag_bindings 的 `target_type = 'device'`/`'device'` 值→`'thing'`。
6. Step 0 grep 列出的其他执行点（如 role_handler_tests.rs）逐一改。

- [ ] **Step 5: 验证**

```bash
cargo test -p db   # 全绿(含新契约测试 + seed_tests)
grep -rn "target_type = 'device'\|perm-device-\|device:read\|device:write\|device:delete\|device:admin" crates/db apps/cloud/src --include="*.rs" --include="*.sql"
# 期望:仅迁移文件自身(WHERE 子句)与 thing_rename 历史测试命中
```

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(db): migrate tag_bindings/permissions contract data to thing naming"
```

---

### Task 2: crates/core 类型与模块改名

**Files:**
- Rename: `crates/core/src/models/device.rs`→`thing.rs`、`device_command.rs`→`thing_command.rs`、`device_template.rs`→`thing_template.rs`、`device_property.rs`→`thing_property.rs`
- Modify: `crates/core/src/models/mod.rs`、`crates/core/src/config.rs`、`crates/core/src/models/event/**` 及全部引用点

**Interfaces:**
- Consumes: Task 1 完成态。
- Produces: `core::models::thing::{Thing, ThingStatus, ThingQueryParams, CreateThingRequest, UpdateThingRequest, ThingStats, ThingStatusUpdate}` 等（全清单见 Global Constraints 映射表）;Task 3/4/5 的类型来源。

- [ ] **Step 1: 模块改名 + 类型 perl 替换**

```bash
cd crates/core/src/models
git mv device.rs thing.rs && git mv device_command.rs thing_command.rs && git mv device_template.rs thing_template.rs && git mv device_property.rs thing_property.rs
cd -
# mod.rs 模块声明同步(device→thing 等 4 行)
files=$(grep -rln "Device" crates/core/src --include="*.rs")
perl -pi -e '
  s/\bCreateDeviceCommandRequest\b/CreateThingCommandRequest/g;
  s/\bUpdateDeviceCommandRequest\b/UpdateThingCommandRequest/g;
  s/\bDeviceCommandQueryParams\b/ThingCommandQueryParams/g;
  s/\bDeviceCommandStatistics\b/ThingCommandStatistics/g;
  s/\bCreateDeviceTemplateRequest\b/CreateThingTemplateRequest/g;
  s/\bUpdateDeviceTemplateRequest\b/UpdateThingTemplateRequest/g;
  s/\bCreateDeviceFromTemplateRequest\b/CreateThingFromTemplateRequest/g;
  s/\bDevicePropertyQueryParams\b/ThingPropertyQueryParams/g;
  s/\bCreateDevicePropertyRequest\b/CreateThingPropertyRequest/g;
  s/\bUpdateDevicePropertyRequest\b/UpdateThingPropertyRequest/g;
  s/\bDevicePropertyStats\b/ThingPropertyStats/g;
  s/\bDeviceCreationInput\b/ThingCreationInput/g;
  s/\bDeviceStatusUpdate\b/ThingStatusUpdate/g;
  s/\bDeviceQueryParams\b/ThingQueryParams/g;
  s/\bDeviceEventType\b/ThingEventType/g;
  s/\bDeviceTemplate\b/ThingTemplate/g;
  s/\bDevicePreview\b/ThingPreview/g;
  s/\bDeviceCommand\b/ThingCommand/g;
  s/\bDeviceProperty\b/ThingProperty/g;
  s/\bDeviceStatus\b/ThingStatus/g;
  s/\bDeviceStats\b/ThingStats/g;
  s/\bDeviceInfo\b/ThingInfo/g;
  s/\bDeviceConfig\b/ThingConfig/g;
  s/\bDevice\b/Thing/g;
' $files
```

- [ ] **Step 2: 编译修正循环**

```bash
cargo check -p core 2>&1 | grep -E "^error" | head -20
```

修正规则：`mod.rs` 的 `pub mod device;`→`pub mod thing;`（4 个模块）;`use crate::models::device::`→`models::thing::` 路径；`EventType::Device` 枚举变体名——**保留 `Device` 变体名不改**（事件分类语义，序列化值 "device" 已裁决保留），仅其 payload 类型 `DeviceEventType`→`ThingEventType` 随映射改；`DeviceInfo`（模板内嵌设备信息）按映射→`ThingInfo`。

- [ ] **Step 3: 验证 + 下游错误盘点**

```bash
cargo test -p core 2>&1 | tail -5   # 全绿
cargo check --workspace 2>&1 | grep -cE "^error"   # 记录数字,报告列出前 20 个——这些是 Task 3/4/5 的工作面
grep -rn "\bDevice[A-Z]\w*\b\|\bDevice\b" crates/core/src --include="*.rs" | grep -v "ThingEventType\|EventType::Device\|//" | head
# 期望:仅 EventType::Device 变体与注释命中
```

- [ ] **Step 4: Commit**

```bash
git add -A crates/core && git commit -m "refactor(core)!: rename Device models to Thing (types + modules)"
```

---

### Task 3: crates/db 类型、函数与模块改名

**Files:**
- Rename: `crates/db/src/device_row_mapper.rs`→`thing_row_mapper.rs`、`device_command.rs`→`thing_command.rs`、`device_property.rs`→`thing_property.rs`、`cache/device_cache.rs`→`cache/thing_cache.rs`
- Modify: `crates/db/src/thing.rs`(70 个 fn + Device\* 类型）、`thing_trace.rs`、`thing_template.rs`、`event.rs`、`lib.rs` 及全部引用点

**Interfaces:**
- Consumes: Task 2 的 `core::models::thing::*`。
- Produces: `tinyiothub_storage::thing::{ThingCriteria, ThingSortBy, ThingSortOrder, ThingCriteriaBuilder, ThingStatusDistribution, QuickThing, find_thing_by_id, find_things, count_things, quick_things, ensure_things_table, ...}`、`thing_trace::{ThingTrace, ThingTraceStatistics}`、`thing_template::ThingTemplate`、`thing_command/thing_property`(wire 模块，字段在 Task 5 改）、`cache::thing_cache::ThingCache`。

- [ ] **Step 1: 文件改名 + perl 替换**

```bash
cd crates/db/src
git mv device_row_mapper.rs thing_row_mapper.rs && git mv device_command.rs thing_command.rs && git mv device_property.rs thing_property.rs && git mv cache/device_cache.rs cache/thing_cache.rs
cd -
files=$(grep -rln "Device\|_device\b\|_devices\b" crates/db/src crates/db/tests --include="*.rs")
perl -pi -e '
  s/\bDeviceStatusDistribution\b/ThingStatusDistribution/g;
  s/\bDeviceCriteriaBuilder\b/ThingCriteriaBuilder/g;
  s/\bDeviceTraceStatistics\b/ThingTraceStatistics/g;
  s/\bDeviceStatusSummary\b/ThingStatusSummary/g;
  s/\bDeviceCriteria\b/ThingCriteria/g;
  s/\bDeviceSortBy\b/ThingSortBy/g;
  s/\bDeviceSortOrder\b/ThingSortOrder/g;
  s/\bDeviceTemplate\b/ThingTemplate/g;
  s/\bDeviceTrace\b/ThingTrace/g;
  s/\bDeviceInfo\b/ThingInfo/g;
  s/\bDeviceCache\b/ThingCache/g;
  s/\bQuickDevice\b/QuickThing/g;
  s/\b(\w+)_devices\b/$1_things/g;
  s/\b(\w+)_device\b/$1_thing/g;
  s/\bdevice_row_mapper\b/thing_row_mapper/g;
  s/\bdevice_command\b/thing_command/g;
  s/\bdevice_property\b/thing_property/g;
  s/\bdevice_cache\b/thing_cache/g;
' $files
```

- [ ] **Step 2: 编译修正循环**

```bash
cargo test -p db 2>&1 | grep -E "^error" | head -30
```

修正规则：
- core 类型引用同步到 Task 2 新名（`models::device::Device`→`models::thing::Thing` 等）；注意 `Device`/`CreateDeviceRequest` 等 core 类型已在 Task 2 改名，本任务 perl 的 `\bDevice\b` 不在 db 文件清单规则内——db 里残留的 core 类型名引用手工随编译错误改。
- `DeviceSortBy::DeviceType` 枚举变体→`ThingSortBy::Category`（值已对应 category 列；变体改名后检查 `match` 穷尽性）。
- wire 模块（thing_command.rs/thing_property.rs）的 Rust 字段 `device_id` **本任务不动**(Task 5 统一改），仅模块名/文件名变。
- `impl Db` 委托方法名随自由函数改；`lib.rs` 模块声明同步。
- migrations.rs 测试名 `delete_from_devices_works_after_migrations`→`delete_from_things_works_after_migrations`(PR-1 deferred minor，顺手处理）。

- [ ] **Step 3: 验证**

```bash
cargo test -p db   # 全绿
grep -rn "\bDevice[A-Z]\w*\b" crates/db/src --include="*.rs" | grep -v "OpenThing\|ThingRow" | head
# 期望:仅 core 类型残留引用(Task 2 已改名,应为 0)或 wire 模块自有类型(Task 5 处理)——报告分类列出
```

- [ ] **Step 4: Commit**

```bash
git add -A crates/db && git commit -m "refactor(db)!: rename Device types/functions/modules to Thing"
```

---

### Task 4: apps/cloud 改名 + admin 路由整理

**Files:**
- Rename: `apps/cloud/src/domains/admin/device/`→`domains/admin/thing/`、`domains/mcp/tools/device.rs`→`domains/mcp/tools/thing.rs`、`domains/thing/legacy/device_query.rs`→`domains/thing/legacy/thing_query.rs`
- Modify: `apps/cloud/src/api/mod.rs`（路由挂载）、admin thing 路由组全部文件、tombstone `management.rs`、调用点全量

**Interfaces:**
- Consumes: Task 2/3 的 core/db 新类型名。
- Produces: admin 监控路由终态 `/api/v1/things/admin/**`;410 tombstone 保持 `/api/v1/devices`;`mcp/tools/thing.rs` 模块（MCP schema 主键翻转在 Task 5)。

- [ ] **Step 1: 模块改名 + perl 替换**

```bash
git mv apps/cloud/src/domains/admin/device apps/cloud/src/domains/admin/thing
git mv apps/cloud/src/domains/mcp/tools/device.rs apps/cloud/src/domains/mcp/tools/thing.rs
git mv apps/cloud/src/domains/thing/legacy/device_query.rs apps/cloud/src/domains/thing/legacy/thing_query.rs
files=$(grep -rln "Device\|admin::device\|_device\b\|_devices\b" apps/cloud/src --include="*.rs" | grep -v "domains/driver/\|mqtt_client")
perl -pi -e '
  s/\bCreateDeviceCommandRequest\b/CreateThingCommandRequest/g;
  s/\bUpdateDeviceCommandRequest\b/UpdateThingCommandRequest/g;
  s/\bDeviceCommandQueryParams\b/ThingCommandQueryParams/g;
  s/\bCreateDeviceTemplateRequest\b/CreateThingTemplateRequest/g;
  s/\bUpdateDeviceTemplateRequest\b/UpdateThingTemplateRequest/g;
  s/\bCreateDeviceFromTemplateRequest\b/CreateThingFromTemplateRequest/g;
  s/\bCreateDeviceRequest\b/CreateThingRequest/g;
  s/\bUpdateDeviceRequest\b/UpdateThingRequest/g;
  s/\bDeviceStatusUpdate\b/ThingStatusUpdate/g;
  s/\bDeviceQueryParams\b/ThingQueryParams/g;
  s/\bDeviceCriteria\b/ThingCriteria/g;
  s/\bDeviceTemplate\b/ThingTemplate/g;
  s/\bDeviceCommand\b/ThingCommand/g;
  s/\bDeviceProperty\b/ThingProperty/g;
  s/\bDeviceStatus\b/ThingStatus/g;
  s/\bDeviceStats\b/ThingStats/g;
  s/\bDeviceInfo\b/ThingInfo/g;
  s/\bQuickDevice\b/QuickThing/g;
  s/\bDevice\b/Thing/g;
  s/\b(\w+)_devices\b/$1_things/g;
  s/\b(\w+)_device\b/$1_thing/g;
  s/\badmin::device\b/admin::thing/g;
' $files
```

- [ ] **Step 2: admin 路由迁移到 /things/admin**

读 `apps/cloud/src/api/mod.rs` 与 admin/thing 路由组，把 admin 监控/管理端点从 `/devices` 挂载点改到 `/things/admin`;410 tombstone 独立小路由保持 `/devices`。规则：
- 端点与 thing 主路由（`domains/thing`）重复的（如 `/{id}/profile`)→ 删 admin 副本，调用方指主路由；
- `/distribution`、`/quick` → 若 thing 主路由已有等价物，删 admin 副本；否则挂 `/things/admin/distribution`、`/things/admin/quick`;
- 其余（status/metrics/performance/traces/system)→ `/things/admin/{id}/...`、`/things/admin/overview`、`/things/admin/system/...`。
- 启动路由构造测试（agent_tasks_api_tests 曾抓双挂载 panic）必须过。

- [ ] **Step 3: 编译修正循环 + 测试 URL 更新**

```bash
cargo check -p tinyiothub-cloud 2>&1 | grep -E "^error" | head -30
```

修正规则：core/db 类型名对齐；`mod.rs` 模块声明；测试内 `/api/v1/devices/` URL→新路径（tombstone 测试保持 `/devices`);`domains/driver/**` 与 `mqtt_client.rs` 的 wire 字段**本任务不动**(Task 5)。tombstone 文案"/api/devices has been removed. Use /api/things instead."保持不变。

- [ ] **Step 4: 验证 + 下游盘点**

```bash
cargo check -p tinyiothub-cloud   # 0 error
cargo test -p tinyiothub-cloud 2>&1 | tail -5   # 全绿
cargo check --workspace 2>&1 | grep -cE "^error"   # agent/runtime/plugin-sdk/edge/drivers 残余,Task 5 工作面,报告列出
```

- [ ] **Step 5: Commit**

```bash
git add -A apps/cloud && git commit -m "refactor(cloud)!: rename Device types/modules to Thing; admin routes to /things/admin"
```

---

### Task 5: 驱动 wire + MCP schema + sinks 直接改名

**Files:**
- Modify: `crates/plugin-sdk/src/**`、`drivers/**`、`apps/edge/src/**`、`apps/cloud/src/domains/driver/**`、`apps/cloud/src/shared/mqtt_client.rs`、`crates/db/src/thing_command.rs`、`crates/db/src/thing_property.rs`、`crates/db/src/heartbeat.rs`、`crates/agent/src/runtime/heartbeat/**`、`apps/cloud/src/domains/agent/host/**`、`apps/cloud/src/domains/mcp/tools/thing.rs`、`apps/cloud/src/domains/driver/plugin/storage/**`
- Test: MCP 契约测试 6 个（PR-1 加的）更新

**Interfaces:**
- Consumes: Task 4 完成态（cloud 编译绿）。
- Produces: wire 契约终态——payload key `thingId`/`category`、heartbeat `"thingId"`、LLM 契约 `"thing_id"`、MCP 主键 `thingId`(alias `deviceId`);workspace 编译全绿。

- [ ] **Step 1: 盘点 wire 面（报告必须列全）**

```bash
grep -rn "device_id\|device_type\|deviceId\|deviceType\|\bDevice\b" crates/plugin-sdk/src drivers apps/edge/src --include="*.rs" | wc -l
grep -rn "deviceId\|device_id" apps/cloud/src/domains/driver apps/cloud/src/shared/mqtt_client.rs --include="*.rs" | wc -l
grep -rn "/devices/\|devices/" apps/cloud/src/shared/mqtt_client.rs apps/edge/src --include="*.rs" | head   # MQTT topic 段
grep -rn "device_id" crates/agent/src/runtime/heartbeat --include="*.rs" | head   # LLM 契约 prompt + parser
```

- [ ] **Step 2: 改名（按目录推进，每目录编译验证）**

1. `crates/plugin-sdk`:`Device`→`Thing`、wire 字段 `device_id`→`thing_id`、`device_type`→`category`(serde rename_all 下 JSON key 随字段变，即 wire 破坏——符合裁决）。
2. `drivers/`：对齐 plugin-sdk。
3. `apps/edge`:storage/mqtt payload 字段与 topic 段 `/devices/`→`/things/`（若 Step 1 grep 有命中）。
4. cloud driver 域 + mqtt_client：wire struct 字段改名（去掉 PR-1 的过渡兼容）；`gateway/types.rs` `DiscoveredDevice.device_type`→`category`、`PairingAck.device_id`→`thing_id`。
5. `thing_command.rs`/`thing_property.rs`(db wire 模块）：字段 `device_id`→`thing_id`，删除 PR-1 的 `#[sqlx(rename)]` 过渡（列名已一致）。
6. heartbeat 链：`crates/db/src/heartbeat.rs` JSON key `"deviceId"`→`"thingId"`;`workspace_heartbeat.rs` 读取侧同步；`pool_adapter.rs` 回退链保持 4 键（免费兼容，不改）。
7. LLM 契约：`crates/agent/src/runtime/heartbeat/loop_.rs` prompt 中 `"device_id"`→`"thing_id"`;`report.rs` parser 同步（`a["thing_id"]`，可保留 `or(device_id)` 一个版本的过渡注释——不，裁决直接改名：只读 thing_id)。
8. MCP:`mcp/tools/thing.rs`+`alarm_mcp.rs`+`job.rs` 的 input_schema JSON 主键→`thingId`/`targetThingId`;serde 属性翻转为 `rename = "thingId", alias = "deviceId"`;6 个契约测试更新（主键调用断言 + alias 兼容断言）。
9. plugin storage sinks:`postgres.rs` 默认列 `device_id`→`thing_id`、`influxdb.rs` tag 同步；CHANGELOG 注明。

- [ ] **Step 3: 全量验证**

```bash
cargo test --workspace 2>&1 | tail -10   # 全绿(本任务结束时 workspace 必须恢复全绿)
cargo clippy --workspace --exclude zeroclaw --all-targets -- -D warnings 2>&1 | tail -3
grep -rn "\bdevice_id\b\|\bdevice_type\b\|\"deviceId\"" crates/plugin-sdk/src drivers apps/edge/src apps/cloud/src/domains/driver apps/cloud/src/shared crates/agent/src --include="*.rs" | grep -v test | head
# 期望:零命中(pool_adapter 回退链字符串与测试除外——报告分类说明)
```

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "refactor(wire)!: rename driver/MQTT/MCP/LLM contract fields to thing (breaking, edge must co-upgrade)"
```

---

### Task 6: 前端适配（web/)

**Files:**
- Modify: `web/src/types/index.ts`(Device\* interface→Thing\*)、`web/src/api/**`、`web/src/ui/**`、`web/src/stores/device-cache.ts`
- Rename: `web/src/stores/device-cache.ts`→`thing-cache.ts`、a2ui catalog `device-card.ts`→`thing-card.ts`、`device-table.ts`→`thing-table.ts`（及 catalog 注册字符串）

**Interfaces:**
- Consumes: Task 4 的路由终态（`/api/v1/things/admin/**`、tombstone `/devices`)、Task 5 的 JSON key 终态（`thingId`/`category`/`targetThingId`)。
- Produces: 前端类型 `Thing/ThingProperty/ThingCommand/ThingAlarm/ThingListParams`;SPA 路由 `/things/:id`。

- [ ] **Step 1: 类型与字段 perl 替换**

```bash
cd web/src
git mv stores/device-cache.ts stores/thing-cache.ts
git mv ui/chat/a2ui/catalog/device-card.ts ui/chat/a2ui/catalog/thing-card.ts 2>/dev/null || true
git mv ui/chat/a2ui/catalog/device-table.ts ui/chat/a2ui/catalog/thing-table.ts 2>/dev/null || true
cd -
files=$(grep -rln "deviceId\|deviceType\|\bDevice\b\|/devices" web/src --include="*.ts" --include="*.tsx" 2>/dev/null)
perl -pi -e '
  s/\bDeviceListParams\b/ThingListParams/g;
  s/\bDeviceProperty\b/ThingProperty/g;
  s/\bDeviceCommand\b/ThingCommand/g;
  s/\bDeviceAlarm\b/ThingAlarm/g;
  s/\bDevice\b/Thing/g;
  s/\bdeviceId\b/thingId/g;
  s/\bdeviceType\b/category/g;
  s/\btargetType: .device./targetType: '"'"'thing'"'"'/g;
' $files
```

- [ ] **Step 2: 路径与组件名手工处理**

1. API 路径：`/devices/distribution`→`/things/admin/distribution`、`/devices/quick`→`/things/admin/quick`、`/monitoring/devices/${id}`→Task 4 终态（读 `api/mod.rs` 确认）、其余 `/devices`→`/things`;tombstone 提示如遇 410 属预期（旧客户端）。
2. SPA 路由：`/devices/${id}`→`/things/${id}`(workspace.ts/chat.ts);router 定义处的路径声明同步。
3. a2ui catalog：组件类型字符串 `"device-card"`→`"thing-card"`、`"device-table"`→`"thing-table"`（注册处 + 使用处 + LLM prompt 中的 catalog 描述若有）。
4. `device-cache` import 路径全量更新。

- [ ] **Step 3: 验证**

```bash
cd web && pnpm type-check && pnpm build 2>&1 | tail -5   # 全绿
grep -rn "deviceId\|deviceType\|/devices" src --include="*.ts" | grep -v "410\|removed" | head
# 期望:零命中(tombstone 文案除外)
```

- [ ] **Step 4: Commit**

```bash
git add -A web && git commit -m "refactor(web)!: adapt frontend to thing naming (types, fields, routes, a2ui catalog)"
```

---

### Task 7: 端到端验证与收尾

**Files:**
- Modify: `CHANGELOG.md`、`docs/superpowers/plans/2026-08-24-device-to-thing-db-rename.md`（追加 PR-2 完成记录，可选）

- [ ] **Step 1: 老库升级复测**（含新契约迁移）：复制 PR-1 的 baseline→migrate 路径实测（可用 `thing_contract_data_tests.rs` 已覆盖 + `data/` 老库手动启动一次验证应用启动不炸）。

- [ ] **Step 2: 全量门禁**

```bash
cargo test --workspace 2>&1 | tail -5
cargo clippy --workspace --exclude zeroclaw --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --all -- --check
cd web && pnpm type-check && pnpm build
```

- [ ] **Step 3: CHANGELOG**(`[Unreleased]` 追加，含所有 BREAKING):

```markdown
### Changed
- **BREAKING (types)**: core/db/cloud 的 Device* 类型与模块全面更名 Thing*(如 `core::models::thing::Thing`、`ThingCriteria`、`find_thing_by_id`);admin 监控路由迁至 `/api/v1/things/admin/**`。
- **BREAKING (wire)**: 驱动 MQTT/插件 payload `deviceId`→`thingId`、`deviceType`→`category`(edge 必须与 cloud 同步升级);LLM heartbeat 契约键 `device_id`→`thing_id`;plugin storage 外部 sink 默认列/tag `device_id`→`thing_id`;MCP input schema 主键 `thingId`/`targetThingId`(旧 `deviceId` 仍作 alias 接受)。
- **BREAKING (data)**: 迁移 `20260826000001` 将 `tag_bindings.target_type` 的 `'device'` 归并为 `'thing'`、permissions `device:*` 更名为 `thing:*`(id `perm-thing-*`);API 写入 `target_type='device'` 会被归一化为 `'thing'`。
- **BREAKING (frontend/a2ui)**: 前端类型 `Device*`→`Thing*`、SPA 路由 `/things/:id`;a2ui catalog 组件 `device-card`/`device-table`→`thing-card`/`thing-table`。
- 保留不变:`events.source_type='device'`、`actor='device'`、`jobs.job_type='device_command'` 枚举值(后续单独评估)。
```

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "docs: changelog for thing rename PR-2 (contracts, wire, frontend)"
```

---

## Self-Review 记录

- **Spec 覆盖**:① tag_bindings ✅(Task 1)、② permissions ✅(Task 1)、③ core 类型/模块 ✅(Task 2)、④ db 类型/函数/模块 ✅(Task 3)、⑤ cloud admin/mcp 模块 ✅(Task 4)、⑥ 驱动 wire ✅(Task 5)、⑦ MCP schema 主键 ✅(Task 5 Step 2.8)、⑧ 前端 ✅(Task 6)、⑨ admin 路由+tombstone+CHANGELOG ✅(Task 4/7)。
- **Placeholder 扫描**:Task 1 测试的 `mark_applied_and_migrate` 注明从既有测试文件复制实现；roles INSERT 列清单注明以 baseline DDL 为准（编译驱动）；其余步骤均有完整命令/代码。
- **类型一致性**:Global Constraints 的类型映射在 Task 2(core）与 Task 3(db）的 perl 规则逐字一致；`ThingCriteria`/`QuickThing` 等由 Task 3 Produces、Task 4 Consumes;MCP serde 翻转方向（rename=thingId, alias=deviceId）在约束与 Task 5 一致；路由终态 `/things/admin` 在 Task 4 定义、Task 6 消费。
- **风险备忘**:① Task 2-4 期间 workspace 部分 crate 编译断（预期，各任务门禁已调整）,Task 5 结束必须全绿；② wire 直接改名要求 edge 与 cloud 同版本部署，CHANGELOG 已注明；③ a2ui 组件名是 LLM 契约，改名后旧会话历史中的 component 引用失效（会话内瞬态，可接受）。
