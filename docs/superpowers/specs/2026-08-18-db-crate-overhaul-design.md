# db crate 整改设计 — buzz-db 式 Db 门面 + 迁移基线化

- 日期：2026-08-18（设计 + 工程评审双闭环；D2-D7 裁决已并入）
- 状态：已批准
- 分支：`refactor/crates-reorg`
- 前置：2026-08-15 agent crate 抽取已完成（本整改在其之后独立进行）

## 1. 背景与动机

`crates/db`（14.5k 行 / ~23 源文件 / **68** 迁移文件）两个问题：

1. **迁移链失控**：`SKIP_MIGRATIONS`、orphan 清理、空壳表准备、迁移后修复、补列、FK 检查——runner 内 6 层补丁；2026-08-18 实证 FK 级联 bug 证明迁移从未在应用真实环境被验证；种子数据写在迁移里是数据丢失链的土壤。
2. **存储实例管理负担**：state 持 `Arc<Database>`，组合点又独立构造 `XxxRepository::new(...)`（state.rs:261-268 等）——一套数据两种入口。用户明确：参考 buzz-db 的 `Db` 门面（`/Users/chenguorong/code/github/buzz/crates/buzz-db/src/lib.rs`），业务层不直接调自由函数、不维护 Repository 实例群。

**已确认决策**（含工程评审裁决）：

| # | 决策点 | 结论 |
|---|---|---|
| Q1 | 整改范围 | 迁移 squash 为单基线 + 代码结构重整（不动 schema 设计） |
| Q2 | 存量库过渡 | 不保留数据，删库重建，不写迁移工具 |
| Q3/D5 | 种子数据 | **两级 seed**：`seed_system()`（默认租户/工作区、admin、内置模板等生产必需行，bootstrap 无条件调用）+ `seed_demo()`（演示设备/属性/命令，配置开关默认开） |
| — | 访问模式 | buzz-db 式：业务层只调 `state.db.<method>(...)`；`impl Db` 委托块分散在各领域文件 |
| D2 | 排布 | 单分支分步执行，每步独立 commit |
| D3 | API 形态 | 平铺 + 领域前缀命名（`db.find_device_properties()`），约定写进 AGENTS.md 守门 |
| D4b/D7 | SQL 收口 | **严格收口、真实规模全做**：cloud 生产+测试共 **508** 处裸 SQL 全部迁入 db 领域函数（`sqlx::query` 在 cloud 出现即 CI 失败，测试豁免经 testing feature，见 §3.8） |
| D6 | edge | edge 是 cloud 的 schema 子集；本期转换保证 edge 编译通过（调用点重写），edge 自建本地表**暂留**——用户已定：后期 edge 直接只用 db baseline（另立项） |
| — | 真实规模 | 68 迁移、**19 个 Repository struct**（db 内 16 + cloud 内 3）、508 处裸 SQL |

**非目标**：schema v2；sqlx `query!` 编译期校验；存量库迁移工具；edge schema 统一（D6 已定后期另立）；web 前端。

## 2. 目标架构

```
crates/db/
  migrations/
    20260819000001_baseline.sql     # 唯一基线：全量 DDL（导出自"全新跑旧链"的库，非开发库）
    <未来迁移正常递增>.sql
  src/
    lib.rs                          # pub struct Db 定义 + 公共面显式导出（无通配）
    <domain>.rs                     # 每领域一文件，三段式（见下）
    migrations.rs                   # 瘦身 runner（~80 行，零补丁）
    seed.rs                         # seed_system() + seed_demo()（幂等，EXISTS 守卫）
    pool.rs                         # 连接创建（pragma、池参数）
    error.rs / models.rs / sql_security.rs / cache/（DeviceCache 保留）
    test_helpers.rs                 # test_pool：直建基线（不跑迁移链）
```

**领域文件三段式**：

```rust
// ① Row 类型（pub，FromRow）
// ② SQL 自由函数（pub(crate)——crate 内唯一写 SQL 的地方）
pub(crate) async fn find_by_device_id(pool: &SqlitePool, id: &str) -> Result<Vec<DeviceProperty>> { ... }
// ③ Db 委托方法（同 crate 内 impl 块分散在领域文件）
impl Db {
    pub async fn find_device_properties(&self, device_id: &str) -> Result<Vec<DeviceProperty>> {
        find_by_device_id(&self.pool, device_id).await
    }
}
```

