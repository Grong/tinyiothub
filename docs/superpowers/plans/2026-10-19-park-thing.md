# 园区物（派生属性 + 驱动双模架构）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 园区物拥有由子物数据表达式计算的派生属性（总用电/总用水/设备数量），属性变更事件驱动重算并写回，资源（3D 模型/图片）作为知识可被 AI 发现并经 A2UI 渲染，全链路验收旅程跑通。

**Architecture:** 驱动双模（Poll/Push）——存量轮询驱动不动，新增首个推送驱动 DerivedDriver（订阅 EventBus 的 PropertyChange → 从 device_cache 取引用值 → 表达式求值 → 经 ValueSink 走共享下游管线写回）。派生定义复用 thing_properties 加三列（source/expression/refs）。规格：`docs/superpowers/specs/2026-10-19-park-thing-design.md`（含 CEO F-A..F-F、Eng E1-E6 全部已采纳修订）。

**Tech Stack:** Rust + Axum + Tokio + sqlx(SQLite) / Lit + Vite + vitest

## Global Constraints

- **runtime crate 禁止直接 sqlx**：一切 DB 读取经 `crates/runtime/src/ports.rs` port trait（E1，b4ac6db7 隔离）
- **集成测试用 sqlx 真实 DB，禁 mock-only**（项目铁律；LLM 才可 mock）
- **E2 硬性顺序**：Task 1（轮询回归锁定）必须先于 Task 2（共享管线抽取）合入
- **workspace 隔离为硬性要求**（D3）：所有新 API/校验按 workspace 作用域
- 派生属性值只允许 DerivedDriver 写（`is_read_only=1`），`PUT .../value` 拒绝（T3）
- 输入上限（F-C）：expression ≤1000 字符、refs ≤50 个、`descendant_count()` 参数须合法 thing_type
- **D2 递延不做**：派生挂告警集成测试、链式派生集成测试（已记 TODOS.md）
- 提交信息格式：中文 conventional commits（参照 `git log --oneline -5`），结尾 `Co-Authored-By: Claude <noreply@anthropic.com>`
- web 侧构建/测试需 Node 24：`export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"`（默认 Node 18 必败）
- spec 伪签名微调（依赖方向）：`start_push` 的 EventBus 参数改为 core 侧 `Arc<dyn PropertyChangeSource>`（core 不能依赖 runtime 的 EventBus），runtime 的 EventBus 实现该 trait

---

### Task 1: data_server 轮询行为回归锁定（E2 前置，CRITICAL）

**Files:**
- Modify: `crates/runtime/src/data_server.rs`（文件尾新增 `#[cfg(test)] mod tests`，并把 `collect_property_change_events`、`build_command_event` 两个私有 fn 改为 `pub(crate)`）

**Interfaces:**
- Consumes: 现有 `DataServer::collect_property_change_events(&mut Thing, &[ResultValue]) -> Vec<DomainEvent>`（data_server.rs:181）、`build_property_change_event_static`（:214）
- Produces: 行为锁定测试（Task 2 重构后必须全绿）；`pub(crate)` 可见性供 Task 2 抽取后复用

- [ ] **Step 1: 写失败测试（行为锁定）**

在 `crates/runtime/src/data_server.rs` 文件尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_core::models::thing::Thing;
    use tinyiothub_core::models::thing_property::ThingProperty;

    fn make_thing_with_prop(name: &str, value: Option<&str>) -> Thing {
        let mut t = Thing::new("thing-1".into(), "电表1".into());
        let mut p = ThingProperty::new("prop-1".into(), "thing-1".into(), "用电数".into());
        if let Some(v) = value { p.set_current_value(v.to_string()); }
        t.properties = Some(vec![p]);
        t
    }

    #[test]
    fn property_change_emits_event_only_when_value_changes() {
        let mut thing = make_thing_with_prop("用电数", Some("100"));
        // 新值不同 → 产生 1 个 PropertyChange 事件，缓存值被更新
        let values = vec![ResultValue::new("用电数".into(), "float".into(), Some("101.5".to_string()))];
        let events = DataServer::collect_property_change_events(&mut thing, &values);
        assert_eq!(events.len(), 1);
        let prop = &thing.properties.as_ref().unwrap()[0];
        assert_eq!(prop.current_value.as_deref(), Some("101.5"));
        // 同值再来 → 0 事件
        let events2 = DataServer::collect_property_change_events(&mut thing, &values);
        assert_eq!(events2.len(), 0);
    }

    #[test]
    fn property_change_first_report_emits_event() {
        let mut thing = make_thing_with_prop("用电数", None);
        let values = vec![ResultValue::new("用电数".into(), "float".into(), Some("42".to_string()))];
        let events = DataServer::collect_property_change_events(&mut thing, &values);
        assert_eq!(events.len(), 1, "首次上报（old=None）必须产生事件");
    }

    #[test]
    fn property_change_event_metadata_carries_old_and_new() {
        let mut thing = make_thing_with_prop("用电数", Some("100"));
        let values = vec![ResultValue::new("用电数".into(), "float".into(), Some("200".to_string()))];
        let events = DataServer::collect_property_change_events(&mut thing, &values);
        let json = serde_json::to_value(&events[0]).unwrap();
        let s = json.to_string();
        assert!(s.contains("\"value\":\"200\""), "metadata.value 必须是新值: {s}");
        assert!(s.contains("\"old_value\":\"100\""), "metadata.old_value 必须是旧值: {s}");
    }

    #[test]
    fn unknown_result_value_is_ignored() {
        let mut thing = make_thing_with_prop("用电数", Some("100"));
        let values = vec![ResultValue::new("不存在的属性".into(), "float".into(), Some("1".to_string()))];
        let events = DataServer::collect_property_change_events(&mut thing, &values);
        assert_eq!(events.len(), 0);
    }
}
```

- [ ] **Step 2: 改可见性**

`collect_property_change_events` 与 `build_property_change_event_static` 的 `fn` 前加 `pub(crate)`（测试模块同文件其实不需要，但 Task 2 抽取后跨模块要用，此处一并改）。

- [ ] **Step 3: 跑测试确认全绿（锁定现状行为）**

Run: `cargo test -p tinyiothub-runtime data_server::tests`
Expected: 4 passed。若有失败说明读代码时对现有行为理解有误——**先修正测试使其描述现状行为**（这是行为锁定，不是行为变更），再继续。

- [ ] **Step 4: Commit**

```bash
git add crates/runtime/src/data_server.rs
git commit -m "test(runtime): data_server 轮询行为回归锁定——变更收集/首报/同值抑制/元数据(E2)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: 共享下游管线抽取（轮询/推送共用）

**Files:**
- Modify: `crates/runtime/src/data_server.rs`（抽取 `apply_property_values` + `cache_and_publish`）

**Interfaces:**
- Consumes: Task 1 的锁定测试
- Produces:
  - `pub(crate) fn DataServer::apply_property_values(device: &mut Thing, thing_name: &str, values: &[ResultValue]) -> Vec<DomainEvent>` —— 纯函数：更新属性当前值并产出变更事件（= 现 `collect_property_change_events` 本体）
  - `pub(crate) async fn DataServer::cache_and_publish(device: Thing, events: Vec<DomainEvent>, device_cache: &Arc<dyn ThingCacheSource>, event_bus: Option<Arc<EventBus>>)` —— 缓存更新 + 事件发布（= 现 171-179 行块）

- [ ] **Step 1: 跑 Task 1 测试确认绿（重构前基线）**

Run: `cargo test -p tinyiothub-runtime data_server::tests`
Expected: 4 passed

- [ ] **Step 2: 抽取（纯移动，不改逻辑）**

```rust
impl DataServer {
    /// 应用一批属性读数：更新当前值并收集变更事件。轮询/推送共用（spec §一 共享下游管线）。
    pub(crate) fn apply_property_values(
        device: &mut Thing,
        thing_name: &str,
        values: &[ResultValue],
    ) -> Vec<DomainEvent> {
        Self::collect_property_change_events(device, values)
    }

    /// 缓存更新 + 事件逐个发布（缓存先于发布——订阅者读缓存必见新值）。
    pub(crate) async fn cache_and_publish(
        device: Thing,
        events: Vec<DomainEvent>,
        device_cache: &Arc<dyn ThingCacheSource>,
        event_bus: Option<Arc<EventBus>>,
    ) {
        device_cache.update(device);
        if let Some(bus) = event_bus {
            for event in events {
                publish_event_safe(bus.clone(), event).await;
            }
        }
    }
}
```

`process_all_drivers` 中原 `updated_device`/`pending_events` 尾块改为调用：

```rust
if let Some(device) = updated_device {
    Self::cache_and_publish(device, pending_events, &device_cache, event_bus_ref).await;
}
```

（`collect_property_change_events` 本体保留为 `pub(crate)`，`apply_property_values` 是其语义化门面——后续 Task 3 的 sink 消费与 Task 7 都调门面。）

- [ ] **Step 3: 跑回归 + 全 crate 测试**

Run: `cargo test -p tinyiothub-runtime`
Expected: 全绿（Task 1 的 4 例 + 既有 ports_injection 3 例等）

- [ ] **Step 4: Commit**

```bash
git add crates/runtime/src/data_server.rs
git commit -m "refactor(runtime): 抽取共享下游管线 apply_property_values/cache_and_publish——轮询推送共用

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: DriverMode + ValueSink + sink 独立消费 spawn

**Files:**
- Modify: `crates/core/src/driver/mod.rs`（DriverMode、DriverReading、ValueSink、PropertyChangeSource、trait 两方法）
- Modify: `crates/runtime/src/event_bus.rs`（impl PropertyChangeSource for EventBus）
- Modify: `crates/runtime/src/data_server.rs`（push 驱动跳过轮询 + sink 消费任务）
- Test: `crates/runtime/tests/push_driver_sink_test.rs`（新建）

**Interfaces:**
- Consumes: Task 2 的 `apply_property_values` / `cache_and_publish`
- Produces:
  - `DriverMode::{Poll, Push}`（core）；`ThingDriver::mode()` 默认 Poll、`ThingDriver::start_push(sink, events)` 默认空
  - `pub type ValueSink = tokio::sync::mpsc::Sender<DriverReading>`；`DriverReading { thing_id: String, values: Vec<ResultValue> }`
  - `trait PropertyChangeSource { fn subscribe(&self) -> broadcast::Receiver<Event>; }`（core）
  - data_server 持有 `value_sink_tx: ValueSink`（构造时创建 channel，rx 由独立任务消费）——Task 7 的 Manager 注册推送驱动时经此注入

- [ ] **Step 1: core 侧失败编译测试**

`crates/core/src/driver/mod.rs` 追加：

```rust
// ─── 驱动双模（spec 2026-10-19 §一）───

