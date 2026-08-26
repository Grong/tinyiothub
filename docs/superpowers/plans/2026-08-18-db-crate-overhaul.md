# db Crate 整改实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 crates/db 整改为 buzz-db 式 Db 门面（19+4 个 Repository struct 打散为 pub(crate) 自由函数 + impl Db 薄委托）、68 个历史迁移 squash 为单基线、种子数据迁入两级 seed.rs、cloud 侧 508 处裸 SQL 全部收编。

**Architecture:** `pub struct Db`（lib.rs 定义）是唯一存储实例；每领域文件三段式（Row 类型 → pub(crate) SQL 自由函数 → impl Db 委托块）；迁移 DDL-only，基线从"全新跑旧链"的库导出；存量库删库重建。

**Tech Stack:** Rust 2024, sqlx 0.9.0-alpha.1 (sqlite), tokio

**Spec:** `docs/superpowers/specs/2026-08-18-db-crate-overhaul-design.md`（v2，D2-D7 已并入）

## Global Constraints

- 业务层唯一调用形态：`state.db.<domain_method>(...)`；`Db` 是 state 唯一存储实例
- 领域 SQL 函数 `pub(crate)`；单语句函数接 `&SqlitePool`，多语句事务函数接 `&mut Transaction<'static, Sqlite>`（buzz 先例）
- 方法命名：平铺 + 领域前缀（`find_device_properties` / `insert_agent_run`）
- 迁移 DDL-only：非 baseline 迁移出现 `INSERT INTO` 即 CI 失败
- cloud 生产代码出现 `sqlx::query` 即 CI 失败（测试经 db 的 `testing` feature 豁免）
- 无 re-export 摆渡层；lib.rs 公共面显式列出
- 存量库过渡 = 删库重建（文档一句），不写迁移工具
- 每 Task 独立 commit；commit 前缀 `refactor(db):`；全程 `cargo test --workspace` 绿
- baseline 导出来源：**全新 DB 跑旧 68 迁移链**的库（不用手头开发库——其数据带伤）

## 关键实测事实（实现前必读）

- **struct 清单（23 个）**：db crate 内 16——AlarmRepository、AlarmRuleRepository（alarm.rs）、CronJobRepository（cron_job.rs）、CronRunRepository（cron_run.rs）、DeviceRepository（device.rs）、DriverInstallationRepo（driver_installation.rs）、EventRepository+RealTimeEventRepository（event.rs）、HeartbeatTaskRepository（heartbeat.rs）、NotificationHistoryRepository+NotificationRuleRepository（notify.rs）、PermissionRepository+PermissionGroupRepository（permission.rs）、PolicyRepository（policy.rs）、RoleRepository（role.rs）、SessionRepository（session.rs）、TagRepository+TagBindingRepository（tag.rs）、TenantRepository（tenant.rs）、UserRepository（user.rs）、WorkspaceRepository（workspace.rs）、AgentRunsRepository（agent_runs.rs）。cloud 内 4——`ThingRepo`（thing/repo.rs）、`TemplateRepository`（thing/template/repo.rs）、`DeviceTraceRepository`（thing/legacy/trace_repository.rs）、`BatchCommandRepository`（admin/batch/batch_command.rs，无字段 unit struct）
- struct 构造形态两种：`XxxRepository::new(database.clone())`（Database 按值）与 `::new(pool)`；`Database::new(pool)` 只是 pool 薄包装
- 事务全部为操作内自包含：`pool.begin()` 于 session.rs:273、device_command.rs:56,131、device.rs:519,684,702、thing/repo.rs:363、thing/template/types.rs:317
- edge 消费面：`apps/edge/src/shared/storage.rs`（Database/create_pool_without_migrations）、`app_state.rs`、`modules/{driver,config_mgmt,device,offline}/service.rs`
- `Db::pool()` 合法基础设施消费方（保留 pub）：state.rs:363 `MemoryStore::new(pool.clone())`、mqtt 连接池、各 state 切片 db_pool 字段
- 待删测试：`apps/cloud/src/tests/migration_thing_model_tests.rs`、`migrations_thing_agent_loop_tests.rs`；**保留适配**：`crates/db/tests/migration_replay_test.rs`（FK 级联回归）
- 系统种子来源（seed_system 的内容）：20260407000001（默认租户/工作区）、20260516044444（内置模板）、20260329000001（admin 密码修复）等 16 个迁移的 INSERT——实现时逐迁移提取，归属判断标准：生产无它会残废 → system；演示/好看 → demo
- 演示数据来源（seed_demo）：一月 rebuild 迁移的设备/属性/命令（35+15 行）+ 0725 迁移的资源标签种子等