**业务层唯一调用形态**：`let props = state.db.find_device_properties(id).await?;`

## 3. 关键规则

1. **Db 是唯一存储实例**：`AppState` 及各域 state 切片只持 `Arc<Db>`（字段 `database: Arc<Database>` → `db: Arc<Db>`）。19 个 Repository struct 全部打散。
2. **SQL 唯一住所**：领域自由函数。`Database` 通用 query/execute/query_first/execute_with_params 删除（runtime_ports.rs 的 SQL 收进 `event.rs`）。
3. **跨领域组合逻辑**写在 `impl Db` 方法（buzz 先例：insert_event 顺带插 mentions，失败仅 warn）。
4. **无 re-export 摆渡**：lib.rs 显式公共面；消费方从类型真实家导入。
5. **DeviceCache 保留** `cache/`。
6. **迁移 DDL-only**：baseline 之后 migrations/ 出现 `INSERT INTO` 即 CI 失败；种子只经 seed.rs 两档。
7. **测试直建基线**：`test_pool()` 执行 baseline.sql；历史 lineage 测试（`migration_thing_model_tests` 等）整体删除。
8. **pool() 规则（P0-3 细化）**：`Db::pool()` 保持 pub——基础设施接线（`MemoryStore::new(pool.clone())`、mqtt 连接池、state 切片）合法共享连接池，不算 SQL 逃逸。**SQL 收口的强制点**：CI grep 守门——`apps/cloud/src`（生产代码）出现 `sqlx::query` 即失败。
9. **测试 SQL 出路（P1-6）**：db crate 加 `testing` feature（对齐 agent crate 先例），暴露测试池构造与夹具辅助；cloud `dev-dependencies` 启用。测试夹具的 SQL 经该 feature 的辅助函数或 db 领域函数写入。
10. **事务规则（P0-2，buzz 先例）**：单语句函数接 `&SqlitePool`；多语句事务函数接 `&mut Transaction<'static, Sqlite>` 参数（buzz-db lib.rs:239/909 同款），`Db::begin_transaction()` 保持 pub。cloud 的 `pool.begin()` 调用点（thing/repo.rs:363、template/types.rs:317）的事务体迁入 db 领域函数。
11. **方法命名约定**：平铺 + 领域前缀（`find_device_properties`/`insert_agent_run`/`list_heartbeat_tasks`）；同名概念靠前缀区分（三种 session 先例）。写进 AGENTS.md。
12. **edge 涟漪**：edge 的 5+ 消费文件随转换直改（编译强制）；edge 自建本地表暂留并标注 TODO（D6：后期 edge 直接用 db baseline）。
13. **sql_security.rs 保留**：动态拼接（Filter/Pagination）仍需 `AssertSqlSafe`。

## 4. runner 与接线

- `migrations.rs` 瘦身（~80 行）：无 `_sqlx_migrations` → 全量跑；有 → 增量。保留 `backup_before_migrate`、迁移专用连接 FK OFF、`enforce_foreign_key_integrity`；删除 SKIP/orphan/shell/repair/consistency 五层补丁与 `load_migrations` 过滤逻辑
- **接线（P2-10）**：`Db::connect(config)` = 建池 + 跑迁移 + FK 完整性检查，一步到位；bootstrap 依次调 `Db::connect` → `seed_system()` →（开关开）`seed_demo()`
- **baseline 导出源（P2-11）**：从"全新 DB 跑旧 68 迁移链"的库导出（不用手头开发库——2026-08-18 实证其数据带伤）；导出时程序化核对表数量与逐表 schema diff

## 5. 落地步骤（每步独立 commit、全程测试绿）

0. **更名先行（P1-7 修正）**：`Database` → `Db`、`AppState.database` → `db`——纯更名 commit，避免步骤 3 的二次 churn
1. **导出基线**——新库跑旧链 → 导出 schema → baseline.sql；schema diff 验证；68 历史迁移与 lineage 测试删除；runner 瘦身 + 接线（`Db::connect`）
2. **seed.rs 两档 + test_pool 改造 + testing feature**——演示数据与系统必需行从迁移 SQL 抽出为 Rust；删手工 ALTER 夹具
3. **Repository struct 打散 + SQL 收编**——按领域逐个转换（alarm → device → event → …），每领域一个 commit：struct 方法 → `pub(crate)` 自由函数 + `impl Db` 委托 + 调用点重写 + 该领域 cloud 裸 SQL 收编（含测试夹具）
4. **Db 瘦身收尾**——通用助手删除、lib.rs 公共面显式化、CI 守门上线（`sqlx::query` grep + DDL-only grep）
5. **文档**——AGENTS.md（Db 门面规则、命名约定、DDL-only、seed 两档、testing feature）、README 结构树

