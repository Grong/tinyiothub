# db crate 整改设计 — buzz-db 式 Db 门面 + 迁移基线化

- 日期：2026-08-18（设计经用户逐节确认）
- 状态：已批准
- 分支：`refactor/crates-reorg`
- 前置：2026-08-15 agent crate 抽取已完成（本整改在其之后独立进行）

## 1. 背景与动机

`crates/db`（14.5k 行 / ~23 源文件 / 62 迁移文件）两个问题：

1. **迁移链失控**：`SKIP_MIGRATIONS`、orphan 记录清理、空壳表准备、迁移后数据修复、补列、FK 完整性检查——runner 内 6 层补丁。2026-08-18 实证的 FK 级联 bug（`20260723000001` 在 FK ON 下 DROP `devices` 级联清空 `device_properties`/`device_commands`）证明迁移从未在应用真实环境被验证（sqlite3 CLI 默认 FK OFF，与 sqlx 默认 ON 行为不同）。种子数据写在迁移里（一月 rebuild 迁移含 35 属性 + 15 命令的演示 INSERT）是数据丢失链的直接土壤。
2. **存储实例管理负担**：state 持有 `Arc<Database>`，同时各组合点又独立构造 `XxxRepository::new(...)` 实例（state.rs:261-268 等）——一套数据两种入口，实例生命周期分散。用户明确：不想要 Repository 实例群，参考 buzz-db 的 `Db` 门面（`/Users/chenguorong/code/github/buzz/crates/buzz-db/src/lib.rs`）。

**已确认的决策**：

| 决策点 | 结论 |
|---|---|
| 整改范围 | 迁移 squash 为单基线 + 代码结构重整（不动 schema 设计本身） |
| 存量库过渡 | **不保留数据，删库重建**——不写导出/导入工具，文档一句"删库重启" |
| 种子数据 | 独立 `seed.rs` Rust 模块（经 repo 函数写入），bootstrap 配置开关，默认开 |
| 访问模式 | buzz-db 式：业务层只调 `state.db.<domain_method>(...)`；不直接调自由函数、不维护 Repository 实例 |
| Db 实现形态 | `impl Db` 委托块分散在各领域文件（非 buzz 的 8400 行集中 lib.rs）——改一个领域只动一个文件 |

**非目标**：schema v2（表结构重命名/合并不动）；sqlx `query!` 编译期校验（另立项）；web 前端；其它 crate 的内部重构（agent/llm 等只改 import 与调用形态）。

## 2. 目标架构

```
crates/db/
  migrations/
    20260819000001_baseline.sql     # 唯一基线：78 表全量 DDL（从当前终态导出）
    <未来迁移正常递增>.sql
  src/
    lib.rs                          # pub struct Db 定义 + 公共面显式导出（无通配）
    <domain>.rs                     # 每领域一文件，三段式（见下）
    migrations.rs                   # 瘦身 runner（~80 行，零补丁）
    seed.rs                         # 演示数据（Rust，经领域函数写入，幂等）
    pool.rs                         # 连接创建（pragma、池参数）
    error.rs / models.rs / sql_security.rs / cache/（DeviceCache 保留）
    test_helpers.rs                 # test_pool：直建基线（不跑迁移链）
```

**领域文件三段式**（以 `device_property.rs` 为例）：

```rust
// ① Row 类型（pub，FromRow，与表列一一对应）
// ② SQL 自由函数（pub(crate) —— crate 内唯一写 SQL 的地方）
pub(crate) async fn find_by_device_id(pool: &SqlitePool, id: &str) -> Result<Vec<DeviceProperty>> { ... }
// ③ Db 委托方法（同 crate 内 impl 块分散在领域文件，Rust 合法）
impl Db {
    pub async fn find_device_properties(&self, device_id: &str) -> Result<Vec<DeviceProperty>> {
        find_by_device_id(&self.pool, device_id).await
    }
}
```

**业务层唯一调用形态**：

```rust
let props = state.db.find_device_properties(id).await?;
```

## 3. 关键规则