---

## Task 1: Database → Db 更名（纯机械，先行）

**Files:**
- Modify: `crates/db/src/database.rs`（struct Database → Db）、`crates/db/src/lib.rs`（导出更名）
- Modify: 全部消费方（`grep -rln "Database" apps/ crates/ tests/ --include="*.rs"` 中引用 `tinyiothub_storage::Database` 的文件；注意 `tinyiothub_storage::database::Database` 路径形态与 `database::Database` 模块名撞车——模块 `database.rs` 保留文件名，struct 更名 Db）

**Interfaces:**
- Produces: `tinyiothub_storage::Db`（后续全部任务的基础类型）；`AppState.db: Arc<Db>`（原 `database: Arc<Database>`）

- [ ] **Step 1: struct 更名 + lib.rs 导出更新**——`database.rs` 内 `pub struct Database` → `pub struct Db`，`impl` 块同步；lib.rs `pub use database::Database` → `pub use database::Db`
- [ ] **Step 2: 消费方批量改写**——`Database::new(` → `Db::new(`、`Arc<Database>` → `Arc<Db>`、`database::Database` → `database::Db`；用 `cargo check -p tinyiothub-storage` 迭代至零错误，再 `cargo check --workspace`
- [ ] **Step 3: AppState 字段更名**——`state.rs:44` `pub database: Arc<Database>` → `pub db: Arc<Db>`；全部 `state.database` / `self.database` 引用同步（编译器逐个指认）
- [ ] **Step 4: 全量验证 + commit**

```bash
cargo check --workspace && cargo test -p tinyiothub-storage
git add -A && git commit -m "refactor(db): rename Database → Db (Task 1)"
```

预期：纯更名，零行为变化；测试全绿。

## Task 2: 基线导出 + runner 瘦身 + Db::connect

**Files:**
- Create: `crates/db/migrations/20260819000001_baseline.sql`
- Delete: `crates/db/migrations/` 下全部 68 个历史迁移；`apps/cloud/src/tests/migration_thing_model_tests.rs`、`migrations_thing_agent_loop_tests.rs`
- Modify: `crates/db/src/migrations.rs`（瘦身）、`crates/db/src/pool.rs`（create_pool 走 Db::connect）、`apps/cloud/src/bootstrap.rs`（接线）
- Test: `crates/db/tests/migration_replay_test.rs`（适配保留）、新建 `crates/db/tests/baseline_schema_tests.rs`

**Interfaces:**
- Produces:
  - `Db::connect(config: &DatabaseConfig) -> Result<Db, sqlx::Error>`——建池（FK pragma）+ 跑迁移 + FK 完整性检查，一步到位
  - `migrations::run_migrations(&pool)` 保留签名（内部瘦身）；`test_helpers::run_all_migrations` 保留为委托

- [ ] **Step 1: 生成旧链终态参照库**

```bash
# 用当前代码（旧 68 迁移链）建一个全新库：
rm -f /tmp/tih-oldchain.db
# 写一个 10 行的一次性 bin 或复用 migration_replay_test 的思路：
# 直接调 run_migrations 于空库（它跑的就是全部嵌入迁移）
```
产出 `/tmp/tih-oldchain.db`——这是基线正确性的**基准**。

- [ ] **Step 2: 导出基线 schema**

```bash
sqlite3 /tmp/tih-oldchain.db ".schema" | grep -v "_sqlx_migrations" > /tmp/baseline-raw.sql
```
整理为 `20260819000001_baseline.sql`：文件头注释（来源、生成日期、校验方式）、`PRAGMA defer_foreign_keys` 不需要、按依赖序排 CREATE TABLE（导出顺序即 sqlite_master 序，通常已满足；FOREIGN KEY 引用在 SQLite 建表时不强制顺序）。**纯 DDL：剔除全部 INSERT**。

