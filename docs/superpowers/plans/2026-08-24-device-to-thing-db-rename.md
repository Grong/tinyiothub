# DB 层 device → thing 彻底重命名实施计划（PR-1）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新增一个迁移把 DB schema 的 device 命名彻底改为 thing（5 张表、13 张表的 device_id 列、device_type→category、tags 表 rebuild），并同步更新 crates/db、crates/core、apps/cloud 的 SQL 与 Rust 字段，PR 结束时 workspace 编译通过、测试全绿。

**Architecture:** SQLite `ALTER TABLE RENAME TO/COLUMN`(3.25+，自动跟随 FK/触发器/索引定义）；唯一需要重建的是 `tags`(CHECK 约束不可 ALTER)。代码层用 perl 全词替换做机械改名，编译器驱动修正残余。baseline.sql 不动（sqlx checksum)。

**Tech Stack:** Rust + sqlx(Sqlite)、axum；迁移器 `db::migrations::run_migrations`（自带备份 + FK OFF 执行）。

## Global Constraints

- **crate 名**:`db`(crates/db)、`core`(crates/core)、`tinyiothub-cloud`(apps/cloud)。测试命令:`cargo test -p db`、`cargo check -p tinyiothub-cloud`、`cargo test`(workspace)。
- **绝不修改** `crates/db/migrations/20260819000001_baseline.sql` 及已应用的迁移（checksum 校验）。
- **命名映射（verbatim)**:
  - 表：`devices`→`things`、`device_traces`→`thing_traces`、`device_memory`→`thing_memory`、`device_alarm_rules`→`thing_alarm_rules`、`device_alarms`→`thing_alarms`
  - 列：`device_id`→`thing_id`(14 张表，Task 1 实测修正：knowledge_relations 无此列，实为 messages、thing_traces、events、thing_memory、batch_command_items、resources、thing_properties、thing_actions、thing_alarm_rules、thing_alarms、knowledge_entities、agent_memories、agent_actions、subscription_plans 之外的第 14 张见迁移）、`jobs.target_device_id`→`target_thing_id`、`devices.device_type`→`category`、`messages.device_type`→`category`、`thing_templates.device_type`→**DROP**（该表已有 category 列）、`subscription_plans.device_limit`→`thing_limit`
  - `thing_type` 列**保持不变**（实体类型，与 category 语义不同）
  - `tags.type` CHECK: `('device','app','thing')` → `('app','thing')`，存量 `'device'` 数据更新为 `'thing'`
  - 触发器 `keep_device_memory_limit` → `keep_thing_memory_limit`
- **PR-1 范围外（不动）**:`apps/edge/`、`drivers/`、`crates/plugin-sdk/` 的驱动 wire 协议字段；`web/` 前端；HTTP 路由挂载点整理（PR-2);`core::models::device::Device` 类型名（PR-2)。
- **已知后续**:sed 会把 cloud/core 中 serde 字段 `deviceId`→`thingId`，前端在 PR-2 适配；驱动 MQTT/插件 payload 中的 `device_id`/`device_type` 保持原样（PR-3 对齐）。
- macOS 环境：用 `perl -pi -e`（支持 `\b`)，不用 BSD sed。
- 工作分支：从当前 `refactor/crates-reorg` 切出 `refactor/thing-db-rename`。

---

### Task 1: 重命名迁移 + schema 测试（TDD)

**Files:**
- Create: `crates/db/migrations/20260825000001_rename_device_to_thing.sql`
- Create: `crates/db/tests/thing_rename_schema_tests.rs`

**Interfaces:**
- Consumes: `db::migrations::run_migrations(&SqlitePool)`（已存在，`crates/db/src/migrations.rs`)
- Produces: 新 schema 对象名（`things`、`thing_id`、`category`、`keep_thing_memory_limit` 等），后续所有任务的代码改名以此为准。

- [ ] **Step 0: 切分支 + 前置核对（防漂移）**

```bash
git checkout -b refactor/thing-db-rename
grep -c "device_id" crates/db/migrations/20260819000001_baseline.sql   # 期望 40+
grep -n "CHECK (type IN" crates/db/migrations/20260819000001_baseline.sql  # 确认 tags 是唯一含 'device' CHECK 的表；若 tag_bindings 等也有，加入迁移 rebuild 清单
```

- [ ] **Step 1: 写失败的 schema 测试**

`crates/db/tests/thing_rename_schema_tests.rs`:

```rust
//! device→thing 重命名迁移的 schema 断言（Task 1）。
//! 全量迁移跑完后：新名存在、旧名消失、tags CHECK 不含 'device'。

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

async fn fresh_migrated_pool() -> (SqlitePool, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("thing-rename-schema-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePoolOptions::new().max_connections(1).connect(&url).await.unwrap();
    db::migrations::run_migrations(&pool).await.unwrap();
    (pool, path)
}

async fn table_names(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table'
           AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx%' ORDER BY name",
    )
    .fetch_all(pool).await.unwrap()
}

async fn columns(pool: &SqlitePool, table: &str) -> Vec<String> {
    sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .fetch_all(pool).await.unwrap()
}

#[tokio::test]
async fn device_tables_renamed_to_thing() {
    let (pool, path) = fresh_migrated_pool().await;
    let tables = table_names(&pool).await;
    for new in ["things", "thing_traces", "thing_memory", "thing_alarm_rules", "thing_alarms"] {
        assert!(tables.contains(&new.to_string()), "missing table {new}");
    }
    for old in ["devices", "device_traces", "device_memory", "device_alarm_rules", "device_alarms"] {
        assert!(!tables.contains(&old.to_string()), "stale table {old}");
    }
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn device_id_columns_renamed() {
    let (pool, path) = fresh_migrated_pool().await;
    for t in [
        "messages", "thing_traces", "events", "thing_memory", "batch_command_items",
        "resources", "thing_properties", "thing_actions", "thing_alarm_rules",
        "thing_alarms", "knowledge_entities", "knowledge_relations",
    ] {
        let cols = columns(&pool, t).await;
        assert!(cols.contains(&"thing_id".to_string()), "{t} missing thing_id");
        assert!(!cols.contains(&"device_id".to_string()), "{t} still has device_id");
    }
    let jobs = columns(&pool, "jobs").await;
    assert!(jobs.contains(&"target_thing_id".to_string()));
    assert!(!jobs.contains(&"target_device_id".to_string()));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn device_type_and_limit_renamed() {
    let (pool, path) = fresh_migrated_pool().await;
    for t in ["things", "thing_templates"] {
        let cols = columns(&pool, t).await;
        assert!(cols.contains(&"category".to_string()), "{t} missing category");
        assert!(!cols.contains(&"device_type".to_string()), "{t} still has device_type");
        assert!(cols.contains(&"thing_type".to_string()) || t == "thing_templates",
                "things.thing_type must survive");
    }
    let plans = columns(&pool, "subscription_plans").await;
    assert!(plans.contains(&"thing_limit".to_string()));
    assert!(!plans.contains(&"device_limit".to_string()));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn tags_check_and_trigger_renamed() {
    let (pool, path) = fresh_migrated_pool().await;
    let tags_sql: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='tags'",
    )
    .fetch_one(&pool).await.unwrap();
    assert!(!tags_sql.contains("'device'"), "tags CHECK still allows 'device': {tags_sql}");
    let trigger: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='trigger' AND name='keep_thing_memory_limit'",
    )
    .fetch_optional(&pool).await.unwrap();
    assert!(trigger.is_some(), "keep_thing_memory_limit missing");
    let old_trigger: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='trigger' AND name='keep_device_memory_limit'",
    )
    .fetch_optional(&pool).await.unwrap();
    assert!(old_trigger.is_none());
    let _ = std::fs::remove_file(path);
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p db --test thing_rename_schema_tests`
Expected: FAIL(`missing table things` 等断言失败）

- [ ] **Step 3: 写迁移 SQL**

`crates/db/migrations/20260825000001_rename_device_to_thing.sql` 完整内容：

```sql
-- device → thing 彻底重命名（PR-1）。
-- SQLite ≥3.25:RENAME TO/COLUMN 自动更新 FK 引用、触发器体、索引定义、UNIQUE 约束。
-- 运行环境:run_migrations 已 PRAGMA foreign_keys = OFF。

-- ── 1. 表重命名 ──
ALTER TABLE devices RENAME TO things;
ALTER TABLE device_traces RENAME TO thing_traces;
ALTER TABLE device_memory RENAME TO thing_memory;
ALTER TABLE device_alarm_rules RENAME TO thing_alarm_rules;
ALTER TABLE device_alarms RENAME TO thing_alarms;

-- ── 2. device_id → thing_id(12 张表;jobs 单列见下)──
ALTER TABLE messages RENAME COLUMN device_id TO thing_id;
ALTER TABLE thing_traces RENAME COLUMN device_id TO thing_id;
ALTER TABLE events RENAME COLUMN device_id TO thing_id;
ALTER TABLE thing_memory RENAME COLUMN device_id TO thing_id;
ALTER TABLE batch_command_items RENAME COLUMN device_id TO thing_id;
ALTER TABLE resources RENAME COLUMN device_id TO thing_id;
ALTER TABLE thing_properties RENAME COLUMN device_id TO thing_id;
ALTER TABLE thing_actions RENAME COLUMN device_id TO thing_id;
ALTER TABLE thing_alarm_rules RENAME COLUMN device_id TO thing_id;
ALTER TABLE thing_alarms RENAME COLUMN device_id TO thing_id;
ALTER TABLE knowledge_entities RENAME COLUMN device_id TO thing_id;
ALTER TABLE knowledge_relations RENAME COLUMN device_id TO thing_id;
ALTER TABLE jobs RENAME COLUMN target_device_id TO target_thing_id;

-- ── 3. device_type → category / device_limit → thing_limit ──
ALTER TABLE things RENAME COLUMN device_type TO category;
ALTER TABLE thing_templates RENAME COLUMN device_type TO category;
ALTER TABLE subscription_plans RENAME COLUMN device_limit TO thing_limit;

-- ── 4. tags 重建(CHECK 不可 ALTER);存量 'device' 归入 'thing' ──
UPDATE tags SET type = 'thing' WHERE type = 'device';
CREATE TABLE tags_new (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL CHECK (type IN ('app', 'thing')),
    name TEXT NOT NULL,
    description TEXT,
    color TEXT,
    tenant_id TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);
INSERT INTO tags_new
    SELECT id, type, name, description, color, tenant_id, created_by, created_at, updated_at
    FROM tags;
DROP TABLE tags;
ALTER TABLE tags_new RENAME TO tags;
CREATE UNIQUE INDEX idx_tags_type_name_tenant ON tags(COALESCE(tenant_id, ''), type, name);
CREATE INDEX idx_tags_type ON tags(type);
CREATE INDEX idx_tags_name ON tags(name);
CREATE INDEX idx_tags_tenant_id ON tags(tenant_id);

-- ── 5. 触发器重命名(定义已被 RENAME 自动更新,仅换名)──
DROP TRIGGER keep_device_memory_limit;
CREATE TRIGGER keep_thing_memory_limit
AFTER INSERT ON thing_memory
BEGIN
    DELETE FROM thing_memory
    WHERE workspace_id = NEW.workspace_id
      AND agent_id = NEW.agent_id
      AND thing_id = NEW.thing_id
      AND id NOT IN (
          SELECT id FROM thing_memory
          WHERE workspace_id = NEW.workspace_id
            AND agent_id = NEW.agent_id
            AND thing_id = NEW.thing_id
          ORDER BY snapshot_time DESC
          LIMIT 100
      );
END;

-- ── 6. 索引换名(定义已随 RENAME 自动更新,此处 DROP+CREATE 仅为命名一致)──
DROP INDEX idx_devices_name_workspace;
CREATE UNIQUE INDEX idx_things_name_workspace ON things(COALESCE(workspace_id, ''), name);
DROP INDEX idx_devices_device_type;
CREATE INDEX idx_things_category ON things(category);
DROP INDEX idx_devices_state;
CREATE INDEX idx_things_state ON things(state);
DROP INDEX idx_devices_parent_id;
CREATE INDEX idx_things_parent_id ON things(parent_id);
DROP INDEX idx_devices_organization_id;
CREATE INDEX idx_things_organization_id ON things(organization_id);
DROP INDEX idx_devices_tenant_id;
CREATE INDEX idx_things_tenant_id ON things(tenant_id);
DROP INDEX idx_devices_workspace;
CREATE INDEX idx_things_workspace ON things(workspace_id);
DROP INDEX idx_devices_driver_name;
CREATE INDEX idx_things_driver_name ON things(driver_name);
DROP INDEX idx_devices_factory_name;
CREATE INDEX idx_things_factory_name ON things(factory_name);
DROP INDEX idx_devices_linked_gateway;
CREATE INDEX idx_things_linked_gateway ON things(linked_gateway);
DROP INDEX idx_devices_fingerprint;
CREATE INDEX idx_things_fingerprint ON things(fingerprint);
DROP INDEX idx_devices_thing_type;
CREATE INDEX idx_things_thing_type ON things(thing_type);
DROP INDEX idx_devices_template_id;
CREATE INDEX idx_things_template_id ON things(template_id);
DROP INDEX idx_device_traces_device_id;
CREATE INDEX idx_thing_traces_thing_id ON thing_traces(thing_id);
DROP INDEX idx_device_traces_device_time;
CREATE INDEX idx_thing_traces_thing_time ON thing_traces(thing_id, created_at DESC);
DROP INDEX idx_device_traces_device_type;
CREATE INDEX idx_thing_traces_thing_type ON thing_traces(thing_id, trace_type);
DROP INDEX idx_device_traces_device_level;
CREATE INDEX idx_thing_traces_thing_level ON thing_traces(thing_id, level);
DROP INDEX idx_device_memory_lookup;
CREATE INDEX idx_thing_memory_lookup ON thing_memory(workspace_id, agent_id, thing_id, snapshot_time DESC);
DROP INDEX idx_device_alarm_rules_device_id;
CREATE INDEX idx_thing_alarm_rules_thing_id ON thing_alarm_rules(thing_id);
DROP INDEX idx_device_alarms_device_id;
CREATE INDEX idx_thing_alarms_thing_id ON thing_alarms(thing_id);
DROP INDEX idx_messages_device_id;
CREATE INDEX idx_messages_thing_id ON messages(thing_id);
DROP INDEX idx_events_device;
CREATE INDEX idx_events_thing ON events(thing_id);
DROP INDEX idx_events_device_timestamp;
CREATE INDEX idx_events_thing_timestamp ON events(thing_id, timestamp) WHERE thing_id IS NOT NULL;
DROP INDEX idx_events_timestamp_level_device;
CREATE INDEX idx_events_timestamp_level_thing ON events(timestamp, event_level, thing_id) WHERE thing_id IS NOT NULL;
DROP INDEX idx_events_device_id;
CREATE INDEX idx_events_thing_id ON events(thing_id);
DROP INDEX idx_jobs_target_device_id;
CREATE INDEX idx_jobs_target_thing_id ON jobs(target_thing_id);
DROP INDEX idx_batch_command_items_device_id;
CREATE INDEX idx_batch_command_items_thing_id ON batch_command_items(thing_id);
DROP INDEX idx_resources_device_id;
CREATE INDEX idx_resources_thing_id ON resources(thing_id);
DROP INDEX idx_thing_properties_device_id;
CREATE INDEX idx_thing_properties_thing_id ON thing_properties(thing_id);
DROP INDEX idx_thing_actions_device_id;
CREATE INDEX idx_thing_actions_thing_id ON thing_actions(thing_id);
DROP INDEX idx_thing_templates_device_type;
CREATE INDEX idx_thing_templates_category ON thing_templates(category);
DROP INDEX idx_knowledge_entities_device;
CREATE INDEX idx_knowledge_entities_thing ON knowledge_entities(thing_id);
```

注意：`idx_events_status_dedup`(partial unique，名字不含 device，定义已被 RENAME COLUMN 自动更新）保持不变。执行前先用 Step 0 的 grep 对照 baseline，若索引清单有出入以 baseline 实际为准增删 DROP/CREATE 对。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p db --test thing_rename_schema_tests`
Expected: PASS(4 个测试全绿）

- [ ] **Step 5: Commit**

```bash
git add crates/db/migrations/20260825000001_rename_device_to_thing.sql crates/db/tests/thing_rename_schema_tests.rs
git commit -m "feat(db): migration renaming device schema to thing (tables, columns, tags CHECK, trigger, indexes)"
```

---

### Task 2: 数据保留回归测试（baseline → 重命名迁移）

**Files:**
- Create: `crates/db/tests/thing_rename_data_tests.rs`

**Interfaces:**
- Consumes: Task 1 的迁移文件；`db::migrations::run_migrations`
- Produces: 老库升级不丢数据的回归保障，后续任务不得破坏。

- [ ] **Step 1: 写数据保留测试**

`crates/db/tests/thing_rename_data_tests.rs`:

```rust
//! 老库(baseline 终态)跑重命名迁移后:数据保留、tags 'device'→'thing'、FK 无违规。

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

#[tokio::test]
async fn baseline_data_survives_rename() {
    let path = std::env::temp_dir().join(format!("thing-rename-data-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePoolOptions::new().max_connections(1).connect(&url).await.unwrap();

    // 1. 只建 baseline 终态(不经 run_migrations,直接执行 baseline.sql)
    sqlx::raw_sql(include_str!("../migrations/20260819000001_baseline.sql"))
        .execute(&pool).await.unwrap();

    // 2. 插入覆盖各改名面的样本数据
    sqlx::query("INSERT INTO users (id, username, password_hash) VALUES ('u1', 'alice', 'x')")
        .execute(&pool).await.unwrap_err(); // users 列名不确定时忽略,样本不依赖 users
    sqlx::query("INSERT INTO devices (id, name, thing_type, device_type) VALUES ('d1', 'demo', 'device', 'sensor')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO thing_properties (id, device_id, name) VALUES ('p1', 'd1', 'temp')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO tags (id, type, name) VALUES ('t1', 'device', 'tag1')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO device_memory (workspace_id, agent_id, device_id, snapshot_data, snapshot_time)
                 VALUES ('w1', 'a1', 'd1', '{}', 1)")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO events (id, event_level, event_type, event_subtype, timestamp, source_type, title, device_id)
                 VALUES ('e1', 'info', 't', 'st', '2026-08-25T00:00:00Z', 'device', 'x', 'd1')")
        .execute(&pool).await.unwrap();

    // 3. 跑剩余迁移(含重命名)
    db::migrations::run_migrations(&pool).await.unwrap();

    // 4. 断言数据保留 + 列名已改 + tags 值已转换
    let things_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE id = 'd1' AND category = 'sensor'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(things_count, 1);
    let prop: String = sqlx::query_scalar("SELECT thing_id FROM thing_properties WHERE id = 'p1'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(prop, "d1");
    let tag_type: String = sqlx::query_scalar("SELECT type FROM tags WHERE id = 't1'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(tag_type, "thing");
    let mem: String = sqlx::query_scalar("SELECT thing_id FROM thing_memory WHERE workspace_id = 'w1'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(mem, "d1");
    let evt: String = sqlx::query_scalar("SELECT thing_id FROM events WHERE id = 'e1'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(evt, "d1");

    // 5. FK 完整性
    let violations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(violations, 0, "foreign_key_check violations after rename");

    let _ = std::fs::remove_file(path);
}
```

注：第 1 步里 users 插入是占位探测——若列名不匹配该语句报错被 `unwrap_err()` 吞掉即跳过；其余样本不依赖 users。若执行时发现 events 有 NOT NULL 列未覆盖，按报错补列。

- [ ] **Step 2: 运行测试确认通过**

Run: `cargo test -p db --test thing_rename_data_tests`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/db/tests/thing_rename_data_tests.rs
git commit -m "test(db): data-preservation regression for device→thing rename migration"
```

---

### Task 3: seed 文件与 seed 加载器更新

**Files:**
- Modify: `crates/db/src/seed/demo.sql`
- Modify: `crates/db/src/seed/system.sql`
- Modify: `crates/db/src/seed.rs`（约 80-86 行：COUNT 查询引用 `devices`)
- Test: `crates/db/tests/seed_tests.rs`

**Interfaces:**
- Consumes: Task 1 的新 schema
- Produces: `seed_system`/`seed_demo` 在新 schema 上可重放；`seed_tests` 绿。

- [ ] **Step 1: 先跑 seed 测试看红**

Run: `cargo test -p db --test seed_tests`
Expected: FAIL(`no such table: devices` / `no column named device_id` 类错误）

- [ ] **Step 2: 机械改名**

```bash
perl -pi -e '
  s/\bdevice_traces\b/thing_traces/g;
  s/\bdevice_memory\b/thing_memory/g;
  s/\bdevice_alarm_rules\b/thing_alarm_rules/g;
  s/\bdevice_alarms\b/thing_alarms/g;
  s/\btarget_device_id\b/target_thing_id/g;
  s/\bdevice_limit\b/thing_limit/g;
  s/\bdevice_id\b/thing_id/g;
  s/\bdevices\b/things/g;
  s/\bdevice_type\b/category/g;
' crates/db/src/seed/demo.sql crates/db/src/seed/system.sql crates/db/src/seed.rs
```

然后人工检查两处 sed 够不着的地方：
1. `system.sql` 中 `features` JSON 里的 `"device_group": true/false` —— 这是订阅特性开关的业务 key，改为 `"thing_group"`；同时 `grep -rn "device_group" apps/cloud/src crates/` 把读取侧一并改（预期在 `apps/cloud/src/domains/admin/system/features.rs` 或租户配额校验处）。
2. demo.sql/system.sql 文案（如"各类传感器设备模板"）不改——中文文案不属于 schema。

- [ ] **Step 3: 跑测试确认绿**

Run: `cargo test -p db --test seed_tests`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/db/src/seed/ crates/db/src/seed.rs $(grep -rl "thing_group" apps/cloud/src crates/ 2>/dev/null)
git commit -m "chore(db): align seed data with thing-renamed schema"
```

---

### Task 4: crates/db 代码改名

**Files:**
- Modify: `crates/db/src/thing.rs`、`device.rs`、`device_trace.rs`、`alarm.rs`、`alarm_rule.rs`、`event.rs`、`memory.rs`、`workspace.rs`、`thing_template.rs`、`batch_command.rs`、`heartbeat.rs`、`lib.rs`、`device_row_mapper.rs`
- Test: `cargo test -p db`

**Interfaces:**
- Consumes: Task 1 新 schema。
- Produces: db crate 公开 API 中 DB 行类型的字段名：`thing_id`、`category`（下游 cloud 在 Task 6 对齐）；`db::device` 模块仍存在（Task 5 才合并）。

- [ ] **Step 1: 机械改名（排除驱动 wire 模块）**

```bash
# device_command.rs / device_property.rs / cache/device_cache.rs 是驱动协议契约,先排除,编译错误驱动手工处理
files=$(grep -rln "device" crates/db/src crates/db/tests --include="*.rs" \
  | grep -v -E "device_command\.rs|device_property\.rs|device_cache\.rs|device\.rs$")
perl -pi -e '
  s/\bdevice_traces\b/thing_traces/g;
  s/\bdevice_memory\b/thing_memory/g;
  s/\bdevice_alarm_rules\b/thing_alarm_rules/g;
  s/\bdevice_alarms\b/thing_alarms/g;
  s/\btarget_device_id\b/target_thing_id/g;
  s/\bdevice_limit\b/thing_limit/g;
  s/\bdevice_id\b/thing_id/g;
  s/\bdevices\b/things/g;
  s/\bdevice_type\b/category/g;
' $files
```

`device.rs` 单独处理（Task 5 要合并，先只改 SQL 字符串，不改 Rust 字段）:
用编辑器把其中的 SQL 字面量按同样规则替换；`Device`/`DeviceCriteria` 等 Rust 类型与字段名保持不动。

- [ ] **Step 2: 编译 + 修正**

```bash
cargo test -p db 2>&1 | tail -40
```

修正规则：
- `FromRow` 结构体字段名必须与列名一致 → 字段改名（如 `ThingResource.device_id` → `thing_id`，删除多余的 `#[sqlx(rename)]`)。
- 驱动 wire 三件套（`device_command.rs`/`device_property.rs`/`device_cache.rs`）若引用 `devices` 表 SQL → 只改 SQL 字符串；字段若是 MQTT/插件 payload → 保留原名，必要时加 `#[sqlx(rename = "thing_id")]`。
- 字符串字面 `'device'`(tags type、events source_type、actor 等运行时值）逐一判断：`tags.type` 的 `'device'` 写入/查询 → 改 `'thing'`;`events.source_type='device'`、`actor='device'` 是事件来源语义 → **保留**（不属于本次映射）。

- [ ] **Step 3: 验证零残留**

```bash
# SQL 语境中不应再有旧名(驱动 wire 三件套除外)
grep -rn "\bdevices\b\|\bdevice_id\b\|\bdevice_traces\b\|\bdevice_memory\b\|\bdevice_alarm" \
  crates/db/src --include="*.rs" \
  | grep -v -E "device_command\.rs|device_property\.rs|device_cache\.rs" \
  | grep -E "SELECT|INSERT|UPDATE|DELETE|FROM|JOIN|WHERE|REFERENCES|pragma"
# 期望:无输出
cargo test -p db
# 期望:全绿
```

- [ ] **Step 4: Commit**

```bash
git add -A crates/db
git commit -m "refactor(db): rename device identifiers to thing across db crate"
```

---

### Task 5: db 模块收敛 —— device.rs 合并进 thing.rs

**Files:**
- Modify: `crates/db/src/thing.rs`（接收 device.rs 全部内容）
- Delete: `crates/db/src/device.rs`
- Modify: `crates/db/src/lib.rs`（移除 `pub mod device`，改模块名 `device_trace`→`thing_trace`)
- Rename: `git mv crates/db/src/device_trace.rs crates/db/src/thing_trace.rs`

**Interfaces:**
- Consumes: Task 4 后的 db crate。
- Produces: 唯一入口 `db::thing`；调用方路径 `db::device::*` → `db::thing::*`（类型名 `Device`/`DeviceCriteria` 等**保持不变**，PR-2 再改类型名）。

- [ ] **Step 1: 盘点 device.rs 的公开项与调用方**

```bash
grep -n "^pub " crates/db/src/device.rs
grep -rn "db::device::\|crate::device::" apps/cloud/src crates/ --include="*.rs" | grep -v "device_command\|device_property\|device_row_mapper\|device_trace" | wc -l
```

- [ ] **Step 2: 合并**

1. 把 `device.rs` 全部类型与函数追加进 `thing.rs`（保留 `//!` 模块注释说明两段来源）。
2. 合并重复 import；若出现同名项（如两边都有 `find_by_id`)，将 device.rs 来源的加 `_legacy` 后缀并在调用方同步改。
3. `git rm crates/db/src/device.rs`。
4. `git mv crates/db/src/device_trace.rs crates/db/src/thing_trace.rs`，改 `lib.rs`:`pub mod thing_trace;`（删除 `pub mod device;` 与 `pub mod device_trace;`)。
5. 全 workspace 替换调用路径：

```bash
perl -pi -e 's/\bdb::device::/db::thing::/g; s/\bcrate::device::/crate::thing::/g' \
  $(grep -rln "db::device::\|crate::device::" apps/cloud/src crates --include="*.rs" \
    | grep -v -E "device_command|device_property|device_row_mapper|device_cache")
```

注意 `device_row_mapper.rs` 是 device.rs 的辅助模块，若仅被 thing.rs 使用则保留文件名不动（PR-2 再议）；编译错误驱动处理。

- [ ] **Step 3: 验证**

```bash
cargo test -p db && cargo check -p tinyiothub-cloud
# 期望:全绿
```

- [ ] **Step 4: Commit**

```bash
git add -A crates/db apps/cloud
git commit -m "refactor(db): merge device.rs into thing.rs as single thing persistence module"
```

---

### Task 6: apps/cloud + crates/core 改名

**Files:**
- Modify: `apps/cloud/src/**`(79 个含 device 引用的文件，见 grep)、`crates/core/src/models/**`
- Test: `cargo check -p tinyiothub-cloud` + `cargo test`(workspace)

**Interfaces:**
- Consumes: Task 4/5 的 db API(`thing_id`、`category` 字段）。
- Produces: cloud 编译通过；JSON API 字段 `deviceId`→`thingId`、`deviceType`→`category`（前端 PR-2 适配）；驱动 wire payload 字段**不变**。

- [ ] **Step 1: core 先改（被 cloud 依赖）**

```bash
perl -pi -e '
  s/\btarget_device_id\b/target_thing_id/g;
  s/\bdevice_id\b/thing_id/g;
  s/\bdevice_type\b/category/g;
' $(grep -rln "device_id\|device_type" crates/core/src --include="*.rs")
cargo check -p core
```

修正规则：`core::models::device::Device` 结构体的 `device_type` 字段 → `category`;**类型名 `Device` 不改**(PR-2)。事件模型中 `source_type='device'` 等语义值保留。

- [ ] **Step 2: cloud 机械改名（排除驱动 wire 目录）**

```bash
files=$(grep -rln "device" apps/cloud/src --include="*.rs" \
  | grep -v -E "domains/driver/|shared/mqtt_client\.rs")
perl -pi -e '
  s/\bdevice_traces\b/thing_traces/g;
  s/\bdevice_memory\b/thing_memory/g;
  s/\bdevice_alarm_rules\b/thing_alarm_rules/g;
  s/\bdevice_alarms\b/thing_alarms/g;
  s/\btarget_device_id\b/target_thing_id/g;
  s/\bdevice_limit\b/thing_limit/g;
  s/\bdevice_id\b/thing_id/g;
  s/\bdevices\b/things/g;
  s/\bdevice_type\b/category/g;
' $files
```

- [ ] **Step 3: 驱动域手工处理**

`apps/cloud/src/domains/driver/**` 与 `shared/mqtt_client.rs`:
- SQL 字符串内的 `devices`/`device_id`/`device_type` → 按映射改（编辑器内逐个文件处理，约 10 个文件：`legacy/diagnostics.rs`、`legacy/monitoring.rs`、`legacy/performance.rs`、`legacy/query_service_impl.rs`、`legacy/service.rs`、`gateway/service.rs`、`gateway/types.rs`、`heartbeat/types.rs`、`heartbeat/handler.rs`、`plugin/storage/*`)。
- Rust 结构体的 wire 字段（MQTT/插件协议）`device_id`/`device_type` → **保留**；若该结构体同时被 `FromRow` 用于改名列，加 `#[sqlx(rename = "thing_id")]` / `#[sqlx(rename = "category")]`。

- [ ] **Step 4: 编译修正循环**

```bash
cargo check -p tinyiothub-cloud 2>&1 | grep -E "^error" | head -30
```

循环修正直到零 error。常见模式：
- axum path 参数 `/{device_id}/...` 被 sed 成 `/{thing_id}/...` → handler 的 `Path<...>` 字段/提取器同名改动已同步，若不一致以编译器报错为准对齐。
- `serde` JSON 字段变化导致测试断言失败 → 更新断言为 `thingId`/`category`，并在 PR 描述中标注前端 breaking。
- 路由路径字符串 `"/{id}/devices"` → 已被 sed 为 `"/{id}/things"`，保留（PR-2 统一整理路由）。

- [ ] **Step 5: 验证零残留 + 全量测试**

```bash
# SQL 语境残留检查(驱动 wire 目录除外)
grep -rn "\bdevices\b\|\bdevice_id\b" apps/cloud/src --include="*.rs" \
  | grep -v -E "domains/driver/|mqtt_client" \
  | grep -E "SELECT|INSERT|UPDATE|DELETE|FROM |JOIN|WHERE"
# 期望:无输出
cargo test 2>&1 | tail -20
# 期望:workspace 全绿;若有 edge/plugin-sdk 因 core 字段改名而编译失败,对 edge 做同样的
# 字段改名( Rust 字段层,不动 MQTT topic/payload 字面量),使其编译通过即可
```

- [ ] **Step 6: Commit**

```bash
git add -A apps/cloud crates/core
git commit -m "refactor(cloud,core): rename device identifiers to thing (deviceId→thingId JSON breaking, frontend adapts in PR-2)"
```

---

### Task 7: 端到端验证与收尾

**Files:**
- Modify: `CHANGELOG.md`（新增条目）

**Interfaces:**
- Consumes: Task 1-6 全部完成。
- Produces: 可评审的 PR-1。

- [ ] **Step 1: 老库升级实测**

```bash
# 用 data/ 下现有 dev 库(先备份)
cp data/*.db /tmp/tih-dev-backup.db 2>/dev/null || echo "no dev db, skip"
cargo run -p tinyiothub-cloud 2>&1 | head -30 &
sleep 15 && kill %1
# 期望:迁移日志无 error;启动不因 foreign_key_check 中止
sqlite3 data/*.db "SELECT name FROM sqlite_master WHERE name IN ('things','devices'); SELECT COUNT(*) FROM pragma_foreign_key_check;"
# 期望:只有 things;FK 违规 0
```

- [ ] **Step 2: 全量门禁**

```bash
cargo test 2>&1 | tail -10        # 全绿
cargo clippy --workspace 2>&1 | grep -E "^error|^warning: unused" | head   # 无新增 error/unused
```

- [ ] **Step 3: CHANGELOG + 自检残留**

```bash
grep -rn "\bdevices\b" crates/db/migrations/20260825000001_rename_device_to_thing.sql | grep -v "RENAME\|DROP\|idx_devices" 
# 期望:无输出(迁移自身引用旧名处只有 RENAME/DROP)
grep -c "device" crates/db/src/seed/demo.sql
# 期望:仅中文文案命中,无 schema 命中
```

CHANGELOG.md 顶部 `[Unreleased]` 下加：

```markdown
### Changed
- **BREAKING (DB/API)**: device schema 全面更名为 thing——表 `devices`→`things` 等 5 张,列 `device_id`→`thing_id`(13 张表)、`device_type`→`category`、`device_limit`→`thing_limit`;JSON 字段 `deviceId`→`thingId`、`deviceType`→`category`。老库经迁移 `20260825000001` 自动升级(启动前自动备份)。驱动 MQTT/插件协议字段暂不变化。前端适配在后续 PR。
```

- [ ] **Step 4: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: changelog for device→thing DB rename (PR-1)"
```

---

## Self-Review 记录

- **Spec 覆盖**:5 表改名 ✅(Task 1 迁移 §1)、13 表 device_id ✅(§2，含 jobs 单列）、device_type→category ✅(§3，含 thing_templates)、tags rebuild ✅(§4)、触发器 ✅(§5)、索引 ✅(§6)、db/cloud SQL ✅(Task 4/6)、seed ✅(Task 3)、device.rs/thing.rs 合并 ✅(Task 5)、一次编译通过 ✅(Task 6 Step 4 + Task 7)。
- **Placeholder 扫描**:Task 2 测试含"若报错补列"说明——events 的 NOT NULL 列以 baseline 实际为准，执行时按编译/运行错误补齐，属可接受的运行时驱动修正；其余步骤均有完整代码/命令。
- **类型一致性**：迁移产出的 `thing_id`/`category`/`thing_traces` 等名称在 Task 1 测试断言、Task 4/6 sed 映射、Task 3 seed 中一致；`db::thing::` 路径在 Task 5 定义并在 Task 6 消费。
- **风险备忘**:① `knowledge_relations.device_id` 的列归属以 Step 0 grep 复核为准；② sed 改了路由路径与 JSON 字段属预期内 breaking(PR-2 收尾）;③ 老库升级前迁移器自动备份，`data/` 实测在 Task 7 Step 1。
