# E4 结转 P2 执行记录：存量物声明回填与直连调用收口

- 日期：2026-10-24
- 分支：`e4-p2-declaration-closeout`（基于 main `295d2209`）
- 判定：三项开放项全部交付；本地全量 cloud lib 通过，fmt/clippy/guards 通过。

## 1. 交付

| # | 事项 | 实现 | 覆盖测试 |
|---|---|---|---|
| 1 | UI 直连 invoke 接入声明校验 | `apps/cloud/src/domains/thing/handler/actions.rs`：在注册动作/参数 schema 之后、确认门之前调用 `AccessService::validate`；user-actionable → 422，声明服务不可用 → 503 | `ui_invoke_rejects_action_not_declared_by_fixed_driver`、`ui_invoke_allows_undeclared_thing_only_when_enforcement_disabled` |
| 2 | 迁移期开关 | 新迁移 `20261024000001_workspace_declaration_enforcement.sql`（默认 1）；`GET/PUT /v1/drivers/enforcement`（admin + workspace）；agent 工具与 UI invoke 在开关关闭时仅放行 `NoDeclaration` | `enforcement_switch_round_trip_and_admin_gate`、`test_invoke_action_allows_missing_declaration_when_enforcement_disabled`、`test_invoke_action_still_rejects_undeclared_tool_when_enforcement_disabled` |
| 3 | 存量物回填 | `POST /v1/drivers/rematerialize?all=false`（admin + workspace）：列出带驱动设备物，跳过已有 active 声明，逐个 `bind_thing`；单物失败进 `failed`，可重跑 | `rematerialize_backfills_things_without_declarations` |
| 4 | MCP 模板建物 | `create_thing_from_template` 先解析 driver/options → `prepare_creation` → 固定能力驱动跳过模板 props/commands → `materialize`（失败删除物） | `mcp_template_creation_materializes_declaration_without_template_capabilities`、`mcp_template_materialize_failure_rolls_back_the_created_thing` |
| 5 | 场景包实例化 | `instantiate_with_declarations`：`Declarative` 节点跳过模板能力（引用模板属性的告警规则跳过并 warning）；提交后逐物 `materialize`，失败进 warnings | `declarative_scene_nodes_skip_template_caps_and_materialize` |
| 6 | 无声明物资源 claim | `AccessService::resolve_action` 无声明时回落合成 `__device__`（与 `resolve_properties` 一致）；开关 ON 时 validate 已先拒绝 | runtime `access` 93 tests |

## 2. 语义与取舍

- **开关默认 fail-closed**：`workspaces.declaration_enforcement = 1`。迁移窗口内管理员显式置 0，存量物才可继续调用；**有声明物的未声明动作/参数越界仍拒绝**。
- **场景包物化是提交后 best-effort**：与安装/升级的 drift-not-rollback 一致——树已提交，物化失败进 `warnings`（UI 可见），不回滚整棵树。
- **固定能力驱动不拷贝模板能力**：`Declarative` 节点的属性/命令完全来自 manifest；动态能力驱动（如 builtin simulator）仍走 `TemplateBootstrap`。
- **回填只处理缺失声明**：默认跳过已有 active 声明；`all=true` 才全量重绑（同版本升级场景由 `install_dir` 的 rebind 覆盖）。

## 3. 验证

| 命令 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过 |
| `cargo clippy --workspace --exclude zeroclaw --all-targets -- -D warnings` | 通过（仅既有 `redis 0.25.4` future-incompat 提示） |
| `cargo test -p tinyiothub-cloud --lib` | **1176 passed / 0 failed / 1 ignored** |
| `cargo test -p runtime --lib access` | 93 passed / 0 failed |
| `bash scripts/guards/sql-residence.sh` | 通过 |
| `bash scripts/guards/ddl-only.sh` | 通过（新迁移 DDL-only） |
| `bash scripts/guards/selftest.sh` | 通过 |

## 4. 结转

- 逐命令超时强制生效（`ValidatedCall.timeout_ms` 消费）仍归 E4b。
- 回填/开关的前端 UI 未做（admin API 已交付，迁移脚本/运维可用）。
- `list_things_with_driver` 与开关读写未加独立 DB 单测，由 handler 集成测试覆盖（真实 SQLite）。