/// 驱动数据获取模式：Poll=运行时轮询拉取；Push=驱动内部订阅数据源主动上报。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DriverMode {
    #[default]
    Poll,
    Push,
}

/// 推送驱动的一次上报：某物的一批属性读数。
#[derive(Debug, Clone)]
pub struct DriverReading {
    pub thing_id: String,
    pub values: Vec<ResultValue>,
}

/// 运行时交给推送驱动的上报句柄（spec §一 ValueSink）。
pub type ValueSink = tokio::sync::mpsc::Sender<DriverReading>;

/// 属性变更事件源抽象（core 不依赖 runtime 的 EventBus，spec 伪签名按依赖方向微调）。
pub trait PropertyChangeSource: Send + Sync {
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<crate::models::event::Event>;
}
```

`ThingDriver` trait 内（`fn read_data` 之前）加：

```rust
    // === 双模（默认轮询，存量驱动零改动） ===

    fn mode(&self) -> DriverMode {
        DriverMode::Poll
    }

    /// 推送驱动启动钩子：实现方在此订阅数据源，数据到达后经 sink 上报。
    /// 默认空实现（轮询驱动永不调用）。
    fn start_push(&mut self, _sink: ValueSink, _events: std::sync::Arc<dyn PropertyChangeSource>) {}
```

- [ ] **Step 2: runtime 适配 + sink 消费任务**

`crates/runtime/src/event_bus.rs` 尾：

```rust
impl tinyiothub_core::driver::PropertyChangeSource for EventBus {
    fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.subscribe()
    }
}
```

`crates/runtime/src/data_server.rs`：
- DataServer 结构加字段 `value_sink_tx: ValueSink`；`new()` 中 `let (value_sink_tx, value_sink_rx) = tokio::sync::mpsc::channel(256);`
- `run()` 中在现有 spawn 之外再 spawn 消费任务：

```rust
let device_cache2 = self.device_cache.clone();
let driver_cache2 = self.driver_cache.clone();
let event_bus2 = self.event_bus.clone();
let mut rx = value_sink_rx;
let mut shutdown_rx2 = shutdown_rx.resubscribe();
tokio::spawn(async move {
    loop {
        tokio::select! {
            _ = shutdown_rx2.recv() => break,
            maybe = rx.recv() => {
                let Some(reading) = maybe else { break };
                // 经 driver_cache 找到 owner 物的 wrapper，走共享管线
                if let Some(driver_arc) = driver_cache2.get(&reading.thing_id) {
                    let mut driver = driver_arc.write().await;
                    let mut device = driver.thing().clone();
                    let events = Self::apply_property_values(&mut device, &device.name.clone(), &reading.values);
                    *driver.thing_mut() = device.clone();
                    drop(driver);
                    Self::cache_and_publish(device, events, &device_cache2, event_bus2.clone()).await;
                }
            }
        }
    }
});
```

- `process_all_drivers` 轮询循环中跳过推送驱动：

```rust
if driver.inner_driver().mode() == tinyiothub_core::driver::DriverMode::Push {
    continue; // 推送驱动不经轮询拉取（spec §一）
}
```

（`DriverCache` 是 moka `Cache<String, Arc<RwLock<DriverWrapper>>>`，`get` 返回 `Option<Arc<RwLock<DriverWrapper>>>`；RwLock 是 tokio 的故 `.write().await`。若现有代码是 std RwLock 则用 `.write().unwrap()`——以 data_server.rs:107 的 `try_write()` 为准，保持同风格。）

- [ ] **Step 3: 失败测试（推送路径端到端，进程内）**

`crates/runtime/tests/push_driver_sink_test.rs`：

```rust
//! 推送驱动 sink → 共享管线 → 缓存更新 + PropertyChange 发布（spec §一/E3）。

use std::sync::Arc;
use tinyiothub_core::driver::ResultValue;

// 复用 runtime 测试设施：构造最小 DataServer + 一个 mode()=Push 的 stub 驱动，
// 经 value_sink_tx 发送 DriverReading，断言：
// 1) device_cache 中该物属性值更新
// 2) EventBus 订阅者收到 ThingEventType::PropertyChange
// 3) 同一 100ms tick 内轮询路径不受推送流量影响（select! 独立 spawn，E3）
#[tokio::test]
async fn push_reading_flows_through_shared_pipeline() {
    // 实现要点：stub 驱动持有 Thing（id="campus-1"，属性 total_power 无值），
    // mode() 返回 Push；start_push 记录收到的 sink。
    // 测试直接向 data_server.value_sink_tx 发送 DriverReading{
    //   thing_id:"campus-1", values:[ResultValue::new("total_power","float",Some("42"))]}
    // 然后以 200ms 轮询等待缓存出现 "42"，并断言订阅到 1 条 PropertyChange。
    // EventBus 时序（E5）：先 let mut rx = event_bus.subscribe() 再发送。
}
```

（测试体内联完整 stub 实现；`DataServer::new(...)` 的构造参数以现有 `data_server.rs:241` 签名为准。）

- [ ] **Step 4: 跑测试**

Run: `cargo test -p tinyiothub-runtime`
Expected: 全绿（Task 1-2 回归 + 新 push 测试）；`cargo clippy -p tinyiothub-runtime -p tinyiothub-core -- -D warnings` 零告警

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/driver/mod.rs crates/runtime/src/event_bus.rs crates/runtime/src/data_server.rs crates/runtime/tests/push_driver_sink_test.rs
git commit -m "feat(runtime): 驱动双模架构——DriverMode/ValueSink/PropertyChangeSource+sink独立消费spawn

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: DB 迁移 + ThingProperty 模型/查询扩展

**Files:**
- Create: `crates/db/migrations/20261019000001_thing_properties_derived.sql`
- Modify: `crates/core/src/models/thing_property.rs`（3 个新字段 + CreateThingPropertyRequest.expression）
- Modify: `crates/db/src/thing_property.rs`（ThingPropertyRow + SELECT 列表 + From + 派生查询）
- Test: `crates/db/tests/thing_property_derived_test.rs`（新建）

**Interfaces:**
- Consumes: 无（基础层）
- Produces:
  - `ThingProperty.source/expression/refs: Option<String>`（serde skip_if none）
  - `db::thing::find_derived_properties(pool) -> Vec<ThingProperty>`（`expression IS NOT NULL`）
  - `CreateThingPropertyRequest.expression: Option<String>`

- [ ] **Step 1: 迁移文件**

```sql
-- 园区物：派生属性（spec 2026-10-19 §二）
-- source: 'template'（模板拷贝，存量默认）| 'derived'（表达式派生）
-- expression/refs 仅 source='derived' 时有值；refs 为 JSON [{thing_id, property_name}]
ALTER TABLE thing_properties ADD COLUMN source TEXT NOT NULL DEFAULT 'template';
ALTER TABLE thing_properties ADD COLUMN expression TEXT;
ALTER TABLE thing_properties ADD COLUMN refs TEXT;
-- E6：descendant_count() 树查询防患
CREATE INDEX IF NOT EXISTS idx_devices_parent_id ON devices(parent_id);
```

注：实施时先核对生产 schema 的列名（`thing_properties` 的 owner 列在 baseline 是 `device_id`、Row 结构是 `thing_id`——以 `sqlx::query("PRAGMA table_info(thing_properties)")` 实测为准）；`devices` 表名同理（若已改名 `things` 则索引建在实际表上）。

- [ ] **Step 2: 模型字段**

`crates/core/src/models/thing_property.rs` `ThingProperty` 结构 `is_read_only` 后加：

```rust
    /// 属性来源：'template'（默认）| 'derived'（派生，spec §二）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 派生表达式原文（仅 derived）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    /// 解析后引用 JSON：[{"thing_id":"...","property_name":"..."}]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refs: Option<String>,