1. **Db 是唯一存储实例**：`AppState`（及各域 state 切片）只持 `Arc<Db>`。现有 ~10 个 Repository struct（`AlarmRepository`/`AlarmRuleRepository`/`DeviceRepository`/`CronJobRepository`/`CronRunRepository`/`RealTimeEventRepository`/`HeartbeatTaskRepository`/`AgentRunsRepository`/`PolicyRepository` 等）全部打散：方法进领域自由函数（`pub(crate)`）+ Db 加薄委托。
2. **SQL 唯一住所**：领域自由函数。`Database` 的通用 `query/query_first/execute/execute_with_params` 助手删除（全仓实测仅 `apps/cloud/src/shared/runtime_ports.rs` 1 处真实调用，其 SQL 收进 `event.rs` 领域函数）。
3. **跨领域组合逻辑**写在 `impl Db` 方法（buzz 先例：`insert_event` 委托后顺带插 mentions，失败仅 warn 不阻断主流程）。
4. **无 re-export 摆渡**：lib.rs 显式列出公共类型（Db、Row 类型、DbError、Filter/Pagination 等）；消费方从 `tinyiothub_storage::{db::Db 或类型真实家}` 导入。
5. **DeviceCache 保留** `cache/`——内存缓存，非存储实例，与行类型耦合紧，不动。
6. **迁移文件只含 DDL**：此后 migrations/ 出现 `INSERT INTO` 即 CI 失败（baseline 本身除外）。演示/种子数据只能经 `seed.rs`。
7. **测试直建基线**：`test_pool()` 执行 baseline.sql 建库；历史 lineage 测试（`migration_thing_model_tests` 等）整体删除——它们测试的 lineage 已不存在。

## 4. runner 终态

`migrations.rs` 瘦身为约 80 行：

- 无 `_sqlx_migrations` 表 → 全量跑（新库）；有 → 跑增量（sqlx Migrator 原生能力）
- 保留：`backup_before_migrate`（迁移前自动备份）、迁移专用连接 FK OFF（2026-08-18 修复）、`enforce_foreign_key_integrity`（启动门禁）
- 删除：`SKIP_MIGRATIONS`、`cleanup_orphaned_migration_records`、`prepare_thing_model_copy`、`repair_thing_model_data`、`ensure_schema_consistency`、`load_migrations` 的过滤逻辑

**基线正确性验证**（一次性工具/脚本）：新库 baseline 建库与旧链终态库的 `.schema` 输出逐表 diff（表集合、列、索引、触发器）——程序化验证，写入 CI 或一次性脚本随 PR 附证据。

**seed 开关**：`app_settings.toml` 加 `[seed] demo_data = true`（默认 true，生产部署显式关）。bootstrap 在迁移完成后调用 `seed::seed_demo_data(&pool)`（幂等，EXISTS 守卫）。

## 5. 落地步骤（每步独立 commit、全程测试绿）

1. **导出基线**——从当前全新建库导出 schema 整理为 baseline.sql；schema diff 验证；历史迁移文件与 lineage 测试删除；runner 瘦身
2. **seed.rs + test_pool 改造**——演示数据从一月迁移 SQL 抽出为 Rust；`test_pool` 直建基线；删手工 ALTER 夹具
3. **Repository struct 打散**——按领域逐个转换（alarm → device → event → …），每领域一个 commit：方法 → `pub(crate)` 自由函数 + `impl Db` 委托 + 调用点重写（`state.db.<method>()`）
4. **Database → Db 瘦身 + 公共面收敛**——删通用助手、修 runtime_ports.rs、lib.rs 显式导出、`Database` 更名 `Db`（对齐 buzz；`AppState` 字段 `database: Arc<Database>` → `db: Arc<Db>`）
5. **文档**——AGENTS.md（db 段落重写：Db 门面规则、迁移 DDL-only 规则、seed 开关）、README 结构树

**风险控制**：步骤 3 是最大面（~150 方法 + 全部调用点），纯机械转换、编译器兜底；步骤 1 的基线正确性由 schema diff 程序化验证。

## 6. 验收标准

- [ ] `cargo build --workspace` 通过；`cargo test --workspace` 全绿
- [ ] `grep -rn "Repository::new\|Repository {" apps/cloud/src` 零命中（存储 struct 实例绝迹）
- [ ] `grep -rn "tinyiothub_storage::" apps/cloud/src | grep -v "state.db\."` 的调用全部经 `Db` 方法（领域函数 pub(crate)，外部不可达——编译器强制）
- [ ] migrations/ 只有 baseline + 递增迁移；`grep -l "INSERT INTO" crates/db/migrations/*.sql | grep -v baseline` 为零
- [ ] `test_pool` 不跑迁移链（测试启动时间可观测下降）
- [ ] 新库与旧链终态 schema diff 为空
- [ ] AGENTS.md 更新

## 7. NOT in scope

| 项 | 理由 |
|---|---|
| schema v2（表/列重设计） | Q1 明确不走 |
| sqlx `query!` 编译期校验 | 正交改进，需离线缓存基建，另立项 |
| 存量库数据迁移工具 | Q2 明确删库重建 |
| DeviceCache 挪窝 | 与行类型耦合紧，无收益 |
| web 前端 / 其它 crate 内部 | 仅改调用形态 |
