# Crates 重组设计 — workspace 结构整改

> 日期： 2026-08-03 | 分支： 基于 main | 状态： 已评审定稿（待执行）
> 决策记录： 去全部前缀（含 ai- 前缀；契约 crate 命名 llm 而非 ai，单一职责）/
> 按领域拆细（auth/user/tenant/admin 等独立 crate）/
> error+config+core 合并 / agent loop+host 合并为单一 agent crate /
> memory crate 并入 memory / storage→db 改名 / stubs 移出 workspace /
> 重复归并：DLQ→runtime、设备/agent heartbeat 各归 driver/agent、cron 四处归并为独立 scheduler crate、
> plugin loader 并入 runtime（FFI 以 plugin-sdk 为单一事实源）、event 独立成 crate（斩断 alarm↔event 环）

## 1. 现状诊断

### 1.1 核心问题：两套架构并存

`crates/tinyiothub-*` 是按主流分层抽出的 library 层（AGENTS.md 声明的依赖方向：
core ← storage/runtime ← web ← cloud，设计本身合理），但 **85k 行业务代码仍全部
留在 `cloud/` 一个 crate 内**，cloud 内部又有一套平行实现：

| 领域 | crates/ 侧 | cloud/ 侧（平行实现） |
|---|---|---|
| 持久化 | `tinyiothub-storage` (2.7k 行， device/cron/notification 3 类 repo) | `shared/persistence/repositories/` (event/session/real_time_event/driver_installation…) |
| Agent/AI | `tinyiothub-ai` (13k 行， thing_agent/policy/orchestrator/skills) | `modules/agent/` (10.3k 行 host) + `shared/agent/` |
| Web 层 | `tinyiothub-web` (554 行， 自述 "placeholder") | `api/` + `server.rs` + `shared/middleware` |

`modules/agent` 已通过 `tinyiothub_ai` 依赖 crates/ai（宿主-适配器关系），说明
分层方向已被代码验证，缺的只是把 cloud 拆完。

### 1.2 具体问题

1. **god crate**：cloud 85k 行 = api 280 + modules 59k + shared 14.6k + tests 10.9k，
   lib+bin 同体（lib.rs 自述 "enables testing of internal modules" —— 正是该拆的信号）。
   30 个业务 modules，最大：agent 10.3k、alarm 6.1k、event 4.5k、thing 4.1k、mcp 3.7k。