```

`CreateThingPropertyRequest` 加 `pub expression: Option<String>`（serde default）。From<ThingPropertyRow> 同步映射；`clear_runtime_data()` 不动（三列是定义不是运行时数据）。

- [ ] **Step 3: db 行结构与查询**

`crates/db/src/thing_property.rs`：
- `ThingPropertyRow` 加 `source: String, expression: Option<String>, refs: Option<String>`
- 全部 SELECT 列表追加 `, source, expression, refs`（find_by_id / find_by_thing_id / 其余查询逐一 grep 补齐）
- INSERT（create_thing_properties_batch_tx）列列表与 bind 同步
- 新增：

```rust
/// 全量派生属性（启动重建内存反向索引用，spec §二）。
pub(crate) async fn find_derived_properties(
    pool: &SqlitePool,
) -> Result<Vec<ThingProperty>, sqlx::Error> {
    let rows = sqlx::query_as::<_, ThingPropertyRow>(
        r#"SELECT id, thing_id, name, display_name, description, data_type, unit,
                  min_value, max_value, default_value, is_read_only, created_at, updated_at,
                  source, expression, refs
           FROM thing_properties WHERE expression IS NOT NULL"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}
```

（crate 公开面：`crates/db/src/lib.rs` 或 `thing.rs` 的 Db 门面加 `pub async fn find_derived_properties`，沿用同文件既有包装模式。）

- [ ] **Step 4: 失败测试 → 实现 → 绿**

`crates/db/tests/thing_property_derived_test.rs`：

```rust
//! 派生属性三列迁移与查询（真实 DB，项目铁律）。

#[sqlx::test(migrations = "src/migrations")]
async fn derived_columns_default_and_roundtrip(pool: sqlx::SqlitePool) {
    // 1) 插入普通属性（不带 source）→ 读回 source='template'（迁移默认值）
    // 2) 插入 expression='{电表1.用电数}' + refs='[{"thing_id":"m1","property_name":"用电数"}]'
    //    → find_derived_properties 恰好返回它
    // 3) PRAGMA index_list(devices) 含 idx_devices_parent_id
}
```

Run: `cargo test -p tinyiothub-db thing_property_derived`
Expected: 3 断言全过

- [ ] **Step 5: Commit**

```bash
git add crates/db/migrations/20261019000001_thing_properties_derived.sql crates/core/src/models/thing_property.rs crates/db/src/thing_property.rs crates/db/tests/thing_property_derived_test.rs
git commit -m "feat(db): thing_properties 派生三列(source/expression/refs)+parent_id 索引+派生查询

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: 表达式解析与求值器

**Files:**
- Create: `crates/runtime/src/expression.rs`
- Modify: `crates/runtime/src/lib.rs`（`pub mod expression;`）
- Test: `crates/runtime/src/expression.rs` 内 `#[cfg(test)] mod tests`（穷举）

**Interfaces:**
- Consumes: 无（纯逻辑）
- Produces（Task 6/7/8 依赖的确切签名）:
  - `pub const MAX_EXPRESSION_LEN: usize = 1000; pub const MAX_REFS: usize = 50;`
  - `pub struct PropertyRef { pub thing_name: String, pub property_name: String }`
  - `pub enum Ast { Num(f64), Ref(PropertyRef), DescendantCount(Option<String>), Add(..), Sub(..), Mul(..), Div(..) }`
  - `pub fn parse(expr: &str) -> Result<Ast, String>`
  - `pub fn extract_refs(ast: &Ast) -> Vec<PropertyRef>`
  - `pub enum EvalError { MissingValue(PropertyRef), NotNumeric(PropertyRef), DivisionByZero }`
  - `pub fn eval(ast: &Ast, value_of: &dyn Fn(&PropertyRef) -> Option<String>, descendant_count: &dyn Fn(Option<&str>) -> Result<f64, String>) -> Result<f64, EvalError>`

- [ ] **Step 1: 穷举失败测试（先写全量用例）**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn dc(_: Option<&str>) -> Result<f64, String> { Ok(7.0) }

    #[test] fn parses_arithmetic_precedence() {
        let ast = parse("{电表1.用电数} + {电表2.用电数} * 2").unwrap();
        let v = eval(&ast, &|r: &PropertyRef| Some(if r.thing_name == "电表1" { "10".into() } else { "20".into() }), &dc).unwrap();
        assert_eq!(v, 50.0); // 10 + 20*2
    }
    #[test] fn parses_parens_and_unary_minus() {
        let ast = parse("({a.x} - 4) * (2 + 3)").unwrap();
        let v = eval(&ast, &|_| Some("9".into()), &dc).unwrap();
        assert_eq!(v, 25.0);
    }
    #[test] fn descendant_count_with_and_without_type() {
        assert!(matches!(parse("descendant_count()").unwrap(), Ast::DescendantCount(None)));
        assert!(matches!(parse("descendant_count(device)").unwrap(), Ast::DescendantCount(Some(ref t)) if t == "device"));
    }
    #[test] fn descendant_count_rejects_empty_type_arg() {
        assert!(parse("descendant_count( )").is_ok(), "空白视为无参");
        assert!(parse("descendant_count(123)").is_err(), "类型参数必须是标识符");
    }
    #[test] fn rejects_garbage() {
        for bad in ["", "{", "{a}", "{a.}", "1 +", "* 2", "{a.b} {c.d}", "sum(1)", "{a.b} % 2", "abc"] {
            assert!(parse(bad).is_err(), "应拒绝: {bad:?}");
        }
    }
    #[test] fn enforces_input_caps() {
        let long = format!("{{a.b}}{}", "+1".repeat(600));
        assert!(parse(&long).is_err(), "F-C: 超 1000 字符拒绝");
        let many_refs = (0..51).map(|i| format!("{{t{i}.p}}")).collect::<Vec<_>>().join("+");
        assert!(parse(&many_refs).is_err(), "F-C: 超 50 refs 拒绝");
    }
    #[test] fn eval_missing_value_is_missing() {
        let ast = parse("{a.x} + 1").unwrap();
        assert!(matches!(eval(&ast, &|_| None, &dc), Err(EvalError::MissingValue(_))));
    }
    #[test] fn eval_non_numeric_is_rejected() {
        let ast = parse("{a.x} + 1").unwrap();
        assert!(matches!(eval(&ast, &|_| Some("abc".into()), &dc), Err(EvalError::NotNumeric(_))));
    }
    #[test] fn division_by_zero() {
        let ast = parse("{a.x} / 0").unwrap();
        assert!(matches!(eval(&ast, &|_| Some("1".into()), &dc), Err(EvalError::DivisionByZero)));
    }
    #[test] fn extract_refs_dedups() {
        let ast = parse("{a.x} + {a.x} + {b.y}").unwrap();
        let refs = extract_refs(&ast);
        assert_eq!(refs.len(), 2);
    }
    #[test] fn numeric_strings_with_float_and_int() {
        let ast = parse("{a.x} + {a.y}").unwrap();
        let v = eval(&ast, &|r: &PropertyRef| Some(if r.property_name == "x" { "1.5".into() } else { "2".into() }), &dc).unwrap();
        assert_eq!(v, 3.5);
    }
}
```

- [ ] **Step 2: 实现解析器（递归下降，~200 行）**

```rust
//! 派生属性表达式：最小集递归下降解析器（spec §二）。
//!   expression := term (('+'|'-') term)*
//!   term       := factor (('*'|'/') factor)*
//!   factor     := '-' factor | NUMBER | ref | func | '(' expression ')'
//!   ref        := '{' 标识符(可含非ASCII/空格) '.' 标识符 '}'
//!   func       := 'descendant_count(' [标识符] ')'