- [ ] **Step 3: 写 schema 等价验证测试（先失败）**

`crates/db/tests/baseline_schema_tests.rs`：

```rust
// 从库的 sqlite_master 提取规范化 schema 集合：(type, name, 规范化 sql)
async fn schema_set(db_url: &str) -> std::collections::BTreeSet<(String, String, String)> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1).connect(db_url).await.unwrap();
    sqlx::query_as::<_, (String, String, String)>(
        "SELECT type, name, sql FROM sqlite_master
         WHERE type IN ('table','index','trigger') AND name NOT LIKE '_sqlx%'
           AND name NOT LIKE 'sqlite_%' ORDER BY 1,2")
        .fetch_all(&pool).await.unwrap()
        .into_iter()
        .map(|(t, n, s)| (t, n, s.split_whitespace().collect::<Vec<_>>().join(" ")))
        .collect()
}

#[tokio::test]
async fn baseline_schema_matches_old_chain() {
    // 库 B（旧链终态）路径经 env var TIH_OLDCHAIN_DB 传入；缺省时 skip——
    // 它是导出时的一次性验证，非常驻 CI。
    let Ok(b_url) = std::env::var("TIH_OLDCHAIN_DB") else { return };
    // 库 A：baseline.sql 直建
    let a_path = std::env::temp_dir().join(format!("baseline-only-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&a_path);
    let a_url = format!("sqlite://{}?mode=rwc", a_path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1).connect(&a_url).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/20260819000001_baseline.sql"))
        .execute(&pool).await.unwrap();
    let a = schema_set(&a_url).await;
    let b = schema_set(&b_url).await;
    assert_eq!(a, b, "baseline 与旧链终态 schema 不一致");
}
```

- [ ] **Step 4: 跑验证**——`TIH_OLDCHAIN_DB=/tmp/tih-oldchain.db cargo test -p tinyiothub-storage --test baseline_schema_tests` 必须 PASS；不一致就修 baseline 直到逐表 diff 为空。把 diff 为空的输出贴进 commit message

- [ ] **Step 5: 删除历史迁移 + 瘦身 runner**

`migrations.rs` 终态（保留项）：`run_migrations`（FK OFF 专用连接 + `backup_before_migrate` + `enforce_foreign_key_integrity`）；删除：`SKIP_MIGRATIONS`、`cleanup_orphaned_migration_records`、`prepare_thing_model_copy`、`repair_thing_model_data`、`ensure_schema_consistency`、`load_migrations` 过滤逻辑（`sqlx::migrate!` 直接用）。

- [ ] **Step 6: 适配 FK 级联回归测试**——`migration_replay_test.rs` 的断言基数改为基线后的真实行数（系统种子行将随 Task 3 的 seed_system 到位；本任务中该测试先只断言"fresh 库迁移成功 + thing_properties 表存在且含 UNIQUE(device_id,name)"，种子断言在 Task 3 恢复）

- [ ] **Step 7: 删除 lineage 测试 + Db::connect 接线**——`Db::connect`（建池+迁移+完整性）；`pool.rs` 的 `create_pool` 改为内部委托 `Db::connect` 或删除（调用方改 `Db::connect`）；bootstrap.rs 改一行调用。`create_pool_without_migrations` 保留（edge 用）

- [ ] **Step 8: 全量验证 + commit**

```bash
cargo test --workspace
git add -A && git commit -m "refactor(db): baseline migration + slim runner + Db::connect (Task 2)"
```

## Task 3: seed.rs 两档 + test_pool 直建基线 + testing feature

**Files:**
- Create: `crates/db/src/seed.rs`
- Modify: `crates/db/src/lib.rs`（`pub mod seed`）、`crates/db/src/test_helpers.rs`、`crates/db/Cargo.toml`（`[features] testing = []`）、`apps/cloud/Cargo.toml`（dev-dependencies 的 db 加 `features = ["testing"]`）、`apps/cloud/src/bootstrap.rs`（seed 调用 + 配置开关）、`app_settings.toml` + `app_settings.example.toml`（`[seed]` 节）
- Test: `crates/db/tests/seed_tests.rs`