2. **存储层分裂**：repository trait/impl 分散在 tinyiothub-storage 与 cloud/shared/persistence。
3. **占位成员污染 workspace**：plugins/* 7 个 1–4 行 cdylib 空壳；cli 10 行。
4. **目录分组不一致**：cloud/edge/marketplace/cli 与 crates/、plugins/、sdks/ 四种分组平级。
5. **命名维度混用**：按层（core/storage/web）、按领域（ai/memory）、按部署单元（cloud/edge）
   三种标准并存；workspace 内全是 path 依赖，`tinyiothub-` 前缀冗余。
6. **依赖方向可疑**：cloud → marketplace（部署单元互相依赖）；marketplace → tinyiothub-web(placeholder)。

## 2. 目标结构

```
crates/
  core/            # 合并现 error+config+core：类型、错误、配置、repository trait 契约
  plugin-sdk/      # 现 sdks/plugin-sdk：驱动作者的 SDK，drivers/* 编译期依赖
                   #   ABI 契约单一事实源（同化 tinyiothub-plugin/ffi.rs 的重复定义）
  db/              # 全部 repository SQLite 实现（现 storage 改名 + cloud/shared/persistence 迁入）
  runtime/         # 设备数据面：EventBus、DataServer、驱动框架、DLQ
                   #   （+ shared/event、mqtt_client、redis
                   #    + 并入 tinyiothub-plugin 的 loader/registry/sandbox 为 runtime::plugin）
  scheduler/       # 任务调度子系统：引擎(runtime/cron.rs) + 调度器(shared/cron_scheduler.rs)
                   #   + repo trait + service；持久化实现在 db，任务管理 API 在 admin
                   #   命名：子系统叫 scheduler（Quartz/APScheduler/JobScheduler 行业标准），
                   #   cron 只是触发器语法的一种，降级为内部类型名
                   #   （独立理由：多域消费的共享子系统，同 event；
                   #    且 tokio-cron-scheduler 重依赖隔离，edge 依赖 runtime 不必带上它）
  web/             # HTTP 基础设施：ApiResponseBuilder、middleware、extractors
                   #   （现 tinyiothub-web + shared/middleware + api_response + error_handling）
  # ── AI 层（现 tinyiothub-ai 13k 行 + tinyiothub-memory 436 行 → 4 个 crate）──
  llm/             # LLM 调用抽象（契约）：LlmProvider trait、LlmResponse/LlmCallMetadata、
                   #   prompt 构建、LLM session 类型（+ shared/llm_provider、ai_adapter）
  memory/          # agent 记忆与反思 + 知识图谱(KnowledgeGraph trait)
                   #   + SqliteAgentMemoryRepository（并入现 memory crate
                   #   + shared/workspace_memory.rs）
  policy/          # 策略引擎 + proposal 提案类型（1.1k 行，独立性强）
  skills/          # skills + tool 注册表（1.2k 行）
  #   注 1：thing_agent + orchestrator + heartbeat(agent 活性信号) + AiEvent 类型
  #         不独立成 crate，并入领域 crate agent 的 loop/ 子模块 —— 全 workspace
  #         只有 cloud 消费 AI，loop 的唯一消费者就是宿主，拆开只有命名混淆没有复用收益
  #   注 2：原 ai/alarm(26 行)告警类型归 alarm 领域 crate
  # ── 领域 crate（模型 + repository trait + service + http handler + router()）──
  auth/            # 认证、JWT、auth session、security（安全边界 = 编译边界，独立审计面）
  user/            # user + role + permission
  tenant/          # tenant + workspace（若与 user 出现循环引用，fallback 合并为 identity）
  thing/           # 物模型：modules/thing + template + tag + 旧 device 管理面
                   #   （CRUD/diagnostics/trace/monitoring/performance 为 legacy，
                   #    随 device→thing 迁移逐步归并/下线）
  driver/          # 数据接入：drivers 管理 + driver_health + gateway + plugin + 设备 heartbeat
                   #   职责一句话：给 thing 提供实时数据（依赖 thing，写入属性/事件）
                   #   注：驱动框架本体已在 runtime（modules/device/driver.rs 只是 re-export）
  event/           # 领域事件中心：modules/event（存储、查询、订阅、路由）
                   #   4 个域消费它（alarm/notify/agent/driver），必须独立；
                   #   传输层统一用 runtime EventBus，本 crate 只做领域语义
  alarm/           # modules/alarm（依赖 event；event/router.rs 中 alarm 相关路由
                   #   移入本 crate，斩断现存的 alarm↔event 循环引用）
  notify/          # modules/notification
  agent/           # agent 完整领域（~19k 行）：loop/(thing_agent+orchestrator+heartbeat)
                   #   + host/(现 modules/agent HTTP 宿主) + chat/ + shared/agent
  mcp/             # modules/mcp
  admin/           # modules/system + monitoring + batch + jobs + 调度任务 API（admin→scheduler）+ open
apps/
  cloud/           # 薄 bin：main.rs 读配置、建 AppState、组装各 domain router、启动
  edge/            # 现有，路径搬迁
  marketplace/     # 现有，路径搬迁；解除对 web placeholder 的依赖
  cli/             # 移出 workspace，实现后回归
drivers/           # 现 plugins/* 空壳，移出 workspace members，实现后回归
```

### 依赖方向（无环，单向）

```
core
 ↑        ↑
plugin-sdk  db ──────────────────┐
 ↑           ↑                   │
runtime ─────┘  （设备数据面：事件总线、DLQ、驱动框架、plugin loader）
 ↑                               │
scheduler（任务调度，引擎+调度器；repo 实现在 db）
 ↑                               │
llm（契约）──→ memory            │
   ↑  ↑                          │
   │  └── policy                 │
   │       ↑                     │
   └──── skills                  │
             ↑                   │
web ─────────┼───────────────────┤
 ↑           │                   │
领域 crate (auth/user/tenant/thing/driver/event/alarm/notify/agent/mcp/admin)
   各自依赖 core + db + web（+ runtime/llm 按需）；
   agent 领域 crate 额外依赖 policy/memory/skills（loop 为其内部子模块）；
   领域之间允许有限依赖：driver→thing（接入层向模型层写数据），
   alarm→event、notify→event、agent→event（事件中心为共享域服务），
   user→tenant（显式记录，出现环则 trait 下沉 core；
   user↔tenant 成环的 fallback 是合并为 identity）
 ↑
apps/*（只依赖领域 crate 和基础设施 crate，app 之间互不依赖）
```

**粒度红线**：单个 crate 不低于 ~400 行（纯契约 crate 除外）。原 ai 内 15–79 行的
小模块按职责归位：prompt/session → llm，knowledge → memory，proposal → policy，
alarm 类型 → alarm 领域 crate，不独立成 crate。

**重复归并规则**（单一职责，一处一概念）：
- DLQ 是通用基础设施 → runtime（非 AI 私有）
- heartbeat 两个概念：agent 活性信号 → agent/loop；设备心跳 → driver（数据接入面）
- cron 四处归并：引擎+调度器 → scheduler 独立 crate，任务 API → admin（admin→scheduler），
  持久化留 db（cron_job/cron_run 表名不动，纯结构重组不改 schema），
  shared 删除；modules/cron(6 行 re-export 壳) 消失
- session 三个概念各归各家：auth session → auth，chat session → agent/chat，ai session 类型 → ai

规则：db 实现各领域 crate 的 repository trait（db → 领域 crate 的
trait 定义）；app 之间禁止互相依赖（cloud 需要的 marketplace 逻辑下沉为领域 crate
或 client 模块，迁入 admin 或独立 marketplace-client —— 迁移时定）。

## 3. 迁移阶段（一次性重组，分 6 个有序 commit）

| Phase | 内容 | 验证 |
|---|---|---|
| P0 | 基线：CI 全绿、通知协作者冻结其他分支 | `just ci` |
| P1 | crates/* 改名去前缀；error+config+core 合并为 core；storage→db 改名；全局 import 重写 | `cargo check --workspace` |
| P2 | cloud/shared/persistence + runtime 相关（event/mqtt_client/redis）迁入 db/runtime；runtime/cron.rs + shared/cron_scheduler.rs → scheduler crate（引擎+调度器，repo trait 随迁，实现入 db）；tinyiothub-plugin 的 loader/registry/sandbox 并入 runtime::plugin，FFI 定义与 plugin-sdk 去重（以 plugin-sdk 为 ABI 单一事实源）；db 补齐全部 repository | `cargo test -p db -p scheduler` |
| P3 | shared/middleware、api_response、error_handling → crates/web；security → auth 预备；llm_provider/ai_adapter → llm；DLQ/事件总线 → runtime；shared 清空仅剩 app_state/service_manager | `cargo check -p tinyiothub-cloud` |
| P3.5 | AI 层归位：llm（LLM 契约）→ memory（并入现 memory crate + workspace_memory + knowledge）→ policy（+ proposal）→ skills；thing_agent/orchestrator/heartbeat/AiEvent 不拆，标记为待并入 agent 的 loop/ | `cargo test -p policy` 等逐包 |
| P4 | 领域 crate 抽取，顺序：thing（试点，建立模式；旧 device 管理面随迁）→ auth → user/tenant → event（事件中心先行；斩断 alarm↔event 环：event/router.rs 的 alarm 路由暂留 cloud，待 alarm 抽取时并入）→ alarm → driver（数据接入面，含设备 heartbeat）→ notify → agent（loop + host + chat 三合一大成）→ mcp → admin（含调度任务 API）。每个 crate 暴露 `router()` + `Service`；tests 随迁 | 每个 crate `cargo test -p <domain>` |
| P5 | apps/ 归位；cloud main.rs 变薄（<200 行）；解除 cloud→marketplace 依赖；plugins/cli 移出 workspace members | `just ci` + docker build |
| P6 | 文档：AGENTS.md（目录约定、依赖方向表、稳定性层级全部更新）、CLAUDE.md hot paths | review |

P4 是工作量的主体。每个领域 crate 的抽取模式（试点在 thing 上定型）：

1. `git mv` 保留历史；2. repository trait 上移到该 crate（或 core）；
3. handler 依赖的 AppState 改为显式 `State<DomainState>` 或 trait 注入；
4. 跨模块引用改走领域 crate 的 pub API；5. 该 crate 独立测试通过。

## 4. 风险与对策

| 风险 | 对策 |
|---|---|
| 领域间循环引用（已确认一个：alarm↔event，event/router.rs 反向引用 alarm；其余如 driver→thing 待查） | 已知环按预定方案斩断（alarm 路由并入 alarm crate，单向 alarm→event）；P0 用 `cargo-modules`/脚本生成 modules 间 import 图找出其余环，环处用 trait 下沉 core 解耦 |
| AppState god object 耦合所有 handler | 各领域 crate 定义自己的 State 切片；apps/cloud 组装 |
| import 重写量大（数万处 use） | 按 crate 批次用 sed/rust-analyzer rename；每批 `cargo check` 守门 |
| 其他分支全部需要 rebase | 选低活动窗口执行；P1–P5 快速连续完成（目标 ≤3 天） |
| sqlx migrate / 测试 DB 路径 | 迁移时统一 `cloud/migrations` → db crate 或保留 apps/cloud，一次定清 |

## 5. 不做的事（Out of scope）

- 不改动任何业务逻辑、API 行为、数据库 schema（纯结构重组）。
- 前端 web/ 不动。
- 领域 crate 内部模块的进一步整理（留给后续按需）。
- 原 ai 内 15–79 行小模块不独立成 crate，按职责归位（prompt/session→llm、
  knowledge→memory、proposal→policy、alarm 类型→alarm）。
  粒度红线：单 crate ≥ ~400 行，纯契约束除外。
- agent crate ~19k 行偏大是已知取舍（loop+host+chat 三合一），
  内部 loop/host/chat 子模块保持职责分离；后续若需要再拆不迟。
