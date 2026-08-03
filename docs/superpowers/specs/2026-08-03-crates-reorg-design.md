# Crates 重组设计 — workspace 结构整改

> 日期： 2026-08-03 | 分支： chore/open-source-prep | 状态： 已通过 /plan-eng-review + /plan-ceo-review（HOLD SCOPE），锁定执行
> CEO 裁决： 全量拆分（B 路径，否决基础设施先行 C 与合并向下 A）/ 分支不排空（作者各自 rebase；
> PR82 已合并，最大碰撞源已消除）/ CI/CD 按 phase 分散更新 / AGENTS.md 提前到 P1 同步 /
> 安全 TODOS 锚点 P2 同步 / 设计文档 checkbox 为唯一进度 tracker（每周五更新）
> 工程裁决： 去全部前缀（契约 crate 命名 llm）/ 按领域拆细 / error+config+core 合并 /
> agent loop+host 合并 / memory 并入 memory / storage→db 改名 / stubs 移出 workspace /
> **db 全面采用 buzz-db 模式**（领域→db 直接依赖，无 trait 倒置，平铺领域模块）/
> **AppState 采用 axum FromRef 子状态** / **渐进 P4**（thing 试点后每周 1-2 crate）/
> **P4.0 前置**：消灭 mcp AppState 单例 + 斩 thing→agent/mcp 边 + 斩 event→notification 边 /
> core 守门条款 / 测试全部随 crate / scheduler 独立 / plugin loader 并入 runtime /
> 重复归并：DLQ→runtime、设备/agent heartbeat 各归 driver/agent、event 独立

## 1. 现状诊断

### 1.1 核心问题：两套架构并存

`crates/tinyiothub-*` 是按主流分层抽出的 library 层，但 **85k 行业务代码仍全部
留在 `cloud/` 一个 crate 内**，cloud 内部又有平行实现（persistence、agent、web 三处双轨）。
`modules/agent` 已通过 `tinyiothub_ai` 依赖 crates/ai（宿主-适配器关系），分层方向
已被代码验证，缺的只是把 cloud 拆完。

### 1.2 具体问题

1. **god crate**：cloud 85k 行 = api 280 + modules 59k + shared 14.6k + tests 10.9k。
2. **全局 AppState 单例后门**（外部声音发现，已核实）：`cloud/src/modules/mcp/mod.rs`
   有 `static APP_STATE: TokioOnceCell<Arc<AppState>>` + 公开 `get_app_state()`，
   任何模块可绕过 State 萃取拿整个 AppState（thing/handler/actions.rs:118,285 在用）。
3. **已核实的循环引用 5 组**（P0 环扫描 2026-08-03 实测）：
   alarm↔event（event/router.rs 反向引用）、event↔notification（event/mod.rs:94 + service.rs:354）、
   thing→agent/mcp（thing/handler/actions.rs）、agent→thing（agent/tools/thing.rs，与上一项成环）、
   agent↔workspace（agent/agent.rs ↔ workspace/{service,handler/mod,handler/heartbeat}.rs）、
   agent↔chat（chat 规划并入 agent crate，内部消解）。
   另：mcp→alarm、mcp→heartbeat 依赖边（mcp/tools/alarm_mcp.rs）。