**Interfaces:**
- Produces:
  - `seed::seed_system(db: &Db) -> Result<()>`——默认租户/工作区、admin 用户、内置模板（生产必需；幂等）
  - `seed::seed_demo(db: &Db) -> Result<()>`——8 演示设备 + 属性/命令（幂等）
  - `test_helpers::test_pool() -> SqlitePool`——直建基线（不跑迁移链）
  - testing feature 下：`test_helpers::fixture_pool_with_db() -> (SqlitePool, Db)` 等夹具构造器

- [ ] **Step 1: 提取种子内容**——从一月 rebuild 迁移与 20260407000001/20260516044444/20260329000001 等 16 个迁移中逐条提取 INSERT，按归属标准分两档（生产必需 → system；演示 → demo）。把归属判断表写进 commit message
- [ ] **Step 2: 写失败测试**

```rust
#[tokio::test]
async fn seed_system_is_idempotent_and_creates_default_workspace() {
    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();
    seed::seed_system(&db).await.unwrap();  // 二次调用零变化
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces")
        .fetch_one(db.pool()).await.unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn seed_demo_creates_env01_with_properties() {
    let pool = test_helpers::test_pool().await;
    let db = Db::new(pool);
    seed::seed_system(&db).await.unwrap();
    seed::seed_demo(&db).await.unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_properties WHERE device_id = 'device-env-01'")
        .fetch_one(db.pool()).await.unwrap();
    assert_eq!(n, 5);
}
```

- [ ] **Step 3: 实现 seed.rs**——用各领域 pub(crate) 函数写入（不用裸 SQL 拼接）；EXISTS 守卫幂等
- [ ] **Step 4: test_pool 直建基线**——`test_helpers::test_pool()` 改为：内存池 + `sqlx::raw_sql(include_str!("../migrations/20260819000001_baseline.sql"))`；全 db 测试转绿（这是"不跑迁移链"的验证点：记录测试启动耗时对比进 commit message）
- [ ] **Step 5: bootstrap 接线 + 配置开关**——`app_settings.toml` 加 `[seed] demo_data = true`；bootstrap 在 `Db::connect` 后依次 `seed_system` →（开关）`seed_demo`
- [ ] **Step 6: 恢复 FK 级联回归测试的种子断言 + profile E2E**（Task 2 Step 6 暂缓的部分）：
  a. fresh 库 run_migrations + seed 后 env01 有 5 属性（migration_replay_test.rs 断言恢复）
  b. profile E2E（spec §6 USER FLOWS）：在 `apps/cloud/src/tests/` 加用例——测试 app 启动（走真实 Db::connect + seed）后 `GET /api/v1/things/device-env-01/profile` 返回的 JSON 含非空 `properties`（≥5 条）与 `actions`（≥2 条）。复用现有 `test_utils` 的 app 构造与 JWT 夹具；该用例在 seed 开关关的 app 上应得空数组（同文件第二用例）
- [ ] **Step 7: 全量验证 + commit** → `refactor(db): two-tier seed module + baseline-built test_pool (Task 3)`

## Task 4: 试点领域转换（cron）—— 建立转换配方

**Files:**
- Modify: `crates/db/src/cron_job.rs`、`crates/db/src/cron_run.rs`、`crates/db/src/lib.rs`
- Modify: 调用点（`grep -rn "CronJobRepository\|CronRunRepository" apps/ crates/ --include="*.rs"`）

**Interfaces:**
- Produces（后续领域任务的命名与形态基准）:
  - 自由函数：`pub(crate) async fn create_job(pool: &SqlitePool, req: &CreateCronJobRequest, ...) -> Result<CronJob>`（原名动词保留，去 struct 前缀）
  - Db 委托：`impl Db { pub async fn create_cron_job(&self, req: &CreateCronJobRequest) -> Result<CronJob> { create_job(&self.pool, req).await } }`

**转换配方（每个后续领域任务逐字复用此流程）：**