## 6. 测试计划（覆盖图）

```
CODE PATHS                                          覆盖
[+] baseline.sql 导出与正确性                        [NEW] schema diff：新库基线 vs 全新跑旧链（表集/列/索引/触发器逐表比对）
[+] runner 瘦身（fresh 全量 / 存量增量）              [NEW] fresh 建库含系统种子 [KEEP] migration_replay_test（FK 级联回归，适配基线后保留）
[+] seed.rs 两档                                     [NEW] 幂等（二次调用零变化）+ demo 开关关时无演示行 + system 档无条件存在
[+] test_pool 直建基线                               [KEEP] 全部 db crate 测试转绿即证；启动耗时对比记入 commit message
[+] struct→Db 转换（19 struct / 每领域 commit）      [KEEP] 现有 800+ 测试为回归网，每 commit 全绿
[+] 508 处 SQL 收编                                  [KEEP] 现有 handler/集成测试覆盖行为；[NEW] CI 守门：cloud 生产代码 sqlx::query 零命中
[+] pool() pub 保留（infra 共享）                    编译器强制（pub(crate) 函数外部不可达）
[+] testing feature 夹具出路                         [NEW] cloud 集成测试经 feature 全绿即证
[+] edge 编译涟漪                                    [KEEP] cargo check --workspace 含 edge target

USER FLOWS
[+] fresh 安装启动 → 迁移 → seed → profile API 有数据  [NEW] 端到端：新库启动后 /things/:id/profile 返回 properties/actions（昨天的 bug 场景固化）
[+] 生产部署（demo 关）启动                            [NEW] demo_data=false 时 seed_system 行在、演示行无
```

**回归规则适用**：FK 级联回归测试（migration_replay_test）随基线化适配保留——这是刚抓过的真实回归，必须活着。

## 7. 验收标准

- [ ] `cargo build --workspace && cargo test --workspace` 全绿（含 edge target 编译）
- [ ] `grep -rn "Repository::new" apps/cloud/src apps/edge/src` 零命中
- [ ] cloud 生产代码 `sqlx::query` 零命中（CI 守门）；领域 SQL 函数 pub(crate)（编译器强制）
- [ ] migrations/ 只有 baseline + 递增迁移；非 baseline 迁移含 `INSERT INTO` 为零（CI 守门）
- [ ] 新库与旧链终态 schema diff 为空
- [ ] `test_pool` 不跑迁移链（启动耗时对比入 commit message）
- [ ] profile E2E：fresh 库 profile 返回 properties/actions
- [ ] AGENTS.md 更新（Db 门面/命名约定/DDL-only/seed 两档/testing feature）

## 8. NOT in scope

| 项 | 理由 |
|---|---|
| schema v2（表/列重设计） | Q1 明确不走 |
| sqlx `query!` 编译期校验 | 正交，另立项 |
| 存量库数据迁移工具 | Q2 删库重建 |
| edge schema 统一 | D6：后期 edge 直接只用 db baseline，另立项 |
| DeviceCache 挪窝 | 与行类型耦合紧，无收益 |

## 9. What already exists（复用清单）

| 已有 | 复用方式 |
|---|---|
| buzz-db `Db`（参照实现） | 门面形态、事务参数形态（Transaction<'static>）、组合逻辑先例的直接蓝本 |
| 现有 22 领域文件 + row 类型 | 三段式的①已就位，②③为形态转换 |
| `test_helpers::run_all_migrations` | 改造为基线直建 |
| `migration_replay_test.rs`（2026-08-18 新增） | FK 级联回归网，适配保留 |
| 现有 800+ 测试 | 转换的回归安全网 |
| buzz `testing` 思路 / agent crate `testing` feature | cloud 测试夹具出路的先例 |