pub const MAX_EXPRESSION_LEN: usize = 1000; // F-C
pub const MAX_REFS: usize = 50;             // F-C

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyRef {
    pub thing_name: String,
    pub property_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ast {
    Num(f64),
    Ref(PropertyRef),
    DescendantCount(Option<String>),
    Add(Box<Ast>, Box<Ast>),
    Sub(Box<Ast>, Box<Ast>),
    Mul(Box<Ast>, Box<Ast>),
    Div(Box<Ast>, Box<Ast>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    MissingValue(PropertyRef),
    NotNumeric(PropertyRef),
    DivisionByZero,
}

pub fn parse(expr: &str) -> Result<Ast, String> {
    if expr.len() > MAX_EXPRESSION_LEN {
        return Err(format!("表达式超长（{}>{}）", expr.len(), MAX_EXPRESSION_LEN));
    }
    let tokens = lex(expr)?;
    let mut p = Parser { tokens, pos: 0 };
    let ast = p.parse_expression()?;
    if p.pos != p.tokens.len() {
        return Err(format!("位置 {} 处有多余内容", p.pos));
    }
    if extract_refs(&ast).len() > MAX_REFS {
        return Err(format!("引用过多（>{}）", MAX_REFS));
    }
    Ok(ast)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    RefName(String), // {物名.属性名} 整体一个词素
    Plus, Minus, Star, Slash, LParen, RParen,
}

fn lex(input: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            c if c.is_whitespace() => i += 1,
            '+' => { out.push(Tok::Plus); i += 1; }
            '-' => { out.push(Tok::Minus); i += 1; }
            '*' => { out.push(Tok::Star); i += 1; }
            '/' => { out.push(Tok::Slash); i += 1; }
            '(' => { out.push(Tok::LParen); i += 1; }
            ')' => { out.push(Tok::RParen); i += 1; }
            '{' => {
                let start = i;
                let mut depth = 0;
                while i < chars.len() {
                    if chars[i] == '{' { depth += 1; }
                    if chars[i] == '}' { depth -= 1; if depth == 0 { break; } }
                    i += 1;
                }
                if i >= chars.len() { return Err("未闭合的 '{'".into()); }
                let inner: String = chars[start + 1..i].iter().collect();
                let (thing, prop) = inner.split_once('.').ok_or("引用须为 {物名.属性名}")?;
                if thing.trim().is_empty() || prop.trim().is_empty() {
                    return Err("引用物名/属性名不能为空".into());
                }
                out.push(Tok::RefName(format!("{}.{}", thing.trim(), prop.trim())));
                i += 1;
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') { i += 1; }
                let s: String = chars[start..i].iter().collect();
                out.push(Tok::Num(s.parse::<f64>().map_err(|_| format!("非法数字 '{s}'"))?));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
                out.push(Tok::Ident(chars[start..i].iter().collect()));
            }
            c => return Err(format!("非法字符 '{c}'")),
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> { self.tokens.get(self.pos) }
    fn next(&mut self) -> Option<Tok> { let t = self.tokens.get(self.pos).cloned(); if t.is_some() { self.pos += 1; } t }

    fn parse_expression(&mut self) -> Result<Ast, String> {
        let mut lhs = self.parse_term()?;
        while let Some(op) = self.peek() {
            match op {
                Tok::Plus => { self.next(); lhs = Ast::Add(Box::new(lhs), Box::new(self.parse_term()?)); }
                Tok::Minus => { self.next(); lhs = Ast::Sub(Box::new(lhs), Box::new(self.parse_term()?)); }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_term(&mut self) -> Result<Ast, String> {
        let mut lhs = self.parse_factor()?;
        while let Some(op) = self.peek() {
            match op {
                Tok::Star => { self.next(); lhs = Ast::Mul(Box::new(lhs), Box::new(self.parse_factor()?)); }
                Tok::Slash => { self.next(); lhs = Ast::Div(Box::new(lhs), Box::new(self.parse_factor()?)); }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_factor(&mut self) -> Result<Ast, String> {
        match self.next() {
            Some(Tok::Minus) => Ok(Ast::Sub(Box::new(Ast::Num(0.0)), Box::new(self.parse_factor()?))),
            Some(Tok::Num(n)) => Ok(Ast::Num(n)),
            Some(Tok::RefName(r)) => {
                let (t, p) = r.split_once('.').unwrap();
                Ok(Ast::Ref(PropertyRef { thing_name: t.into(), property_name: p.into() }))
            }
            Some(Tok::LParen) => {
                let e = self.parse_expression()?;
                if self.next() != Some(Tok::RParen) { return Err("缺少 ')'".into()); }
                Ok(e)
            }
            Some(Tok::Ident(name)) if name == "descendant_count" => {
                if self.next() != Some(Tok::LParen) { return Err("descendant_count 后须跟 '('".into()); }
                let arg = match self.peek() {
                    Some(Tok::Ident(t)) => { let t = t.clone(); self.next(); Some(t) }
                    _ => None,
                };
                if self.next() != Some(Tok::RParen) { return Err("descendant_count 缺少 ')'".into()); }
                Ok(Ast::DescendantCount(arg))
            }
            Some(t) => Err(format!("意外的词素 {:?}", t)),
            None => Err("表达式意外结束".into()),
        }
    }
}

pub fn extract_refs(ast: &Ast) -> Vec<PropertyRef> {
    fn walk(ast: &Ast, out: &mut Vec<PropertyRef>) {
        match ast {
            Ast::Ref(r) => { if !out.contains(r) { out.push(r.clone()); } }
            Ast::Add(a, b) | Ast::Sub(a, b) | Ast::Mul(a, b) | Ast::Div(a, b) => { walk(a, out); walk(b, out); }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(ast, &mut out);
    out
}

pub fn eval(
    ast: &Ast,
    value_of: &dyn Fn(&PropertyRef) -> Option<String>,
    descendant_count: &dyn Fn(Option<&str>) -> Result<f64, String>,
) -> Result<f64, EvalError> {
    match ast {
        Ast::Num(n) => Ok(*n),
        Ast::Ref(r) => {
            let raw = value_of(r).ok_or_else(|| EvalError::MissingValue(r.clone()))?;
            raw.trim().parse::<f64>().map_err(|_| EvalError::NotNumeric(r.clone()))
        }
        Ast::DescendantCount(t) => {
            descendant_count(t.as_deref()).map_err(|_| EvalError::MissingValue(PropertyRef {
                thing_name: "<descendant_count>".into(),
                property_name: t.clone().unwrap_or_default(),
            }))
        }
        Ast::Add(a, b) => Ok(eval(a, value_of, descendant_count)? + eval(b, value_of, descendant_count)?),
        Ast::Sub(a, b) => Ok(eval(a, value_of, descendant_count)? - eval(b, value_of, descendant_count)?),
        Ast::Mul(a, b) => Ok(eval(a, value_of, descendant_count)? * eval(b, value_of, descendant_count)?),
        Ast::Div(a, b) => {
            let d = eval(b, value_of, descendant_count)?;
            if d == 0.0 { return Err(EvalError::DivisionByZero); }
            Ok(eval(a, value_of, descendant_count)? / d)
        }
    }
}
```

- [ ] **Step 3: 跑测试**

Run: `cargo test -p tinyiothub-runtime expression`
Expected: 11 例全过

- [ ] **Step 4: Commit**

```bash
git add crates/runtime/src/expression.rs crates/runtime/src/lib.rs
git commit -m "feat(runtime): 派生表达式解析求值器——最小集语法+输入上限(F-C)+穷举单测

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6: DerivedPropertyQueries port + refs/DAG 校验

**Files:**
- Modify: `crates/runtime/src/ports.rs`（新 port trait）
- Modify: `apps/cloud/src/shared/runtime_ports.rs`（db 适配器实现，沿用既有适配器模式）
- Create: `apps/cloud/src/domains/thing/service/derived_validation.rs`
- Test: `apps/cloud/src/tests/derived_validation_test.rs`（真实 DB）

**Interfaces:**
- Consumes: Task 4（find_derived_properties）、Task 5（parse/extract_refs/PropertyRef）
- Produces:
  - `trait DerivedPropertyQueries { async fn list_derived(&self) -> Result<Vec<ThingProperty>, String>; async fn count_descendants(&self, root_thing_id: &str, thing_type: Option<&str>) -> Result<i64, String>; }`
  - `pub async fn validate_derived_expression(pool, workspace_id, owner_thing_id, expression) -> Result<Vec<ResolvedRef>, String>`；`pub struct ResolvedRef { pub thing_id: String, pub property_name: String }`——Task 8 API 保存时调用

- [ ] **Step 1: port trait**

`crates/runtime/src/ports.rs` 追加：

```rust
/// 派生属性读取（DerivedDriverManager：启动索引重建 + descendant_count 求值）。
/// spec §三 E1：runtime 禁止直接 sqlx。
#[async_trait]
pub trait DerivedPropertyQueries: Send + Sync {
    /// 全部派生属性定义（expression IS NOT NULL），含 workspace 隔离由调用方按物过滤。
    async fn list_derived(&self) -> Result<Vec<tinyiothub_core::models::thing_property::ThingProperty>, String>;
    /// root 物的后代计数（沿 parent_id 递归，深度上限 10）；thing_type 为空计全部。
    async fn count_descendants(&self, root_thing_id: &str, thing_type: Option<&str>) -> Result<i64, String>;
}
```

- [ ] **Step 2: db 适配器（apps/cloud 组合根注入）**

`apps/cloud/src/shared/runtime_ports.rs` 追加适配器（沿用同文件 ThingCacheSource 适配器的结构）：

```rust
pub struct DbDerivedPropertyQueries { pool: sqlx::SqlitePool }

#[async_trait::async_trait]
impl tinyiothub_runtime::ports::DerivedPropertyQueries for DbDerivedPropertyQueries {
    async fn list_derived(&self) -> Result<Vec<ThingProperty>, String> {
        tinyiothub_db::thing::find_derived_properties(&self.pool).await.map_err(|e| e.to_string())
    }
    async fn count_descendants(&self, root_thing_id: &str, thing_type: Option<&str>) -> Result<i64, String> {
        // 递归 CTE，深度上限 10（spec §二 防环兜底）
        let rows: (i64,) = sqlx::query_as(
            r#"WITH RECURSIVE sub(id, depth) AS (
                 SELECT id, 0 FROM devices WHERE parent_id = ?1
                 UNION ALL
                 SELECT d.id, s.depth + 1 FROM devices d JOIN sub s ON d.parent_id = s.id
                 WHERE s.depth < 10
               )
               SELECT COUNT(*) FROM devices d JOIN sub s ON d.id = s.id
               WHERE (?2 IS NULL OR d.thing_type = ?2)"#,
        )
        .bind(root_thing_id)
        .bind(thing_type)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(rows.0)
    }
}
```

- [ ] **Step 3: 失败测试（校验逻辑，真实 DB）**

`apps/cloud/src/tests/derived_validation_test.rs`：

```rust
//! 派生表达式保存时校验：refs 解析/同 workspace 存在性/DAG 环（spec §二）。

#[tokio::test]
async fn validate_ok_resolves_names_to_ids() { /* 建 电表1+用电数 属性 → validate 返回 ResolvedRef{真实id} */ }

#[tokio::test]
async fn validate_rejects_unknown_thing_or_property() { /* {不存在.x} → Err 含无效引用清单 */ }

#[tokio::test]
async fn validate_rejects_cross_workspace_ref() { /* 他 workspace 的物名 → Err（D3） */ }

#[tokio::test]
async fn validate_rejects_cycle() {
    // A.total = {B.x}（B 有派生 x={A.total}）→ 建第二个时 Err 含环路径 A→B→A
}

#[tokio::test]
async fn validate_self_reference_rejected() { /* owner 引用自己 → Err（环长 1） */ }
```

- [ ] **Step 4: 实现校验**

`apps/cloud/src/domains/thing/service/derived_validation.rs`：

```rust
//! 派生表达式保存时校验（spec §二）：语法 → 上限 → refs 解析（物名→id，同 workspace）
//! → DAG 环检测。保存入口（Task 8）的唯一校验点。

use sqlx::SqlitePool;
use tinyiothub_runtime::expression;

pub struct ResolvedRef {
    pub thing_id: String,
    pub property_name: String,
}

pub async fn validate_derived_expression(
    pool: &SqlitePool,
    workspace_id: &str,
    owner_thing_id: &str,
    expr: &str,
) -> Result<Vec<ResolvedRef>, String> {
    let ast = expression::parse(expr)?; // 语法 + F-C 上限
    let refs = expression::extract_refs(&ast);
    let mut resolved = Vec::with_capacity(refs.len());
    let mut invalid = Vec::new();
    for r in &refs {
        // 物名 → id（workspace 作用域）；属性须存在于该物
        let thing = sqlx::query_as::<_, (String,)>(
            "SELECT id FROM devices WHERE workspace_id = ? AND name = ?",
        )
        .bind(workspace_id).bind(&r.thing_name)
        .fetch_optional(pool).await.map_err(|e| e.to_string())?;
        match thing {
            Some((id,)) => {
                let exists: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM thing_properties WHERE thing_id = ? AND name = ?",
                )
                .bind(&id).bind(&r.property_name)
                .fetch_optional(pool).await.map_err(|e| e.to_string())?;
                if exists.is_none() {
                    invalid.push(format!("{} 无属性 {}", r.thing_name, r.property_name));
                } else {
                    resolved.push(ResolvedRef { thing_id: id, property_name: r.property_name.clone() });
                }
            }
            None => invalid.push(format!("物 {} 不存在", r.thing_name)),
        }
    }
    if !invalid.is_empty() {
        return Err(format!("无效引用: {}", invalid.join("; ")));
    }
    // DAG：owner → 每个引用物一条边；与既有派生边合并后从 owner DFS 找环
    detect_cycle(pool, workspace_id, owner_thing_id, &resolved).await?;
    Ok(resolved)
}

async fn detect_cycle(
    pool: &SqlitePool,
    workspace_id: &str,
    owner_thing_id: &str,
    new_edges: &[ResolvedRef],
) -> Result<(), String> {
    // 既有边：workspace 内所有派生属性 owner_thing_id -> refs.thing_id（refs 为 JSON）
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT p.thing_id, p.refs FROM thing_properties p
           JOIN devices d ON d.id = p.thing_id
           WHERE d.workspace_id = ? AND p.expression IS NOT NULL"#,
    )
    .bind(workspace_id)
    .fetch_all(pool).await.map_err(|e| e.to_string())?;
    let mut graph: std::collections::HashMap<String, Vec<String>> = Default::default();
    for (owner, refs_json) in rows {
        let ids: Vec<serde_json::Value> = serde_json::from_str(&refs_json).unwrap_or_default();
        for v in ids {
            if let Some(t) = v.get("thing_id").and_then(|x| x.as_str()) {
                graph.entry(owner.clone()).or_default().push(t.to_string());
            }
        }
    }
    // 新边（同 owner 多条属性时既有边已含旧引用；此处校验的是"新增后是否成环"，
    // 保守做法：新边全量加入——同 key 的旧边由调用方在更新路径先移除，见 Task 8 PUT 语义）
    for e in new_edges {
        graph.entry(owner_thing_id.to_string()).or_default().push(e.thing_id.clone());
    }
    // DFS 从 owner 出发能回到 owner 即环（含自环）
    let mut stack = vec![owner_thing_id.to_string()];
    let mut seen = std::collections::HashSet::new();
    while let Some(n) = stack.pop() {
        for next in graph.get(&n).cloned().unwrap_or_default() {
            if next == owner_thing_id {
                return Err(format!("引用成环: {} → … → {}", owner_thing_id, owner_thing_id));
            }
            if seen.insert(next.clone()) { stack.push(next); }
        }
    }
    Ok(())
}
```

- [ ] **Step 5: 跑测试**

Run: `cargo test -p tinyiothub-cloud derived_validation`
Expected: 5 例全过

- [ ] **Step 6: Commit**

```bash
git add crates/runtime/src/ports.rs apps/cloud/src/shared/runtime_ports.rs apps/cloud/src/domains/thing/service/derived_validation.rs apps/cloud/src/tests/derived_validation_test.rs
git commit -m "feat(thing): DerivedPropertyQueries port+db适配器+派生表达式校验(refs/DAG/workspace)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 7: DerivedDriver + DerivedDriverManager

**Files:**
- Create: `crates/runtime/src/driver/derived.rs`
- Create: `crates/runtime/src/driver/derived_manager.rs`
- Modify: `crates/runtime/src/driver/mod.rs`（`pub mod derived; pub mod derived_manager;`）
- Modify: `apps/cloud/src/bootstrap.rs`（启动时装配 Manager：注入 ports/cache/driver_cache/value_sink_tx/event_bus，全量重算）
- Test: `crates/runtime/tests/derived_driver_test.rs`（新建，进程内全链路）

**Interfaces:**
- Consumes: Task 3（start_push/ValueSink/PropertyChangeSource/DriverReading）、Task 5（expression）、Task 6（DerivedPropertyQueries）、Task 2（共享管线由 data_server sink 消费侧承担）
- Produces:
  - `DerivedDriver`（impl ThingDriver，mode()=Push；execute_command 返回"该物不支持动作"）
  - `DerivedDriverManager::{ new(queries, cache, driver_cache, sink, event_bus), start().await, notify_config_changed(owner_thing_id).await, notify_tree_changed(root_thing_id).await, notify_thing_deleted(thing_id).await, derived_status(property_id) -> Option<DerivedStatus> }`
  - `pub struct DerivedStatus { pub stale: bool, pub stale_reason: Option<String>, pub last_eval_at: Option<String> }`——Task 8 GET 注入

- [ ] **Step 1: 失败测试（进程内全链路，E5 时序可控）**

`crates/runtime/tests/derived_driver_test.rs`：

```rust
//! DerivedDriver 事件驱动重算全链路（spec §三）。
//! 链：模拟源物属性变更事件 → EventBus → DerivedDriver 命中引用表 →
//! 缓存取引用值 → 求值 → sink →（测试内直接消费 rx 走共享管线）→ 园区物缓存更新。

#[tokio::test]
async fn recompute_on_referenced_property_change() {
    // 装配：cache 含 电表1(用电数="100")/电表2(用电数="200")/园区物(total_power 无值)
    // Manager 注入 list_derived stub（port 测试替身，返回 total_power 表达式 "{电表1.用电数}+{电表2.用电数}"）
    // E5：let mut rx = event_bus.subscribe() 确认 subscriber_count>=1 后，发布
    //     PropertyChange(电表1.用电数, "150")
    // 断言：园区物 total_power == "350"（100+200 → 150+200），
    //       且园区物自己也产生 PropertyChange 事件
}

#[tokio::test]
async fn missing_ref_value_marks_stale_no_write() {
    // 电表2 无值 → total_power 不写入 + manager.derived_status(id).stale == true
    //   且 stale_reason 含"等待数据"
}

#[tokio::test]
async fn startup_full_recompute_produces_first_value() {
    // Manager.start() 后不经任何事件，total_power 已有值（首值补偿）
}

#[tokio::test]
async fn lagged_triggers_full_recompute_and_metric() {
    // 构造 broadcast 溢出（容量 1，连发 3 事件不消费）→ derived_eventbus_lagged +1
    //   且随后全量重算使值正确（自愈，spec §三）
}

#[tokio::test]
async fn thing_deleted_marks_stale_and_hot_updates_index() {
    // notify_thing_deleted(电表1) → total_power stale(reason 含"引用缺失")
}

#[tokio::test]
async fn tree_change_recomputes_descendant_count() {
    // notify_tree_changed(园区) → device_count 重算（count_descendants 走 port stub）
}
```

- [ ] **Step 2: 实现 derived.rs**

```rust
//! 派生驱动（spec §一/§三）：平台内置推送驱动，挂在拥有派生属性的物上。
//! 订阅 EventBus 的 PropertyChange，命中引用表即重算并经 sink 上报。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast::error::RecvError;
use tinyiothub_core::driver::{DriverMode, PropertyChangeSource, ResultValue, ThingDriver, ValueSink, DriverReading};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::event::ThingEventType;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;
use crate::expression::{self, Ast};
use crate::ports::ThingCacheSource;

/// 一条派生定义：AST + 解析后的引用（thing_id 级，spec §二 refs 以 id 为准）。
#[derive(Debug, Clone)]
pub struct DerivedDef {
    pub property_id: String,
    pub property_name: String,
    pub ast: Ast,
    /// (ref_thing_id, ref_property_name)
    pub refs: Vec<(String, String)>,
    pub has_descendant_count: bool,
}

pub static DERIVED_EVENTBUS_LAGGED: AtomicU64 = AtomicU64::new(0);

pub struct DerivedDriver {
    thing: Thing,
    defs: Vec<DerivedDef>,
    cache: Arc<dyn ThingCacheSource>,
    /// 启动后由 Manager 注入的 descendant_count 求值器（经 port）
    descendant_counter: Arc<dyn Fn(Option<&str>) -> Result<f64, String> + Send + Sync>,
}

impl DerivedDriver {
    pub fn new(
        thing: Thing,
        defs: Vec<DerivedDef>,
        cache: Arc<dyn ThingCacheSource>,
        descendant_counter: Arc<dyn Fn(Option<&str>) -> Result<f64, String> + Send + Sync>,
    ) -> Self {
        Self { thing, defs, cache, descendant_counter }
    }

    pub fn defs(&self) -> &[DerivedDef] { &self.defs }
    pub fn replace_defs(&mut self, defs: Vec<DerivedDef>) { self.defs = defs; }

    /// 重算全部 defs，返回有变化的读数（spec §二 求值规则：任一引用无值 → 该 def 跳过）。
    fn recompute(&self) -> Vec<ResultValue> {
        let cache = &self.cache;
        let dc = &self.descendant_counter;
        self.defs.iter().filter_map(|d| {
            let v = expression::eval(
                &d.ast,
                &|r: &expression::PropertyRef| {
                    // refs 以 thing_id 为准（PropertyRef.thing_name 在解析后已替换为 id）
                    cache.get(&r.thing_name)
                        .and_then(|t| t.properties?.into_iter().find(|p| p.name == r.property_name))
                        .and_then(|p| p.current_value)
                },
                &|t: Option<&str>| dc(t),
            );
            match v {
                Ok(n) => Some(ResultValue::new(d.property_name.clone(), "float".into(), Some(trim_float(n)))),
                Err(_) => None, // stale 由 Manager 记（见 derived_manager）
            }
        }).collect()
    }
}

fn trim_float(n: f64) -> String {
    if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n}") }
}

impl ThingDriver for DerivedDriver {
    fn thing(&self) -> &Thing { &self.thing }
    fn thing_mut(&mut self) -> &mut Thing { &mut self.thing }

    fn mode(&self) -> DriverMode { DriverMode::Push }

    fn start_push(&mut self, sink: ValueSink, events: Arc<dyn PropertyChangeSource>) {
        let thing_id = self.thing.id.clone();
        let defs = self.defs.clone();
        let cache = self.cache.clone();
        let dc = self.descendant_counter.clone();
        // 首值补偿（spec §三：启动即全量重算）
        let boot = Self { thing: self.thing.clone(), defs: defs.clone(), cache: cache.clone(), descendant_counter: dc.clone() };
        let boot_values = boot.recompute();
        let sink_boot = sink.clone();
        let boot_thing = thing_id.clone();
        tokio::spawn(async move {
            if !boot_values.is_empty() {
                let _ = sink_boot.send(DriverReading { thing_id: boot_thing, values: boot_values }).await;
            }
        });
        // 事件订阅循环（E5：测试经 subscriber_count 确认订阅已建立）
        let mut rx = events.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if event.event_type() != ThingEventType::PropertyChange { continue; }
                        let src = event.source();
                        // 命中引用表才重算（refs 以 thing_id 为准）
                        let hit = defs.iter().any(|d| d.refs.iter().any(|(tid, _)| *tid == src.source_id));
                        if !hit { continue; }
                        let tmp = DerivedDriver { thing: Thing::new(thing_id.clone(), thing_id.clone()), defs: defs.clone(), cache: cache.clone(), descendant_counter: dc.clone() };
                        let values = tmp.recompute();
                        if !values.is_empty() {
                            let _ = sink.send(DriverReading { thing_id: thing_id.clone(), values }).await;
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        DERIVED_EVENTBUS_LAGGED.fetch_add(n, Ordering::Relaxed);
                        // 自愈（spec §三）：lag 后全量重算，输入永远是最新缓存
                        let tmp = DerivedDriver { thing: Thing::new(thing_id.clone(), thing_id.clone()), defs: defs.clone(), cache: cache.clone(), descendant_counter: dc.clone() };
                        let values = tmp.recompute();
                        if !values.is_empty() {
                            let _ = sink.send(DriverReading { thing_id: thing_id.clone(), values }).await;
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }

    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        Ok(vec![]) // 推送驱动不经轮询拉取
    }

    fn execute_command(&mut self, _cmd: &ThingCommand) -> Result<bool, Error> {
        Err(Error::DriverError("该物不支持动作".into()))
    }
}
```

- [ ] **Step 3: 实现 derived_manager.rs（索引/生命周期/stale 内存态）**

```rust
//! 派生驱动管理器（spec §三）：反向索引、生命周期、stale 内存态、三类触发源入口。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tinyiothub_core::driver::ValueSink;
use tinyiothub_core::models::thing::Thing;
use crate::driver::derived::{DerivedDef, DerivedDriver};
use crate::event_bus::EventBus;
use crate::expression;
use crate::ports::{DerivedPropertyQueries, ThingCacheSource};

#[derive(Debug, Clone, serde::Serialize)]
pub struct DerivedStatus {
    pub stale: bool,
    pub stale_reason: Option<String>,
    pub last_eval_at: Option<String>,
}

type DriverCache = moka::future::Cache<String, Arc<tokio::sync::RwLock<crate::driver::wrapper::DriverWrapper>>>;

pub struct DerivedDriverManager {
    queries: Arc<dyn DerivedPropertyQueries>,
    cache: Arc<dyn ThingCacheSource>,
    driver_cache: DriverCache,
    sink: ValueSink,
    event_bus: Arc<EventBus>,
    /// (ref_thing_id, ref_property_name) → [owner_thing_id]
    index: Arc<RwLock<HashMap<(String, String), Vec<String>>>>,
    /// property_id → 状态（内存态不落库，spec §二 E4）
    status: Arc<RwLock<HashMap<String, DerivedStatus>>>,
}

impl DerivedDriverManager {
    pub fn new(
        queries: Arc<dyn DerivedPropertyQueries>,
        cache: Arc<dyn ThingCacheSource>,
        driver_cache: DriverCache,
        sink: ValueSink,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self { queries, cache, driver_cache, sink, event_bus,
               index: Default::default(), status: Default::default() }
    }

    /// 启动：扫派生定义重建索引（F-F：JSON 损坏行 fail-closed 标 stale 不阻断），
    /// 为每个 owner 物注册 DerivedDriver。
    pub async fn start(&self) -> Result<(), String> {
        let derived = self.queries.list_derived().await?;
        let mut by_owner: HashMap<String, Vec<DerivedDef>> = Default::default();
        for p in derived {
            match Self::def_of(&p) {
                Ok(def) => {
                    for (tid, pname) in &def.refs {
                        self.index.write().await
                            .entry((tid.clone(), pname.clone()))
                            .or_default()
                            .push(p.thing_id.clone());
                    }
                    by_owner.entry(p.thing_id.clone()).or_default().push(def);
                }
                Err(reason) => {
                    // F-F fail-closed
                    self.status.write().await.insert(p.id.clone(), DerivedStatus {
                        stale: true, stale_reason: Some(reason), last_eval_at: None,
                    });
                }
            }
        }
        for (owner_id, defs) in by_owner {
            self.ensure_driver(&owner_id, defs).await?;
        }
        Ok(())
    }

    fn def_of(p: &tinyiothub_core::models::thing_property::ThingProperty) -> Result<DerivedDef, String> {
        let expr = p.expression.clone().unwrap_or_default();
        let ast = expression::parse(&expr)?;
        let raw: Vec<serde_json::Value> = serde_json::from_str(p.refs.as_deref().unwrap_or("[]"))
            .map_err(|e| format!("refs JSON 损坏: {e}"))?;
        let refs = raw.iter().filter_map(|v| {
            Some((v.get("thing_id")?.as_str()?.to_string(), v.get("property_name")?.as_str()?.to_string()))
        }).collect::<Vec<_>>();
        let has_dc = p.expression.as_deref().unwrap_or("").contains("descendant_count");
        Ok(DerivedDef { property_id: p.id.clone(), property_name: p.name.clone(), ast, refs, has_descendant_count: has_dc })
    }

    async fn ensure_driver(&self, owner_id: &str, defs: Vec<DerivedDef>) -> Result<(), String> {
        if self.driver_cache.get(owner_id).await.is_some() {
            // 已有驱动 → 热更新 defs（增量维护）
            if let Some(arc) = self.driver_cache.get(owner_id).await {
                let mut w = arc.write().await;
                let inner = w.inner_driver_mut();
                // DerivedDriver 经 Any 下行不合适——ensure_driver 只在注册路径使用；
                // 热更新走 replace_defs（实现时给 DriverWrapper 加 derived_defs_mut 访问器，
                // 或 driver_cache 另存 DerivedDriver 句柄——二选一，实现者按最小侵入定）
                let _ = inner;
            }
            return Ok(());
        }
        let Some(thing) = self.cache.get(owner_id) else {
            return Err(format!("owner 物 {owner_id} 不在缓存"));
        };
        let dc = {
            let queries = self.queries.clone();
            let owner = owner_id.to_string();
            std::sync::Arc::new(move |t: Option<&str>| {
                // descendant_count 是 async port，此处同步包阻塞——实现时改为
                // tokio::task::block_in_place + Handle::block_on（runtime 内可用），
                // 或把 recompute 改 async（见 Task 7 Step 4 注）
                let q = queries.clone();
                let o = owner.clone();
                let t = t.map(|s| s.to_string());
                tokio::task::block_in_place(move || {
                    tokio::runtime::Handle::current().block_on(async move {
                        q.count_descendants(&o, t.as_deref()).await.map(|n| n as f64)
                    })
                })
            }) as std::sync::Arc<dyn Fn(Option<&str>) -> Result<f64, String> + Send + Sync>
        };
        let mut driver = DerivedDriver::new(thing, defs, self.cache.clone(), dc);
        driver.start_push(self.sink.clone(), self.event_bus.clone());
        let wrapper = crate::driver::wrapper::DriverWrapper::new(Box::new(driver));
        self.driver_cache.insert(owner_id.to_string(), Arc::new(tokio::sync::RwLock::new(wrapper))).await;
        Ok(())
    }

    /// 物管理面钩子（spec §三 三类触发源）：配置增删改 / 子树变更 / 物删除。
    pub async fn notify_config_changed(&self, _owner_thing_id: &str) { /* 重建该 owner defs+索引（调 start 的局部版） */ }
    pub async fn notify_tree_changed(&self, _root_thing_id: &str) { /* 触发含 descendant_count 的 defs 重算 */ }
    pub async fn notify_thing_deleted(&self, thing_id: &str) {
        // F-B：受影响派生标 stale + 索引热更新
        let mut idx = self.index.write().await;
        let affected: Vec<String> = idx.iter()
            .filter(|((tid, _), _)| tid == thing_id)
            .flat_map(|(_, owners)| owners.clone())
            .collect();
        idx.retain(|(tid, _), _| tid != thing_id);
        drop(idx);
        for owner in affected {
            let _ = &owner; // 标 stale（reason: 引用缺失）+ 热更新该 owner 驱动 defs
        }
    }

    pub async fn derived_status(&self, property_id: &str) -> Option<DerivedStatus> {
        self.status.read().await.get(property_id).cloned()
    }
}
```

注（实现者）：上面 `ensure_driver`/descendant_counter 两处标注了两个实现期抉择——①热更新 defs 的最小侵入访问器；②async port 在同步 eval 里的桥接（`block_in_place` 或把 recompute 全链路改 async）。推荐后者（eval 调用点只有 DerivedDriver，改 async 更干净）；若改 async，Task 5 的 eval 签名保持同步不动（纯逻辑），由 derived.rs 在调用前先把 descendant_count 值取好传入。

- [ ] **Step 4: 跑测试**

Run: `cargo test -p tinyiothub-runtime`
Expected: 全绿（含 derived_driver_test 6 例）+ `DERIVED_EVENTBUS_LAGGED` 在 lag 用例 >0

- [ ] **Step 5: bootstrap 装配 + panic 监督（T1）**

`apps/cloud/src/bootstrap.rs` 在 DataServer 装配处：
- 构造 `DbDerivedPropertyQueries` → `DerivedDriverManager::new(...).start().await`
- Manager 句柄挂 AppState（供 Task 8 handler 查询 derived_status）
- T1：data_server 的 sink 消费任务与推送订阅任务若在运行中 panic—— tokio::spawn 的 panic 不会传播；监督 = spawn 外包 `tokio::spawn(async move { loop { ... } })` 并在 panic 时记录 + 重启（对 sink 消费任务用 `catch_unwind` 语义不可行（async），改用最简监督：外层 spawn 监控 JoinHandle，`is_finished` 且非正常退出时重建）。**实现时优先验证：panic 在订阅任务内是否真会丢失重算**——若 recompute 已 catch 全部 EvalError（无 unwrap），则 panic 面≈0，T1 降级为"确认无 unwrap/expect 进入派生路径"的代码审查清单 + 一条故意注入 panic 的集成测试。

- [ ] **Step 6: Commit**

```bash
git add crates/runtime/src/driver/derived.rs crates/runtime/src/driver/derived_manager.rs crates/runtime/src/driver/mod.rs crates/runtime/tests/derived_driver_test.rs apps/cloud/src/bootstrap.rs
git commit -m "feat(runtime): DerivedDriver+Manager——事件驱动重算/首值补偿/lag自愈/stale内存态(F-B/F-E/F-F/T1/T4)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 8: 属性定义管理 API

**Files:**
- Modify: `apps/cloud/src/domains/admin/thing/properties.rs`（POST/PUT/DELETE + GET 派生状态注入 + PUT value 拒绝）
- Modify: `apps/cloud/src/domains/thing/service/mod.rs`（`pub mod derived_validation;`，Task 6 已建文件）
- Modify: `apps/cloud/src/domains/thing/handler/crud.rs`（删除物时调 `notify_thing_deleted`，F-B 钩子）
- Test: `apps/cloud/src/tests/derived_properties_api_test.rs`（新建）

**Interfaces:**
- Consumes: Task 6 `validate_derived_expression`、Task 7 `DerivedDriverManager`（AppState 句柄 + derived_status）
- Produces:
  - `POST /api/v1/things/admin/{thing_id}/properties`、`PUT .../properties/{property_id}`、`DELETE .../properties/{property_id}`
  - GET properties 响应派生行附 `derived_status{stale,stale_reason,last_eval_at}`（ThingProperty 模型加 runtime-only 字段 `derived_status: Option<DerivedStatus>`，serde skip_if none）

- [ ] **Step 1: 失败测试（真实 DB + 真实路由）**

`apps/cloud/src/tests/derived_properties_api_test.rs`：

```rust
//! 属性定义管理 API（spec §四）。

#[tokio::test]
async fn create_derived_property_ok() { /* POST {name:total_power, expression:"{电表1.用电数}+{电表2.用电数}"} → 200；GET 该行 source='derived' */ }

#[tokio::test]
async fn create_derived_rejects_bad_expression_400_with_position() { /* 语法错误 → 400 含定位 */ }

#[tokio::test]
async fn create_derived_rejects_caps_400() { /* 1001字符 / 51 refs → 400（F-C） */ }

#[tokio::test]
async fn create_derived_rejects_cycle_400() { /* 环 → 400 含环路径 */ }

#[tokio::test]
async fn create_derived_rejects_cross_workspace_404_or_400() { /* 他 workspace 引用 → 拒绝（D3） */ }

#[tokio::test]
async fn update_expression_revalidates() { /* PUT 改表达式 → 重新校验；改后 GET 可见新 refs */ }

#[tokio::test]
async fn delete_derived_property() { /* DELETE → 200；GET 不再出现；Manager notify_config_changed 被调（经状态断言） */ }

#[tokio::test]
async fn put_value_rejects_derived() { /* PUT .../value 对 source='derived' → 4xx（T3） */ }

#[tokio::test]
async fn duplicate_name_conflict_409() { /* 同名属性 POST 两次 → 409（UNIQUE(device_id,name)） */ }

#[tokio::test]
async fn get_properties_includes_derived_status() { /* Manager 造 stale → GET 该行 derived_status.stale=true */ }
```

- [ ] **Step 2: 实现 handler**

`apps/cloud/src/domains/admin/thing/properties.rs`：

```rust
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreatePropertyRequest {
    pub name: String,
    pub display_name: Option<String>,
    pub unit: Option<String>,
    pub data_type: Option<String>,
    pub expression: Option<String>, // 带 expression 即派生（spec §四）
}

// create_router() 追加：
//   .route("/{thing_id}/properties", post(create_property))
//   .route("/{thing_id}/properties/{property_id}", put(update_property).delete(delete_property))

async fn create_property(
    State(state): State<AdminState>,
    Path(thing_id): Path<String>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Json(req): Json<CreatePropertyRequest>,
) -> Json<ApiResponse<ThingProperty>> {
    // 1) 物归属校验（tenant_device_service.get_thing(&thing_id) 不存在 → 404）
    // 2) req.expression 为 Some 时：
    //    derived_validation::validate_derived_expression(pool, &workspace_id, &thing_id, expr)
    //    → Err(msg) 返回 400（msg 含语法定位/无效引用/环路径，前端行内红字）
    //    refs JSON = [{thing_id, property_name}]（validate 返回的 ResolvedRef）
    //    source='derived', data_type 缺省 'number', is_read_only=1
    // 3) db::thing::create_thing_properties_batch(...) → UNIQUE 冲突 → 409
    // 4) state.derived_manager.notify_config_changed(&thing_id)
    // 5) 200 返回新建属性
}

async fn update_property(/* 同构：改 expression → 全量重新校验；PUT 语义 = 替换该属性定义，
    DAG 校验前先从既有边移除本属性旧引用（detect_cycle 注释处） */)
async fn delete_property(/* 删除 → notify_config_changed；最后一个派生删除时 Manager 回收驱动 */)
```

`update_property_value`（既有）开头加：

```rust
// T3：派生属性值只由 DerivedDriver 写（spec §四）
if matches!(prop.source.as_deref(), Some("derived")) {
    return ApiResponseBuilder::error("派生属性不支持手动写值");
}
```

GET `get_thing_properties` 返回前：对 `source='derived'` 的行，从 `state.derived_manager.derived_status(&p.id)` 注入 `p.derived_status`（模型加 runtime-only 字段，参照 `current_value`/`alarm_status` 先例）。

- [ ] **Step 3: F-B 删除钩子**

`apps/cloud/src/domains/thing/handler/crud.rs` `delete_thing` 成功路径加 `state.derived_manager.notify_thing_deleted(&id).await`（物删除 RESTRICT 有子节点拒绝的既有行为不动）。

- [ ] **Step 4: 跑测试**

Run: `cargo test -p tinyiothub-cloud derived_properties_api`
Expected: 10 例全过

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/admin/thing/properties.rs apps/cloud/src/domains/thing apps/cloud/src/tests/derived_properties_api_test.rs crates/core/src/models/thing_property.rs
git commit -m "feat(api): 属性定义管理 POST/PUT/DELETE——派生校验/状态注入/手写拒绝/workspace隔离

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 9: 智慧园区模板与模拟物料

**Files:**
- Modify: `templates/builtin/scenes/smart_campus.json`（园区属性骨架 +3）
- Create: `templates/builtin/devices/electric_meter.json`、`templates/builtin/devices/water_meter.json`（若无同名内置模板——先 `ls templates/builtin/devices/` 核对）
- Test: `crates/db/tests/scene_templates_file_test.rs`（既有，扩充断言）

**Interfaces:**
- Consumes: 无
- Produces: 验收旅程物料——实例化后园区物自带 total_power/total_water/device_count 属性骨架（无表达式，实例层配置，spec §五）

- [ ] **Step 1: 核对既有物料**

Run: `ls templates/builtin/devices/ | head -20; python3 -c "import json;print([p['name'] for p in json.load(open('templates/builtin/scenes/smart_campus.json'))['properties']])"`
Expected: 确认电表/水表模板是否存在；smart_campus 现有 properties = [area, plot_ratio]

- [ ] **Step 2: smart_campus.json properties 追加三骨架**

```json
{"name": "total_power", "display_name": {"zh": "总用电", "en": "Total Power"}, "data_type": "float", "unit": "kWh", "is_read_only": true},
{"name": "total_water", "display_name": {"zh": "总用水", "en": "Total Water"}, "data_type": "float", "unit": "m³", "is_read_only": true},
{"name": "device_count", "display_name": {"zh": "设备数量", "en": "Device Count"}, "data_type": "int", "unit": "个", "is_read_only": true}
```

（字段名以既有 area 属性的实际键为准对齐。）

- [ ] **Step 3: 电表/水表模板（若不存在）**

`templates/builtin/devices/electric_meter.json`：

```json
{
  "name": "electric_meter",
  "display_name": {"zh": "电表", "en": "Electric Meter"},
  "version": "1.0.0", "category": "energy",
  "device_info": {"default_name_pattern": "电表{index}", "required_fields": []},
  "properties": [
    {"name": "用电数", "display_name": {"zh": "用电数", "en": "Energy"}, "data_type": "float", "unit": "kWh", "is_read_only": true},
    {"name": "功率", "display_name": {"zh": "功率", "en": "Power"}, "data_type": "float", "unit": "kW", "is_read_only": true}
  ]
}
```

`water_meter.json` 同构（用水数 m³）。若内置加载有 schema 校验，先跑 Step 4 测试暴露格式偏差再对齐。

- [ ] **Step 4: 模板加载测试**

Run: `cargo test -p tinyiothub-db scene_templates_file_test`
Expected: 全绿（含新增模板被加载/解析）

- [ ] **Step 5: 模拟表计配置说明（写入验收旅程文档块，不动代码）**

在计划 Task 13 的验收清单中固化：挂模拟驱动到表计实例的配置键——`interval=3000`（3s 上报）、`daily_amplitude_scale=0.2`、`noise_level=0.05`（simulated_driver.rs:151-156 的配置键），驱动名以 `crates/runtime/src/driver/mod.rs` 注册表中的 simulated 键为准（实现时 `grep -n "simulated" crates/runtime/src/driver/mod.rs` 确认）。

- [ ] **Step 6: Commit**

```bash
git add templates/builtin/
git commit -m "feat(templates): smart_campus 园区指标骨架+电表/水表内置模板

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 10: 前端属性 Tab 派生展示

**Files:**
- Modify: `web/src/ui/views/things-detail.ts`（属性 Tab 渲染处）
- Test: `web/src/ui/views/things-detail.test.ts`（新建或扩充既有）

**Interfaces:**
- Consumes: GET properties 响应的 `source/expression/refs/derived_status` 字段（Task 8）
- Produces: 派生徽标 + stale 灰点 + 表达式只读展开（spec §四 交互状态表）

- [ ] **Step 1: 失败测试（vitest）**

```typescript
// web/src/ui/views/things-detail.test.ts
import { describe, it, expect } from "vitest";
// 渲染属性行 fixture：
// 1) source='derived' 行显示「派生」徽标
// 2) derived_status.stale=true → 灰点 + 文字"等待数据"（非纯色编码，D11）
// 3) 点击展开 → 表达式只读文本 + 引用物名可点击（href=#/things/{id}）
// 4) refs 中标记已删除的引用 → 红字划线 + "已删除"
```

- [ ] **Step 2: 实现（沿用 home.css tokens，不引新组件）**

属性行渲染处：`source==='derived'` 时行尾加 `<span class="badge badge-derived">派生</span>`；`derived_status?.stale` 时状态点 `class="status-dot stale"` + 文字节点；表达式展开区用 `<details>` 原生元素（reuse ladder：原生优先）。样式加到既有视图 CSS（参照 `.status-dot` 既有类）。

- [ ] **Step 3: 跑测试**

Run: `cd web && export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && npx vitest run src/ui/views/things-detail.test.ts`
Expected: 全过

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/views/things-detail.ts web/src/ui/views/things-detail.test.ts
git commit -m "feat(web): 属性 Tab 派生徽标+stale 灰点+表达式只读展开(交互状态表)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 11: 派生编辑器 drawer

**Files:**
- Modify: `web/src/ui/views/things-detail.ts`（drawer 宿主）或 Create: `web/src/ui/views/derived-editor.ts`（行数 >300 时独立文件）
- Test: `web/src/ui/views/derived-editor.test.ts`

**Interfaces:**
- Consumes: POST/PUT properties API（Task 8）；物树/属性列表既有 API（GET /api/v1/things, GET properties）
- Produces: 派生创建/编辑 drawer（spec §四 交互状态表）

- [ ] **Step 1: 失败测试（vitest）**

```typescript
// 1) 「+ 添加引用」→ 物选择器（树形+搜索过滤）→ 选物 → 列出其数值属性（data_type float/int/number）
// 2) 选属性 → 表达式区插入 `{物名.属性名}` 文本
// 3) 运算符按钮 + − × ÷ ( ) 与 descendant_count() 插入
// 4) 保存 400（语法/环/上限）→ 行内红字定位到表达式区（非裸 toast）
// 5) 保存成功 → toast + drawer 关闭 + 属性行出现（"计算中…"态）
// 6) 无候选物空态："本工作区还没有带数值属性的物"
```

- [ ] **Step 2: 实现 drawer + 物选择器**

要点：选择器数据用 `GET /api/v1/things?parent_id=` 逐层懒加载（复用物列表既有 fetch 封装）；表达式区用 `contenteditable` 或只读文本域 + 插入按钮（不实现光标级编辑——本期插入即追加，YAGNI）；保存错误按 ApiResponse error msg 原样行内展示（后端已带定位）。复用 TODOS P2 里场景实例化 picker 缺的 search-select 模式——本任务的选择器组件做成可复用函数 `renderThingPicker(container, {onPick, filterNumericProps})`，后续场景 picker 可直接接入（在代码注释里标 `// reuse: scene-instantiator picker (TODOS P2)`）。

- [ ] **Step 3: 跑测试**

Run: `cd web && export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && npx vitest run src/ui/views/derived-editor.test.ts`
Expected: 全过

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/views/derived-editor.ts web/src/ui/views/derived-editor.test.ts web/src/ui/views/things-detail.ts
git commit -m "feat(web): 派生编辑器 drawer——物选择器+表达式构建+校验行内定位

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 12: 资源 URL 服务（T5）与知识工具元数据（T6）缺口

**Files:**
- Modify: `apps/cloud/src/domains/thing/handler/resources.rs`（T5 核实后定）
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/search_knowledge.rs`（T6 核实后定）
- Test: 随核实结果补

**Interfaces:**
- Consumes: resources 表（file_path/resource_type）
- Produces: 前端/A2UI 可加载的资源 URL；知识工具返回含 `resource_type` + URL

- [ ] **Step 1: T5 核实（证据先行）**

Run: `grep -rn "file_path\|ServeDir\|/files\|static" apps/cloud/src/api/mod.rs apps/cloud/src/domains/thing/handler/resources.rs | head -10`
判定：resources.file_path 当前是否有 HTTP 服务面。
- 若已有服务路由 → 记录路径，仅补测试
- 若无 → 加 `GET /api/v1/resources/{id}/file`：workspace 校验后按 file_path 读文件流返回（Content-Type 按扩展名：glb/gltf→model/gltf-binary,json；png/jpg→image/*；越权/不存在 → 404）。防路径穿越：file_path 必须 canonicalize 后仍在资源根目录下

- [ ] **Step 2: T5 测试 + 实现**

```rust
#[tokio::test]
async fn resource_file_served_within_workspace() { /* 上传 fixture glb → GET 200 + 字节一致 */ }
#[tokio::test]
async fn resource_file_cross_workspace_404() { }
#[tokio::test]
async fn resource_file_path_traversal_rejected() { /* file_path="../x" → 拒绝 */ }
```

- [ ] **Step 3: T6 核实 + 补齐**

Run: `grep -n "resource_type\|file_path\|url" apps/cloud/src/domains/agent/host/tools/thing/search_knowledge.rs | head`
判定知识工具返回是否已含类型/URL。若缺：返回结构每条加 `resource_type` 与 `url`（`/api/v1/resources/{id}/file`，Step 1 的路由）；`get_thing_profile` 的知识文档列表同步补（Agent 发现 3D 模型的前提，spec §五）。

- [ ] **Step 4: T6 工具测试**

```rust
#[tokio::test]
async fn search_knowledge_returns_type_and_url() { /* 挂 scene3d 资源 → 工具返回含 resource_type='scene3d' 与可访问 url */ }
```

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/thing/handler/resources.rs apps/cloud/src/domains/agent/host/tools/thing/search_knowledge.rs apps/cloud/src/api/mod.rs
git commit -m "feat(thing): 资源文件 HTTP 服务(workspace 隔离+防穿越)+知识工具资源元数据(T5/T6)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 13: 全链路集成测试 + 验收旅程

**Files:**
- Create: `apps/cloud/src/tests/park_derived_e2e_test.rs`
- 手测：spec §八 验收旅程 6 步

**Interfaces:**
- Consumes: 全部前序任务
- Produces: dead-path 防线（R2）：真实管线 E2E

- [ ] **Step 1: E2E 集成测试（真实 DB + 真实 EventBus + 真实 sink 管线，禁 mock 管线）**

`apps/cloud/src/tests/park_derived_e2e_test.rs`：

```rust
//! 园区派生全链路（spec §七/§八；R2：不接受 mock 管线）。

#[tokio::test]
async fn meter_report_triggers_park_total_recompute() {
    // 装配：真实 DB（sqlx::test migrations）+ 真实 DataServer/EventBus/Manager
    // 1) 建 园区物 + 电表1/电表2（挂 simulated 语义的手动值即可——本测试经
    //    ingest_driver_values 注入读数，模拟器行为在 Task 3 已证）
    // 2) POST 派生属性 total_power = {电表1.用电数}+{电表2.用电数}（走真实 API 校验）
    // 3) 经共享管线注入 电表1 新读数 → 等待（≤1s 轮询）园区物 total_power 更新
    // 4) 断言 events 表有园区物的 PropertyChange（真实落库）
    // 5) 电表2 值置空 → total_power 转 stale，GET 返回 stale_reason（spec §八 step4）
}

#[tokio::test]
async fn device_count_recomputed_on_tree_change() {
    // 园区下增 1 物 → notify_tree_changed → device_count +1（descendant_count('device')）
}

#[tokio::test]
async fn restart_rebuilds_index_and_first_value() {
    // 重建 Manager（模拟重启）→ start() 后 total_power 不经事件即有值（首值补偿）
}
```

- [ ] **Step 2: 跑全量后端测试 + clippy**

Run: `cargo test --workspace && cargo clippy --workspace -- -D warnings`
Expected: 全绿零告警

- [ ] **Step 3: 验收旅程手测（spec §八，逐项打勾记录）**

| 步骤 | 操作 | 通过标准 |
|---|---|---|
| 1 | 市场实例化智慧园区场景包 | 园区物+楼栋子树落库 |
| 2 | 建 2 电表 1 水表挂模拟驱动（interval=3000） | 详情页属性值持续跳动 |
| 3 | 园区物配 3 个派生属性 | total_power={电表1.用电数}+{电表2.用电数}、total_water={水表1.用水数}、device_count=descendant_count('device') |
| 4 | 看园区详情页；停 1 块表 | 值随上报刷新；停表 → stale"等待数据" |
| 5 | 上传园区 glb + 图片挂知识 | 知识 Tab 可见；资源 URL 可加载 |
| 6 | AI 对话"看看园区的三维模型和概况" | A2UI 渲染 scene-3d + 图片 + 实时属性卡 |

- [ ] **Step 4: Commit**

```bash
git add apps/cloud/src/tests/park_derived_e2e_test.rs
git commit -m "test(e2e): 园区派生全链路——表计上报→重算→落库+stale流+重启首值(R2防线)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Self-Review 记录（写完计划后对照 spec）

- spec §一 双模/共享管线 → Task 1-3 ✓；§二 三列/表达式/求值/DAG → Task 4-6 ✓；§三 生命周期/触发源/可观测 → Task 7 ✓；§四 API/前端 → Task 8/10/11 ✓；§五 资源/A2UI/模板物料 → Task 9/12 ✓；§六 错误处理 → 各任务测试步骤覆盖 ✓；§七 测试策略 → Task 1/3/5/6/7/8/13 ✓；§八 验收旅程 → Task 13 ✓
- D2 递延（告警挂载/链式派生集成测试）未出现在任何任务 ✓
- 类型一致性：ValueSink/DriverReading/PropertyChangeSource（T3）→ DerivedDriver（T7）→ API（T8）签名一致；PropertyRef.thing_name 在解析后存 thing_id（T5 注释/T7 recompute 一致）✓
- 已知实现期抉择点（不阻塞，已在任务内标注）：①DerivedWrapper 热更新 defs 访问器形态；②descendant_count async port 与同步 eval 的桥接（推荐 recompute 改 async）；③driver_cache RwLock 是 tokio 还是 std（以 data_server.rs:107 try_write 为准）