- [ ] **Step 1: struct 方法 → 自由函数**。对 struct 每个 `pub async fn m(&self, args)`：
  - 函数体顶部 `self.database.pool()` / `self.pool` → 参数 `pool: &SqlitePool`
  - 签名改 `pub(crate) async fn m(pool: &SqlitePool, args)`
  - `map_*_row` 等私有助手已是自由函数，不动
- [ ] **Step 2: 同文件追加 impl Db 委托块**（方法名 = 领域前缀 + 动宾）：

```rust
impl Db {
    /// 创建 cron 任务（计算下次运行时间）。
    pub async fn create_cron_job(&self, req: &CreateCronJobRequest, created_by: &str) -> Result<CronJob> {
        create_job(&self.pool, req, created_by).await
    }
    // … 每个自由函数一个委托
}
```

- [ ] **Step 3: 删除 struct 与 new()**；lib.rs 的 `pub use cron_job::CronJobRepository` 等导出删除
- [ ] **Step 4: 调用点重写**——`xxx_repo.m(a, b)` → `db.m(a, b)`；`XxxRepository::new(...)` 构造点删除；持有 repo 的字段/参数改 `Arc<Db>` 或直接用 `state.db`
- [ ] **Step 5: 验证**——`cargo check --workspace` 零错误；`cargo test -p tinyiothub-storage cron` 绿；`grep -rn "CronJobRepository\|CronRunRepository" apps/ crates/ tests/` 零命中
- [ ] **Step 6: Commit** → `refactor(db): convert cron domain to Db facade (Task 4)`

## Task 5-12: 逐领域转换（复用 Task 4 配方）

每个任务 = 对下列领域执行 Task 4 的 Step 1-6，并在该 commit 内**顺带收编该领域在 cloud 的裸 SQL**（D4b 严格收口）：

| Task | 领域（文件） | struct | 特殊注意 |
|---|---|---|---|
| 5 | session（session.rs） | SessionRepository | :273 内部事务——函数签名不动（pool.begin 在函数体内） |
| 6 | tag + permission + role（tag.rs/permission.rs/role.rs） | TagRepository、TagBindingRepository、PermissionRepository、PermissionGroupRepository、RoleRepository | 三个小文件合一个 commit |
| 7 | tenant + user + workspace（tenant.rs/user.rs/workspace.rs） | TenantRepository、UserRepository、WorkspaceRepository | tenant/workspace 的 cloud handler 裸 SQL 多，收编面最大的一组 |
| 8 | device + device_property + device_command + driver_installation（device.rs 等 4 文件） | DeviceRepository、DriverInstallationRepo | device.rs:519,684,702 三处内部事务；edge 涟漪在此组（edge 的 device/config_mgmt/driver service 调用点重写） |
| 9 | heartbeat + agent_runs + policy（heartbeat.rs/agent_runs.rs/policy.rs） | HeartbeatTaskRepository、AgentRunsRepository、PolicyRepository | agent 域消费方多（persist.rs subscriber、orchestrator 接线）；policy/skills crate 的消费点直改 |
| 10 | event + notify + notification_channel（event.rs/notify.rs/notification_channel.rs） | EventRepository、RealTimeEventRepository、NotificationHistoryRepository、NotificationRuleRepository | runtime_ports.rs 的 EventRetentionAdapter SQL 收进 event.rs（删 Database 通用助手的前置） |
| 11 | alarm（alarm.rs 1667 行拆分为 alarm.rs + alarm_rule.rs） | AlarmRepository、AlarmRuleRepository | 大文件拆分在此任务内完成（row 类型随各自 repo） |
| 12 | cloud 侧 4 struct + 剩余裸 SQL 域（thing/repo.rs、thing/template/repo.rs、thing/legacy/trace_repository.rs、admin/batch、auth handlers、tenant handlers、admin/monitoring、driver/gateway、marketplace installer 等） | ThingRepo、TemplateRepository、DeviceTraceRepository、BatchCommandRepository | thing/repo.rs:363、template/types.rs:317 的事务体迁入 db（函数接 `&mut Transaction<'static, Sqlite>`）；auth/tenant/admin 等裸 SQL 收编进对应领域文件（没有对应领域文件的新建 `auth.rs`/`admin.rs` 或归入就近领域——按表归属判断） |