4. **存储层分裂**：repository 分散在 tinyiothub-storage 与 cloud/shared/persistence。
5. **占位成员污染 workspace**：plugins/* 7 个 1–4 行空壳；cli 10 行。
6. **命名维度混用**：按层/按领域/按部署单元三种标准并存；`tinyiothub-` 前缀冗余。
7. **依赖方向可疑**：cloud → marketplace（部署单元互相依赖）。

### 1.3 已核实的 TODOS 状态（防止过期条目误导）

- DLQ 已接线：`service_manager.rs:151,235-236`（LoggingDropNotifier +
  SqliteDeadLetterQueue at modules/agent/dlq_repo.rs）。搬迁无死代码。
- 心跳 runner → Trigger 框架迁徙（TODOS P2）**排序裁决：重组先行**，心跳随 P4 原样
  搬进 agent/loop（纯搬迁不改语义），Trigger 语义迁徙在重组完成后独立进行
  （Beck：结构与行为不同时变）。

## 2. 目标结构

```
crates/
  core/            # 合并 error+config+core：类型、错误、配置
                   #   【守门条款】只许 trait + 值类型（DTO/error/config），
                   #   禁止逻辑函数与 I/O；新增类型须说明为何不属于任何领域 crate
  plugin-sdk/      # 驱动作者 SDK；ABI 契约单一事实源
  db/              # 【buzz-db 模式】只依赖 core；具体 repository 实现，
                   #   按领域平铺模块（thing.rs/alarm.rs/event.rs/scheduler.rs…）；
                   #   无 trait 倒置，领域 crate 直接依赖 db 使用具体类型；
                   #   测试用真实 SQLite（不用 mock —— 项目已有 mock 掩盖死路径前科）
  runtime/         # 设备数据面：EventBus、DataServer、驱动框架、DLQ trait
                   #   （+ shared/event、mqtt_client、redis
                   #    + 并入 tinyiothub-plugin 的 loader/registry/sandbox 为 runtime::plugin）
  scheduler/       # 任务调度子系统：引擎 + 调度器 + service（独立理由：多域消费的
                   #   共享子系统；任务 API 在 admin，持久化在 db）
  web/             # HTTP 基础设施：ApiResponseBuilder、middleware、extractors
  # ── AI 层 ──
  llm/             # LLM 调用抽象（契约）：LlmProvider、prompt、session 类型
  memory/          # agent 记忆与反思 + 知识图谱 + SqliteAgentMemoryRepository
  policy/          # 策略引擎 + proposal 类型
  skills/          # skills + tool 注册表
  #   注：thing_agent + orchestrator + heartbeat(活性信号) + AiEvent 并入 agent 的 loop/
  # ── 领域 crate（模型 + service + http handler + router()；直接依赖 db）──
  auth/            # 认证、JWT、auth session、security（安全边界 = 编译边界）
  user/            # user + role + permission
  tenant/          # tenant + workspace（与 user 成环则 fallback 合并 identity）
  thing/           # 物模型：thing + template + tag + 旧 device 管理面（legacy 逐步下线）
  driver/          # 数据接入：drivers 管理 + driver_health + gateway + plugin + 设备心跳
  event/           # 领域事件中心（存储、查询、订阅、路由）；传输用 runtime EventBus
  alarm/           # alarm（依赖 event；event/router.rs 的 alarm 路由移入本 crate）
  notify/          # notification（依赖 event；event→notification 边已斩，单向 notify→event）
  agent/           # agent 完整领域（~19k）：loop/ + host/ + chat/
  mcp/             # mcp（依赖 alarm、agent/heartbeat；单例已在 P4.0 消灭）
  admin/           # system + monitoring + batch + jobs + 调度任务 API + open
apps/
  cloud/           # 薄 bin：main.rs + 总 AppState（含各领域 State 切片）+ 组装
  edge/            # 现有，路径搬迁
  marketplace/     # 现有，路径搬迁；解除对 web placeholder 的依赖
  cli/             # 移出 workspace，实现后回归
drivers/           # 现 plugins/* 空壳，移出 workspace members，实现后回归
```

### 依赖方向（无环，单向）

```
core（守门：只许 trait/值类型）
 ↑           ↑
plugin-sdk   db（buzz 模式：平铺领域模块，具体实现）
 ↑           ▲
runtime      │（领域 crate 直接依赖 db）
 ↑           │
scheduler    │
 ↑           │
llm ──→ memory / policy / skills
 ↑
web
 ↑
领域 crate：各自依赖 core + db + web（+ runtime/llm/scheduler 按需）
   driver→thing；notify→event；alarm→event；agent→event/policy/memory/skills；
   mcp→alarm/agent；user→tenant（成环 fallback 合并 identity）
 ↑
apps/*（只依赖领域 crate 和基础设施 crate；app 之间互不依赖；
  apps/cloud 定义总 AppState，内含各领域 State 切片，axum FromRef 派生）
```

**AppState 模式（已裁决）**：axum `FromRef` 子状态。apps/cloud 的总 AppState 包含各
领域 State 切片；领域 crate 的 handler 只声明自己需要的切片（`State<ThingState>`），
编译期检查，无动态分发。trait 注入先例（HeartbeatTaskRepository）保留在 AI 层内部，
不作为领域 crate 间模式。

**粒度红线**：单 crate ≥ ~400 行（纯契约 crate 除外）。ai 内 15–79 行小模块按职责
归位：prompt/session→llm，knowledge→memory，proposal→policy，alarm 类型→alarm。

**重复归并规则**（单一职责，一处一概念）：
- DLQ trait 是纯通用接口（workspace_id/event_type/payload_json 字符串）→ runtime；
  SqliteDeadLetterQueue 实现 → db
- heartbeat 两个概念：agent 活性信号 → agent/loop；设备心跳 → driver
- cron 四处归并：引擎+调度器 → scheduler crate，任务 API → admin，持久化留 db
  （cron_job/cron_run 表名不动）；modules/cron(6 行壳) 消失
- session 三个概念：auth session→auth，chat session→agent/chat，ai session 类型→llm

## 3. 迁移阶段（渐进执行）

| Phase | 内容 | 验证 |
|---|---|---|
| P0 | CI 全绿基线；**cargo-modules 生成全量 import 环图**（已知 3 组环 + mcp 依赖边，核对无遗漏）。**分支策略：不排空**（CEO 裁决：PR82 已合并，最大碰撞源消除；余 4 分支作者各自 rebase，alarm/ai-event-integration 疑似过期可直接删） | `just ci` |
| P1 | crates/* 改名去前缀；error+config+core 合并为 core；storage→db 改名；**AGENTS.md 同步更新**（依赖方向表/稳定性层级/目录约定/core 守门条款 —— 防 6 周文档撒谎期，P6 只做最终校对） | `cargo check --workspace` |
| P2 | shared/persistence → db（**buzz 模式定型：削除 trait 倒置，具体实现平铺**）；runtime 相关归位（event/mqtt/redis + plugin loader + DLQ trait）；scheduler crate 成立；db 补齐全部 repository；**同步更新 Dockerfile migrations COPY 行 + TODOS #40/#41/#44 锚点（#40 保持 P0 级标注）** | `cargo test -p db -p scheduler` + docker build |
| P3 | shared/middleware、api_response、error_handling → web；security → auth 预备；llm_provider/ai_adapter → llm；shared 仅剩 app_state/service_manager | `cargo check -p tinyiothub-cloud` |
| P3.5 | AI 层归位：llm → memory（并入 memory crate + workspace_memory + knowledge）→ policy（+ proposal）→ skills | `cargo test -p policy` 等逐包 |
| P4.0 | **前置斩环**：① 消灭 `mcp::get_app_state()` 单例（改 State 萃取传递，grep 守门调用数=0）② 斩 thing→agent/mcp 边（take_pending_action/policy_engine/validate_action_params 改由 agent 侧反向提供 API）③ 斩 event→notification 边（NotificationChannelType/NotificationAggregate 类型下沉 core 或 event 侧自定义） | `cargo check -p tinyiothub-cloud` + 环图复扫 |
| P4 | **渐进抽取**（每周 1-2 个，各自独立 PR）：thing（试点）→ auth → user/tenant → event → alarm → driver → notify → agent → mcp → admin。**试点成功判据**：独立 `cargo test -p thing` 绿 + 无 `get_app_state` 调用 + 无反向 import + FromRef 切片落地 + db 平铺模块成型。判据全绿才推广；tests 全部随 crate（跨领域测试住进相关 crate 的 tests/，apps/cloud 只留冒烟）；**每领域抽取时同步更新 ci.yml 架构守卫路径与 Dockerfile templates COPY 行** | 每个 crate `cargo test -p <domain>` + CI 绿 |
| P5 | apps/ 归位；cloud main.rs 变薄（<200 行）；解除 cloud→marketplace；plugins/cli 移出 members；**CI/CD 收尾：apps/ 路径迁移 + release dry-run**（migrations/templates/守卫已随 P2/P4 分散处理）；**E2E 验收（T19 脚本，唯一 E2E 门）** | `just ci` + docker build + E2E 绿 |
| P6 | 文档校对：AGENTS.md 最终校对（主体已随 P1 更新）、CLAUDE.md、README 结构树 | review |

**已接受的风险（用户裁决，不再争论）**：
- 路径资产（fs 相对读、templates/）不做专项审计：include_str!/migrate! 编译期可抓，
  运行时 fs 读取由 P5 E2E 兜底。
- E2E 只在 P5 跑一次：中间 phase 的集成断裂可能累积到 P5 才暴露，定位成本已接受。
- 不测编译时间基线：21 crate 的编译税接受未知。

## 4. 风险与对策

| 风险 | 对策 |
|---|---|
| 循环引用 3 组 + mcp 依赖边（已全量核实） | P4.0 前置斩环（方案见 phase 表）；P0 import 图全量核对 |
| AppState 单例后门复活 | P4.0 grep 守门（`get_app_state` 调用数=0）；每领域 crate PR checklist 含此项 |
| db 变 god crate（平铺后 20k+ 行） | buzz 模式平铺模块天然分文件；单文件超 2k 行再拆子模块；db 只依赖 core 兜底方向 |
| 心跳双重迁徙 | 已排序：结构（P4）先行，语义（Trigger，TODOS P2）在后 |
| 其他分支 rebase | 渐进 P4 降低单次冲突面；agent crate 排在 thing-agent-loop 合并之后 |
| sqlx migrate / 测试 DB 路径 | P2 统一 migrations 归 db crate 或 apps/cloud，一次定清 |
| 中间 phase 集成断裂累积 | 已接受（E2E 仅 P5）；单 crate cargo test 逐包兜底 |

## 5. 不做的事（Out of scope）

- 不改动任何业务逻辑、API 行为、数据库 schema（纯结构重组）。
- 前端 web/ 不动。
- 心跳 runner → Trigger 框架的语义迁徙（TODOS 已有条目，重组后独立任务）。
- unwrap 治理、sqlx alpha 迁出（已记入 TODOS，与重组解耦）。
- mcp 架构的重新设计（P4.0 仅消灭单例，不动 MCP 协议层设计）。
- 领域 crate 内部模块的进一步整理。
- 编译时间基线测量、per-phase E2E（用户裁决接受）。
- agent crate ~19k 行偏大是已知取舍（loop/host/chat 子模块分离），后续需要再拆。

## 6. What already exists（复用清单）

| 已有 | 复用方式 |
|---|---|
| tinyiothub-storage（2.7k 行平铺 SQLite impl） | buzz 模式雏形，P2 平铺化的起点 |
| modules/agent → tinyiothub_ai 依赖 | 宿主-适配分层先例，证明方向可行 |
| HeartbeatTaskRepository trait 注入先例 | 保留在 AI 层内部（不作领域间模式） |
| modules/device/driver.rs re-export | 驱动框架已在 runtime，driver crate 只做管理面 |
| T19 E2E 验收脚本 | P5 唯一 E2E 门直接复用 |
| buzz-db（姐妹项目） | db 模式参考实现（平铺、无倒置、只依赖 core、真实 DB 测试） |
| SqliteDeadLetterQueue 已接线 | P2 搬迁 impl→db，无死代码问题 |

## 7. Failure modes（每个新路径一种现实故障）

| 故障模式 | 测试覆盖 | 错误处理 | 用户可见性 |
|---|---|---|---|
| include_str!/migrate! 路径断裂 | cargo check（编译期）✓ | — | 编译错误，明确 |
| fs 相对路径读 templates/ 运行时断裂 | **无 per-phase 覆盖（已接受）**，P5 E2E 兜底 | 运行时错误 | 功能缺失，E2E 可见 |
| AppState 单例残留调用 | P4.0 grep 守门 ✓ | — | 编译期可见 |
| 反向 import 残留 | P0 环图 + 每 crate cargo check ✓ | — | 编译错误，明确 |
| CI/发布管线断裂 | P5 release dry-run ✓ | — | CI 红，明确 |
| feature flag 矩阵丢失 | `cargo check --all-features` ✓ | — | 编译错误，明确 |
| 中间 phase 集成断裂（mock 掩盖） | **无（已接受，E2E 仅 P5）** | — | 延迟至 P5 暴露 |

## 8. Worktree 并行策略

| Step | 涉及模块 | 依赖 |
|---|---|---|
| P1 改名合并 | crates/* | — |
| P2 db/runtime/scheduler | crates/*, cloud/shared | P1 |
| P4.0 斩环 | cloud/modules/{mcp,thing,event,notification,agent} | P3 |
| P4 领域抽取 ×10 | cloud/modules/<domain>（各不相交） | P4.0 |
| P5+P6 CI/CD+文档 | .github/, Dockerfile, deploy/, docs | P4 |

- **Lane A（主线，串行）**：P1 → P2 → P3 → P3.5 → P4.0 → P4（逐领域）→ P5
- **Lane B（文档，可并行）**：AGENTS.md 守门条款、README 结构树（P6 内容可提前写）
- P4 各领域抽取目录不相交，但都改 cloud/Cargo.toml 与 shared/，**不建议并行 worktree**，
  串行周节奏即可；若加速，thing 试点后 auth + notify 可双 lane（依赖面无交集）。

## 9. Implementation Tasks

- [ ] **T1 (P1, CC: ~30min)** — P0 — cargo-modules 全量 import 环扫描，核对 3 组已知环 + mcp 边无遗漏
  - Surfaced by: 架构评审发现4/外部声音 OV-4/OV-7
  - Files: cloud/src/modules/**
  - Verify: 环图输出与文档 §1.2.3 一致
- [ ] **T2 (P1, CC: ~2h)** — P4.0a — 消灭 mcp::get_app_state() 单例，改 State 萃取
  - Surfaced by: 外部声音 OV-2（已核实 mcp/mod.rs static APP_STATE）
  - Files: cloud/src/modules/mcp/mod.rs, cloud/src/modules/thing/handler/actions.rs
  - Verify: `grep -rn "get_app_state" cloud/src` 调用数=0
- [ ] **T3 (P1, CC: ~1h)** — P4.0b — 斩 thing→agent/mcp 边（agent 侧反向提供 API）
  - Surfaced by: 外部声音 OV-1（actions.rs:18,118,240,264,285）
  - Files: cloud/src/modules/thing/handler/actions.rs, cloud/src/modules/agent/tools/
  - Verify: `grep -n "modules::agent\|modules::mcp" cloud/src/modules/thing/` 无命中
- [ ] **T4 (P1, CC: ~1h)** — P4.0c — 斩 event→notification 边（类型下沉 core）
  - Surfaced by: 外部声音 OV-4（event/mod.rs:94, service.rs:354）
  - Files: cloud/src/modules/event/mod.rs, cloud/src/modules/event/service.rs, crates/tinyiothub-core/
  - Verify: `grep -n "modules::notification" cloud/src/modules/event/` 无命中
- [ ] **T5 (P2, CC: ~10min)** — core 守门条款入 AGENTS.md
  - Surfaced by: 架构评审发现5
  - Files: AGENTS.md
  - Verify: 条款文本评审通过
- [ ] **T6 (P1, CC: ~1h)** — P5 CI/CD 路径迁移 + release dry-run
  - Surfaced by: 架构评审发现1（Step 0 分布检查）
  - Files: Dockerfile, Dockerfile.dev, .github/workflows/*, deploy/docker/*, scripts/build-static.sh
  - Verify: CI 绿 + dry-run 成功
- [ ] **T7 (P2, CC: ~15min)** — thing 试点落地 buzz 模式 db 平铺（thing.rs）+ FromRef 切片定型
  - Surfaced by: 架构评审发现2/发现3
  - Files: crates/db/src/thing.rs, apps/cloud/src/app_state.rs
  - Verify: 试点判据全绿（P4 行）
- [ ] **T8 (P3, CC: ~5min)** — 心跳 Trigger 迁徙 TODOS 条目补 "Depends on: 重组 P4"
  - Surfaced by: 架构评审发现4
  - Files: TODOS.md
  - Verify: 条目更新
- [ ] **T9 (P1, CC: ~30min)** — P1 同步更新 AGENTS.md（依赖表/稳定性层级/目录约定/守门条款）
  - Surfaced by: CEO 评审 D6（防 6 周文档撒谎期）
  - Files: AGENTS.md
  - Verify: 与 P1 改名结果一致
- [ ] **T10 (P1, CC: ~20min)** — P2 同步 Dockerfile migrations 行 + TODOS 安全锚点（#40/#41/#44）
  - Surfaced by: CEO 评审 D4/D5（Dockerfile:79-80、ci.yml:98-118 硬编码路径）
  - Files: Dockerfile, TODOS.md
  - Verify: docker build 绿 + 锚点指向新路径
- [ ] **T11 (P2, CC: 每领域 ~5min)** — P4 每领域抽取同步 ci.yml 守卫路径 + Dockerfile templates 行
  - Surfaced by: CEO 评审 D4
  - Files: .github/workflows/ci.yml, Dockerfile
  - Verify: 每领域 PR 的 CI 绿

## 10. CEO 评审补充（2026-08-03，HOLD SCOPE）

### Dream state delta

```
本方案落地后                          距 12 个月理想态还差
─────────────────────────────       ─────────────────────────────
21 crates，边界编译器强制             贡献者文档深化（plugin-sdk 指南、
双轨合一，单例后门消灭                architecture decision records）
AGENTS.md 与代码同步                 真正的协议驱动实现（Modbus/ONVIF/SNMP
开源卫生（LICENSE/诚实 README）   →   —— 目前只有 MQTT 和框架）
                                    CI 徽章/coverage 可见性
                                    第三方驱动市场的实际运转
```

结论：本方案是理想态的**必要非充分**条件 —— 它解锁"外部贡献者能看懂代码"，
但 plugin 生态需要真实驱动实现才有意义（README 已诚实标注开发中）。

### Error & Rescue Registry（迁移执行面）

| 代码路径 | 故障模式 | 救援 | 可见性 |
|---|---|---|---|
| P1 sed import 重写 | 批次部分应用 | 单批次单 commit，git checkout 恢复 | cargo check 红，明确 |
| P2 migrations 路径 | Dockerfile COPY 断裂 | T10 同步更新 + docker build 门 | 构建失败，明确 |
| P4.0 单例消灭 | 遗漏调用点 | grep 守门（get_app_state=0） | 编译错，明确 |
| P4 领域抽取 | 漏边/反向 import | P0 环图 + 复扫 | 编译错，明确 |
| P4 中间态行为微差 | mock 测试掩盖 | **无 per-phase 救援（已接受）**，P5 E2E 兜底 | 延迟至 P5 |
| 4 分支 rebase | 穿越冲突 | 作者各自承担（CEO 裁决不排空） | 明确 |
| P5 E2E 失败 | 集成断裂累积暴露 | 阻断合并，按 phase 二分定位 | 明确 |

CRITICAL GAP：无（唯一的无救援路径"中间态行为微差"为用户显式接受风险）。

### CEO 评审裁决记录

- 路径：B 全量拆分（否决 A 合并向下、C 基础设施先行）— 用户第三次确认范围
- 模式：HOLD SCOPE — 范围不再挑战，专攻执行面
- D3 分支不排空：作者各自 rebase；PR82 已合并，最大碰撞源消除
- D4 CI/CD 按 phase 分散（推翻工程评审的 P5 集中方案 —— 基于 Dockerfile/ci.yml 实证）
- D5 安全 TODOS 锚点 P2 同步（#40 保持 P0）
- D6 AGENTS.md 提前到 P1
- D7 本文档 §9 checkbox = 唯一进度 tracker，每周五更新

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAR | HOLD_SCOPE；路径 B 裁决；5 执行面发现全裁决（D3-D7） |
| Codex Review | codex CLI | Independent 2nd opinion | 1 | ERROR | CLI 运行时错误（RC=1），已回退 Claude subagent |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR | 9 issues 全部裁决（6 架构 + 2 测试 + 1 性能） |
| Design Review | — | UI/UX gaps | 0 | — | 无前端变更 |
| DX Review | — | Developer experience | 0 | — | 未运行 |

- **CROSS-MODEL:** 外部声音第一轮（Claude subagent）8 发现：5 属实（OV-1/2/4/7/8）、1 细节伪造（OV-6）、
  2 已被裁决覆盖（OV-3/OV-5）；3 个张力点全部用户裁决（P4.0 前置斩环 / 斩 event→notify 边 /
  维持 21 crate + 补判据）。第二轮（CEO 视角）用户跳过 —— 第一轮已抓出核心问题，边际收益递减。
- **VERDICT:** CEO + ENG CLEARED — ready to implement。已接受风险 3 项（路径资产靠编译+测试、
  E2E 仅 P5、不测编译基线）均为用户显式裁决并记录在 §3。下一步：P0（T1 cargo-modules 环扫描）。

NO UNRESOLVED DECISIONS