**每任务验收**（写进各自 commit message）：`grep -rn "<StructName>" apps/ crates/ tests/` 零命中；`cargo test --workspace` 绿；该领域 cloud 裸 SQL 零残留（`grep -rn "sqlx::query" <领域相关 cloud 文件>` 为零，测试文件除外）。

## Task 13: Db 瘦身收尾 + CI 守门

**Files:**
- Modify: `crates/db/src/database.rs`（删 query/query_first/execute/execute_with_params；保留 pool()/begin_transaction()）、`crates/db/src/lib.rs`（公共面显式化）
- Modify: `.github/workflows/ci.yml`（两条新守门）

- [ ] **Step 1: 删通用助手**——确认 `grep -rn "\.execute_with_params\|\.query_first(" apps/ crates/ --include="*.rs" | grep -v "sqlx\|test"` 零命中后删除四个方法
- [ ] **Step 2: lib.rs 公共面**——删除全部 `pub use <domain>::*` 通配；显式列出 `Db`、`DbError`/`Result`、Row 类型、`Filter/Pagination` 等
- [ ] **Step 3: CI 守门（两条新 step）**：

```yaml
- name: DB SQL Residence Guard
  run: |
    if grep -rEn --include='*.rs' 'sqlx::query' apps/cloud/src apps/edge/src --exclude-dir=tests | grep -v '_test\|test_utils\|#\[cfg(test)\]'; then
      echo "❌ raw SQL outside crates/db"; exit 1
    fi
- name: Migration DDL-only Guard
  run: |
    if ls crates/db/migrations/*.sql | grep -v baseline | xargs grep -l "INSERT INTO" 2>/dev/null; then
      echo "❌ migrations must be DDL-only (seeds go to seed.rs)"; exit 1
    fi
```

- [ ] **Step 4: 守门自证**——向某 cloud handler 注入一行 `sqlx::query("SELECT 1")` 确认守卫 fail，回滚；向新迁移文件注入 INSERT 确认 fail，回滚。证据记入报告
- [ ] **Step 5: Commit** → `refactor(db): slim Db + CI guards for SQL residence and DDL-only migrations (Task 13)`

## Task 14: 文档 + 验收

**Files:**
- Modify: `AGENTS.md`（db 段落重写）、`README.md`（结构树）

- [ ] **Step 1: AGENTS.md 更新**——db 段落写：Db 门面规则（唯一实例、平铺+前缀命名、pub(crate) 领域函数、事务参数形态）、DDL-only 迁移规则、seed 两档、testing feature、edge 暂留 TODO（D6：后期 edge 直接只用 db baseline）
- [ ] **Step 2: 验收清单逐项核对**（spec §7 八条，逐条贴证据）
- [ ] **Step 3: Commit** → `refactor(db): docs + acceptance for crate overhaul (Task 14)`

## 验收清单（spec §7）

- [ ] `cargo build --workspace && cargo test --workspace` 全绿（含 edge target）
- [ ] `grep -rn "Repository::new" apps/cloud/src apps/edge/src` 零命中
- [ ] cloud 生产代码 `sqlx::query` 零命中（CI 守门）
- [ ] migrations/ 只有 baseline + 递增迁移；非 baseline 迁移 INSERT INTO 为零（CI 守门）
- [ ] 新库与旧链终态 schema diff 为空（Task 2 Step 4 证据）
- [ ] `test_pool` 不跑迁移链（耗时对比入 Task 3 commit message）
- [ ] profile E2E：fresh 库 profile 返回 properties/actions（Task 3 Step 6）
- [ ] AGENTS.md 更新

## 风险与备注

- **Task 2 的基线正确性是整个计划的承重墙**：schema diff 必须程序化逐表验证，不接受目测
- Task 5-12 是纯机械转换，编译器兜底；每任务独立 commit 可安全停任一边界
- edge 只保证编译通过 + 调用形态一致；其自建本地表的 schema 统一是后期立项（D6），本期在 `apps/edge/src/shared/storage.rs` 头注 TODO 即可
- 若转换中发现某 struct 方法被 cloud 以"事务内组合"方式调用（Task 4 配方假设不成立），停下来把该路径按规则 10 迁入 db 领域函数——不要给门面开 `&mut Transaction` 后门
