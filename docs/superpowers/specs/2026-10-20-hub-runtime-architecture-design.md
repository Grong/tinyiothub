# Hub 运行时架构设计（对标 AGH 的全平台演进）

- 日期：2026-10-20
- 分支：ai-consolidation
- 状态：原则/目标分层/扩展点/演进阶段四节已与作者逐节确认（2026-10-20 会话）；裁定表 15 项已确认
- 对标对象：[AgnesAI-Labs/agnes-harness](https://github.com/AgnesAI-Labs/agnes-harness)，main 分支 2026-10-20 快照（pre-alpha，Apache-2.0）
- 定位：**架构设计**，非功能设计。回答"我们的 AI+设备平台应该长成什么样"；易扩展性与可演进性是一等约束。对标事实与裁定降附录（§7/§8），是本文的决策依据。

## 1. 背景与对标方法

动机：AGH 是面向多设备协同的 agent harness（"LLM 是脑、Harness 是记忆、MHS 是身体"），其 mars-world 示例展示了 11 台虚拟设备的三条多设备协同链（侦察采样/风暴收拢/救援），与 tinyiothub 的园区多设备目标同向。

方法：两份调查报告交叉——
- AGH 侧：README、`docs/develop/{architecture,source-map}.md`、`docs/guide/{mhs,sessions,skills,security,packages,limitations}.md`、`packages/mhs/{README,spec/{mhs-spec,mos-spec,mhs-conformance},server/hub-api}.md`、`packages/core/src`（step/session.ts、op-state.ts、resume.ts、log/integrity.ts）、`packages/protocol/schema/session-v1.json`、`examples/mars-world/README.md`。
- 我方侧：crates/{agent,llm,runtime,scheduler,policy,skills,memory,plugin-sdk,core} 源码普查 + apps/cloud AI 域 + web/src/ui AI 视图 + TODOS.md + ai-consolidation spec。

## 2. 差距的本质

**AGH 把 Hub 当"运行时"，我们把 Hub 当"数据库+轮询器"。**

AGH 的 AgnesHub 内存中活着每设备的实时状态、资源占用、job 进展、watch 条件、健康/位置派生摘要；brain 不轮询——它在固定规则决定的时刻拿快照（读设备/工具返回/turn 开头/被唤醒），其余时间由事件唤醒（hub-api.md §15："brain 看到多少取决于发生多少，而不是数据多快"）。

我们的 DataServer 同样有 device_cache 与 EventBus，但 AI 侧的认识方式是周期心跳轮询 + 主动 `get_thing_profile` 逐个查。设备对 AI 而言不是"活着会说话的实体"，而是"等被查询的行"。这个哲学差异派生出附录 §7 中大多数具体差距。本架构设计的目标就是完成这个范式转换。

## 3. 架构原则（P1-P6）

| # | 原则 | 含义 | 反模式（现状） |
|---|---|---|---|
| P1 | **Hub 是运行时，不是数据库** | 设备实时态、watch 条件、资源占用、派生摘要活在运行时内存；DB 是账本/报告，不是实时真相源 | AI 靠 `get_thing_profile` 逐行查库认识设备 |
| P2 | **事件驱动唤醒，不是轮询** | brain 由事件/条件命中唤醒 + turn 开头快照注入；watch 是确定性规则，不跑模型 | 只有周期心跳一种唤醒 |
| P3 | **能力声明制** | 设备/插件/工具注册即声明能力，运行时按声明校验+钳制；能力变化=重新声明，而非改代码 | 模板一次性拷贝，运行时无校验 |
| P4 | **关键决策点皆是接缝（seam）** | 预算/审批/门控/调度/触发都是封闭契约的可替换段——override 不 fork 核心（借鉴 AGH ReplacementOperation）。**今天换不掉的部分就是明天演进的障碍** | 改 loop 行为只能改源码 |
| P5 | **契约为演进而版本化** | 所有跨层契约（设备协议/工具 schema/事件信封/prompt 段）带显式版本；新旧版本协商共存（按能力交集工作）；任何组件可独立于其消费者升级 | 工具 id 两处手工对齐，无单一事实源 |
| P6 | **绞杀者演进（strangler）** | 新架构从新链路长起，逐步接管；旧路径自然萎缩退役，不做一次性大迁移——每阶段独立交付、独立回滚 | —— |

P4+P5+P6 合起来是"架构可以演进"的具体机制：**接缝让替换有边界，版本化契约让替换有协商，绞杀者让替换有节奏**。例外（R1-2-4 改写）：接入层（§4 第四层）**架构上按最优解设计、不为存量驱动在协议内留共存路径**；但 E4 退出必须含存量驱动的迁移计划（迁移是执行问题，协议不背兼容包袱，也不把存量驱动永远留在旧路径上）。

## 4. 目标架构

```
┌─ 呈现层 ── Devices 工作台(设备卡/Flow/World) + AI 中心(#/ai) + chat/A2UI
│            全部是 Hub 运行时的投影(projection)，不直接查库
├─ Brain 层 ── Run Loop 内核 + 可替换接缝(Trigger/Budget/Approval/Policy/ToolExec)
│             固定设备工具集 + turn 开头快照注入 + 事件唤醒通道
├─ Hub 运行时层 ── LiveState(实时态+health/zone 派生摘要)
│             Watch 引擎(确定性条件,注册表式词表,one-shot,唤醒会话)
│             Job 仲裁(资源占用,busy 带持有者)
│             双通道事件:有序不丢(控制/事件) + latest-wins(遥测流)
│             ★新核心；设备对 brain 的唯一入口，模型永不直连设备
├─ 接入层 ── 声明式注册(identity+tools+state fields+sources+resources+placement)
│            Hub 按声明校验+钳制每次调用(越界 Hub 侧拦截)
│            需求驱动采样(无订阅者/无 watch 的源不采数)
│            一致性验收(pass/fail/warn/skip/manual 五级 profile 化)
├─ 治理层 ── 包信任 / Skills 治理 / 审计账本——各自独立显式边界
│            架构留位，实现等触发条件(§6 E5)
└─ 持久层 ── 事件账本(append-only) + 报告
            实时态不落库；落库的是事件/报告/账本
```

**账本捕获策略**（R1-1-2 补，X6 时间机器的硬前置）：状态变更事件全量落账本；遥测原始流走 latest-wins 可丢，但**派生摘要变化与 watch 越限/命中事件必落**；job 生命周期（accepted/progress/result/rejected）全量落。判定尺：账本足以回放重建任一历史时刻的"设备状态+进行中的协同链"——回放精度=状态变更粒度，遥测毛刺不保证回放。

**多租户隔离**（R1-1-4）：LiveState/Watch/Job 仲裁均以 workspace_id 为隔离边界；跨工作区引用（watch 他区设备、composite 跨区调用）一律拒绝。附录 C 的"每 caller 上限"中 caller=工作区内的会话。

### 各层职责与边界

- **呈现层**：只做投影与交互。任何设备实时信息来自 Hub API 订阅，不直接查 DB。工作台三 tab 对应三种投影：设备卡=LiveState 投影，Flow=watch/job 事件链投影，World=空间语义投影。
- **Brain 层**：Run Loop 内核只管推进；Trigger/Budget/Approval/Policy/ToolExec 五个接缝有封闭输入输出契约，可整体替换（替换器拿 `next()` 内建实现，委托须原样返回——AGH 同款规则）。固定设备工具集（list/read/call/set/stop/watch/unwatch 形态）——设备自有工具是 `call` 的参数，**工具列表不随设备增减变化**。
- **Hub 运行时层**：本架构的新核心。LiveState 以 EventBus 为输入、替代 device_cache 成为 **AI 侧唯一读面**（R1-3-2；device_cache 仍是驱动侧写面，cache-before-publish 不变量保证订阅者读到新值），持有每设备最新状态并派生 health（ok/attention/bad+原因，worst-first）与空间归属（zone 推算）；Watch 引擎挂确定性条件（不跑模型，需要理解的条件由数据源陈述）；Job 仲裁按设备声明的 resources 做占用互斥，拒绝文案带持有者——**E1 阶段的退化形态：每设备默认单一互斥资源，E4 声明制接入后细化为多资源**（R1-2-2）；事件双通道分离保证"视频突发永远不能延迟 stop"。**重启语义**（R1-1-3）：watch 注册表=易失投影；GoalWatch 目标表=持久物；启动时由目标表重织 watch 网（**X7 的规则引用解析复用此重织逻辑**——R2-3-2，E1c 先建）——"场景包装完就开始值守"跨发版重启成立。**会话临时 watch（E2 的 watch 工具注册）不持久**，重启/超时即失效；需长期值守者必须落目标表（R2-1-2）。
- **接入层**：注册即声明。Hub 校验+钳制使坏驱动伤害受限；需求驱动采样使十传感无观众设备近似零开销；一致性验收使接入质量可度量。
- **治理层**：包信任/工具审批/执行约束是三个不同边界，各自显式（P6 边界显式化）。哈希链账本、双哈希包信任架构留位不建——触发条件见 §6 E5。
- **持久层**：事件账本为骨架（append-only），为未来恢复语义/审计/跨端 resume 留地基；**现在只落事件与报告，不建哈希链**（原裁定 #11/#12）。

## 5. 扩展点设计

架构的可演进性最终体现在：**未来加新东西时，改的是数据/注册，不是核心代码**。

| 扩展点 | 挂载方式 | 未来场景 |
|---|---|---|
| 新触发源 | 实现 Trigger 接口发 WakeSignal（现有接口升格为一等扩展点） | TrendAnomalyTrigger；GoalTrigger 进化为 watch 引擎的内部触发面 |
| 新 watch 条件类型 | 条件词表是**注册表**非 enum 硬编码：新类型=注册一个比较器（纯函数：声明+事件流→bool） | detections 标签、表达式条件、多设备联合条件（"A 且 B"） |
| 新数据源 kind | MOS 式词表 + `x_<name>` 自定义逃生舱；Hub 对未知 kind 透传不拒 | 视频、热成像、声波检测接入零 Hub 变更 |
| 新设备能力 | 声明制天然支持：设备声明即存在；Hub 不认识的字段忽略（MHS MSG-6 规则） | 新设备类型零 Hub 变更接入 |
| 新 brain 工具 | 工具注册表 + catalog 分组单一事实源（顺手消现有动态/静态分组不一致债） | composite 工具（跨设备编排原语） |
| 新工作台面板 | Hub API 客户端 + 投影订阅；呈现层不动 Hub | 能耗面板、安防面板 |
| 新审批/门控策略 | P4 接缝：Policy seam 整体替换，封闭契约 | 企业 SSO 审批流、按设备类型差异化门控 |
| 新存储 | 账本/报告的 StorageAdapter 接口 | SQLite→PG、冷热分层 |

**版本协商机制（P5 落地）**：每条跨层契约（设备注册协议/工具 schema/事件信封/prompt 段格式）带 `version` 字段；消费者声明理解的版本集，生产者按交集产出；不认识的版本/字段=忽略不炸。任何一层换代时其它层不被迫同步。

## 6. 演进阶段（E1-E5）

每阶段独立交付、独立回滚（P6）。CEO 评审（2026-10-20，SCOPE EXPANSION）后 X1/X2/X3/X4/X6/X7 六项扩张并入，X5 云边两级推迟（附录 B #22）。

| 阶段 | 交付的架构能力 | 内容 | 退出标准 |
|---|---|---|---|
| **E1 值守中枢**（原 Hub 运行时内核，扩张后） | P1/P2 地基 + 值守产品形态 | 内部子阶段：**E1a** 引擎内核（LiveState 实时态 + Watch 引擎注册表式条件词表 + one-shot 唤醒通道 + 双通道事件语义）；**E1b** 派生摘要（health + **zone 空间归属**（X4）+ 预算化快照的数据源）；**E1c** **GoalWatch 目标维持**（X2：目标表/达成判定/长期记忆/周期报告，GoalTrigger 正式落地；**目标→watch 的编译发生在设目标时刻（可用模型），watch 运行期为纯比较器**（R1-3-3），与 P2 自洽）；**E1d** **知觉权**（X1：brain 直接消费遥测流，发现规则未覆盖异常；**架构位置=TrendAnomalyTrigger 式 Trigger 接缝实现，输出 WakeSignal，配额由 Budget 接缝钳制**（R1-2-3；**E1 阶段退化形态：配置化 token 上限挂在现有 runner 硬预算（runner.rs:25-28）旁，E2 接缝化后迁入 Budget 接缝**——R2-2-2）；范围锁定「单类异常 + 成本闸门」，不建通用异常检测平台） | E1a-c：设目标 → watch 网自动织出 → 越限唤醒 run → run 读到含空间归属的摘要 → 周期报告；且**账本足以回放该场景全程**（R1-1-2）；**E1a 另验：两 run 并发操作同设备时后者收到带持有者的 busy 拒绝；E1c 另验：重启后 watch 网由目标表重织、值守无需人工介入继续**（R2-1-1）；**E1d：一个遥测异常被 brain 端到端发现，且该子阶段 token 成本有硬顶并实测不超**（R1-1-1） |
| **E2 Brain 接缝层** | P4 接缝化 | Run Loop 五接缝封闭契约；固定设备工具补齐（watch/stop）；turn 开头快照注入（预算化，AGH ≤6KB 参照）；审批原因六分类（unavailable≠rejected）；**composite 编排原语框架**（X3：多设备动作链声明为可复用工具，失败/部分成功/回滚语义以 E1 真实协同场景为设计输入）。**现有 9 工具→目标态映射**（R1-3-1）：设备类 list_things/get_thing/get_thing_profile/read_property/invoke_action 折叠进 list_devices/read_device/call_device/set_device 形态；非设备类 query_events/search_knowledge/read_document/get_thing_tree 保留为独立类别（知识/文档/事件工具），不进设备工具集 | 接缝各有一个真实替换实现（证明可换）；快照注入上线且 AI 不再逐物查 profile；一条 composite 链端到端可审批可溯源 |
| **E3 呈现层** | 投影化 | Devices 工作台三 tab（设备卡/Flow/World，前端只订阅 Hub 投影）；**Flow 时间机器**（X6：拖时间轴回看任意时刻设备状态+事件链，依赖持久层事件账本回放能力；**回放精度=状态变更粒度，遥测毛刺不保证回放**，见 §4 账本捕获策略——R2-3-1）；**watch/协同规则模板库**（X7：场景包携带 watch 条件与目标模板，实例化含规则引用解析与校验） | 多设备协同链（watch→job→结果）在 Flow 中可见可溯可回放；装一个场景包即自带值守规则 |
| **E4 接入层声明制** | P3 最优解 | 驱动声明式注册（目标态直连，协议内不留共存包袱）；Hub 校验钳制；需求驱动采样；一致性验收雏形 | 一个新驱动纯声明接入（零 Hub 代码变更）并通过验收 profile；**并附存量驱动迁移计划（迁移执行可另立阶段）**（R2-2-1，与 §3 例外条款对齐） |
| **E5 治理层** | 信任边界显式化 | 包/Skills 治理、审计账本 | **架构已留位，启动等触发条件** |

**顺序逻辑**：E1 立起"Hub 是运行时"（watch+摘要是一切协同的地基）→ E2 让 brain 通过接缝消费运行时 → E3 把运行时投影给人看（Flow 的数据正是 E1 的 watch/job 事件链）→ E4 动接入协议（运行时价值已验证，改造有底气）→ E5 等真实需求触发。

**E5 启动触发条件**：出现审计合规需求（哈希链账本）；run 中途崩溃造成实际事故（恢复语义）；OpenClaw 接入立项（跨端 session resume）；第三方驱动生态 ≥5 个（包治理）；引入移动机器人设备类型（运动安全/手动接管）。

**X5 云边两级 Hub（推迟的方向预留）**：edge 节点作为 Hub 运行时的下游（断网本地值守、复连同步）是已确认的远期方向（CEO 评审 D2.5 Defer）。架构预留=P5 版本化契约 + 分层边界（Hub 运行时不假设单进程）；启动条件=edge 运行时自身成熟（do_scan/驱动运行时落地，TODOS P2）+ E1-E4 就位。

**与 ai-consolidation 的关系**：其统一 Run Loop = E2 的 Run 内核前身；概念模型（Trigger/Run/Verdict/Disposition）与本文档完全兼容；E2 接缝化是其自然续集，不冲突不返工。

## 7. 附录 A：逐维度对标事实（决策依据）

### 7.1 Agent Loop 恢复语义（差距最大）

| | AGH | 我们 |
|---|---|---|
| 执行状态 | **OpState 程序计数器**（phase: checkpoint/inference/tools/compaction/deferred/failure_drain），与事件批**同事务 CAS 提交**——"崩溃没有 between"（session.ts；session-v1.json） | runner 内存态（RunContextInner，`crates/agent/src/runtime/thing_agent/runner.rs:64-84`）+ 完成时落 RunReport；run 中途崩溃即丢失 |
| 取消 | 取消是账本已决事实，任何 phase 看到 cancel 记录后不得再发模型请求 | CancellationToken best-effort + 30s 兜底（runner.rs:29） |
| 恢复 | 每个未完成 effect 逐个裁定 rerun/rerun-idempotent/retry/poll/settle；嵌套调用图按 parentEffectId 遍历（step/resume.ts） | 无 run 级恢复概念 |
| 活性 | 相位边不推进记 `run-no-progress`；phase 抛错记诊断并结束 turn | 连败 5 次暂停（`governor.rs`），粒度是 run 不是相位 |
| 可替换段 | **ReplacementOperation**：Inbox/Budget/Inference/Approval/ToolExecution/StopGate 六段可整体替换（封闭契约，不 fork core 即可 override 关键决策） | 无对应物——本架构 P4 即为此立 |

### 7.2 事件账本（审计级 vs 记录级）

AGH：append-only 事件链（31 种内建事件类型），每条带 `origin`（principal/model/system/tool:*…）与 **`trust` 污点标注**（不可信内容进过上下文沿账本传播）；JCS（RFC 8785 风格）+ SHA-256 **哈希链**，verified-head 增量校验，完整性失败时一切请求拒 `E_LEDGER_INTEGRITY`（log/integrity.ts、sessions.md）。

我们：`agent_runs` 表 + ToolTraceEntry 轨迹 + RunRecorded 重试/DLQ。留痕够用，无篡改检测、无事件级信任标注。

### 7.3 设备抽象（声明制 vs 模板拷贝制）

AGH：设备连接时**自我声明** tools（参数 schema/timeout/motion 标记）/ state fields（含 alert 阈值）/ data sources / resources / placement（MHS §5 `mhs/register`）；Hub 按声明**校验+钳制**每次调用，参数越界在 Hub 侧拦住（MHS §6.4）。

我们：能力来自 ThingTemplate **一次性快照拷贝**（`apps/cloud/src/domains/marketplace/scene_instantiator.rs`），实例化后脱钩；`invoke_action` 直接透传驱动（`apps/cloud/src/domains/agent/host/tools/thing/invoke_action.rs:40-44`），无运行时声明校验层。

### 7.4 Brain 与设备的接口形态

| | AGH | 我们 |
|---|---|---|
| 工具集 | **固定 7 个**（list_devices/read_device/call_device/set_device/stop_device/watch_device/unwatch_device）；设备自有工具是 `call_device` 的参数，**工具列表不随设备增减变化**（hub-api.md §15.1） | 9 个 thing 工具 + MCP 设备工具（`apps/cloud/src/domains/agent/host/tools/thing/mod.rs`）；`invoke_action`≈`call_device`；无 watch/stop |
| 上下文 | 每 turn 开头注入 ≤6KB 快照：地图+places 在前，然后每设备健康/位置/在干什么（hub-api.md §15.2 context hook） | 四段式 prompt（`crates/agent/src/prompt/runtime.rs`），设备信息靠 AI 主动查 |
| 读设备 | `read_device` 不带 sources 只回 state+health+position 摘要 + 一句写给模型的 `text` | `read_property` 逐属性读，无摘要层 |
| 唤醒 | job 结束/watch 命中/estop/人接管 → 唤醒对应会话（1s 内合并）（hub-api.md §15.3） | 无；run 结束即结束 |

### 7.5 条件等待 watch（我们完全缺失的原语）

AGH：**确定性比较**（state 字段 lt/gt/eq/ne；source 字段阈值；detections 出现某标签；health/trust 变化），one-shot，每 caller 上限 20，超时上限 3600s，命中唤醒对应会话。明确"watch 里不跑模型，需要理解的条件由 source 陈述"（hub-api.md §12）。

我们：TODOS 中的 **GoalTrigger**（"车间温度维持 20-26°C"式持续目标，P3）本质就是它，未做；Trigger 接口已预留（`crates/agent/src/runtime/thing_agent/trigger/mod.rs:18-24`）。这是多设备协同最关键的缺失——mars-world 的"风暴预警→收太阳能板→降落 hopper→rover 回坞"链式协同靠 watch+唤醒实现，我们只能等下一个心跳 tick。

### 7.6 仲裁与人机优先级（完全缺失）

AGH：caller 分级（level 0 stop=任何人 / 1=ui 人 / 2=agent）；设备声明 **resources**，job 占用资源，`busy` 拒绝带持有者名（"held by drive_to j9 (web)"）；人接管立即打断 AI 的 motion job，松手后 3s 内 AI motion 调用被拒；`hub/stop` 永远可用（hub-api.md §13）。

我们：`invoke_action` 无占用、无仲裁；两个 run 同时操作同一设备无互斥。

### 7.7 空间模型（有骨架无语义）

AGH：map = 命名 2D 帧（`{id, name, bounds, anchor}`，anchor 支持挂到更大帧或地球锚定）；place = landmark（点+朝向）或 zone（≥3 点多边形）；设备声明地图，Hub 合并保留（断连不丢）；位置 trust 三态（trusted/uncertain/lost）+ 所在 zone 自动推算（MOS §3.5、hub-api.md §4.4/§6.2）。

我们：`parent_id` 层级树（园区→建筑→楼层，`crates/core/src/models/thing.rs:85-116`）+ `position` 字段 + 场景包模板。层级语义清晰，但无坐标帧/区域/朝向概念，AI 无法回答"哪些设备在 X 区域"类空间问题。

### 7.8 审批语义（各有胜负）

AGH 更细：拒绝原因六分类（含 **unavailable ≠ rejected**——"没人可问"不算拒绝且不执行）；会话级 grant（同名工具后续调用免批）；审批卡截断展示规则；"审批不免除未来 turn 的验证"（security.md）。

我们更细：工作区信任三档 + AutonomyPolicy（allow/deny 清单 + 速率熔断，fail-closed，kill-switch 每动作新鲜读档，`autonomous_invoke.rs:278-280`）。AGH 审批主要保护命令执行，我们门控覆盖全部 run。

### 7.9 插件/包治理（代差）

AGH：信任绑定**内容哈希+能力哈希双值**（`package trust <id> <INTEGRITY> <CAPABILITY_HASH>`），能力声明变化必须重新信任；desired/actual 双状态；安装预览强制展示 integrity/warnings/blocker；回滚保留有界（packages.md、plugins.md）。

我们：plugin-sdk 的 FFI ABI 契约在 `crates/plugin-sdk/src/macros.rs:53-76`（`extern "C"` 导出）与 `ffi.rs`，`DRIVER_ABI_VERSION = 1` 定义于 `crates/core/src/driver/dynamic.rs:6`；`drivers/` 不在 workspace 编译门禁内（TODOS P2）。无哈希、无能力声明、无生命周期治理。

### 7.10 一致性验收（我们为零）

AGH `mhs-check`：单一 JSON Schema（gen:check 防类型漂移）+ 测试 hub + 日志校验器三层；10 个 profile（core/motion/manual/pause/perception/maps/derived/streaming/video/clip）；五级结果（pass/fail/warn/skip/manual）；物理行为用 odometry 自动判定或 `--interactive` 问操作员；`--no-motion` 记 skip 不记 pass。

我们：设备接入验收 = 连上看有没有数据。

### 7.11 会话共享与恢复

AGH：CLI/Web/SDK 同一后端 session，`/resume <id>` 跨端续接；恢复用**当前**权限（过去授权不复活）；导出/导入带附件哈希校验；LEDGER_FORMAT_VERSION 版本门。

我们：自研 chat（`apps/cloud/src/domains/agent/host/chat/`），SSE 直转，session_key fail-closed 校验；OpenClaw 接入仅在计划文档中；已知缺口：chat 无 per-session 广播（TODOS P2）。

### 7.12 我们领先的方面（如实记录）

- **judge/policy 结构化决策已在线**（`crates/policy` gate_check）——AGH 的 Jev 仍在"集成中"
- **信任三档 + kill-switch** 语义完整度更高
- **AI 中心 UX**（ai-consolidation 的 Run 流/需要你/今日自述）比 AGH Trace 视图更面向运维
- **A2UI + Scene3D**：AI 主动生成界面，AGH 无此能力
- **场景包模板实例化**（智能园区一键复制）AGH 无对应物
- **派生表达式引擎**（`crates/runtime/src/expression.rs` 跨物计算）AGH 无对应物

### 7.13 AGH 自认的空白（不必抄的部分）

pre-alpha 无发布承诺；公网部署未验证；进程内插件无隔离；无硬件认证；外部副作用无 exactly-once；Windows 支持不全（limitations.md）。**这些恰是 IoT 平台必须自己补的**。

## 8. 附录 B：裁定记录（作者已确认，2026-10-20）

| # | AGH 机制 | 裁定 | 架构归宿 |
|---|---|---|---|
| 1 | watch 条件唤醒 | **采纳，最高优先级** | E1 Watch 引擎 |
| 2 | turn 开头设备/健康快照注入 | **采纳** | E1 LiveState 派生 + E2 快照注入 |
| 3 | 空间语义（zone/坐标/朝向） | **改造采纳**（不搬 maps 帧概念，叠加现有层级树） | ~~E3 Hub 派生能力~~ **已被 #19（X4）前移至 E1b** |
| 4 | Devices 工作台（设备卡+Flow+2D 地图） | **采纳，依赖 E1** | E3 呈现层 |
| 5 | 审批原因分类（unavailable≠rejected） | **采纳，小项** | E2 Approval 接缝 |
| 6 | 资源占用/仲裁（busy 带持有者、同设备互斥） | **改造采纳**（人接管打断不做——楼宇固定设备弱需求） | E1 Job 仲裁 + E4 resources 声明 |
| 7 | 固定 7 工具形态 | **已对齐** | E2 补 watch/stop 两工具 |
| 8 | 设备能力运行时声明+Hub 校验钳制 | **采纳，远期** | E4 接入层 |
| 9 | 一致性验收套件 | **采纳，远期** | E4 验收框架 |
| 10 | 需求驱动采样 | **采纳（架构目标态）** | E4 接入层 |
| 11 | OpState 程序计数器+同事务相位边 | **不采纳（现阶段）**，持久层留事件账本地基 | 留位，触发条件 §6 E5 |
| 12 | 哈希链账本 | **不采纳（现阶段）** | 治理层留位 |
| 13 | 插件双哈希信任治理 | **不采纳（现阶段）** | 治理层留位 |
| 14 | 跨端共享 session resume | **记录** | 与 OpenClaw 接入决策绑定 |
| 15 | 手动接管/运动安全（estop/deadman/断连自停） | **不采纳** | 楼宇固定设备为主；引入移动机器人时重评 |

### CEO 评审扩张裁定（2026-10-20，SCOPE EXPANSION，D2.1-D2.7 + D2.final 确认）

| # | 扩张项 | 裁定 | 归宿 |
|---|---|---|---|
| 16 | X1 知觉权（brain 直接消费遥测流发现异常） | **Include** | E1d 子阶段 |
| 17 | X2 GoalWatch 目标维持（GoalTrigger 落地） | **Include** | E1c 子阶段 |
| 18 | X3 composite 跨设备编排原语 | **Include** | E2 框架 |
| 19 | X4 空间语义前移（zone 派生） | **Include** | E1b 子阶段（原 E3） |
| 20 | X6 Flow 时间机器（账本回放） | **Include** | E3 |
| 21 | X7 watch/协同规则模板库 | **Include** | E3（E1 引擎之后） |
| 22 | X5 云边两级 Hub | **Defer** | 方向预留，启动条件见 §6 |

## 9. 附录 C：AGH 值得记住的协议细节（E1-E4 实施时回看）

**规范性地位**（R1-1-5）：以下参数为**默认采纳的要求**——实施时默认沿用 AGH 值，任何偏差需在对应阶段的 spec/plan 中显式裁定并记录理由。

- watch 条件词表（hub-api.md §12）：state 比较 / source 字段阈值 / detections 标签 / health/trust 变化；one-shot；only new data counts；每 caller 上限 20，超时 ≤3600s。
- 唤醒合并：同会话 1s 内事件合并为一条（§15.3）。
- 快照预算：≤6000 bytes，地图占一半以内，放不下的 places 只计数（§15.2）。
- 健康派生规则（§6.1）：alert 阈值/problem+faults/源流速不足/连接 RTT 四类原因，worst-first。
- 位置 trust 三态判定（§6.2）：trusted/uncertain/lost + age；fixed 设备永远 trusted。
- busy 拒绝文案带持有者（"held by drive_to j9 (web)"）——人读得懂的拒绝。
- MOS derived 源对齐规则：`of_seq` + `t` 等于输入采集时间；pose 失锁必须 `ok:false` 且继续按速率发——"沉默表示定位器停了而非丢了"。
- mhs-check 设计原则：schema 严格处严格开放处开放；跨字段规则不进 schema 进 checker；`--no-motion` 记 skip 不记 pass。

## 10. 参考索引

- AGH 仓库：https://github.com/AgnesAI-Labs/agnes-harness
- 关键文档：docs/develop/architecture.md（请求路径与状态归属）、packages/mhs/server/hub-api.md（brain 接口全集）、packages/mhs/spec/mhs-spec.md（设备侧协议）、docs/guide/security.md（审批/信任/沙箱）、docs/guide/sessions.md（恢复语义）、docs/guide/packages.md（包治理）、examples/mars-world/README.md（多设备协同示例）
- 我方关键代码：`crates/agent/src/runtime/thing_agent/`（loop 本体）、`crates/runtime/src/data_server.rs`（设备运行时）、`apps/cloud/src/domains/agent/host/tools/thing/`（AI 工具集）、`crates/policy/src/autonomy.rs`（自治门控）
- 相关 spec：2026-10-12-ai-consolidation-design.md（概念模型与 Run 归一）、2026-07-06-ai-harness-loop-design.md、2026-10-08-ai-run-transparency-design.md、2026-08-31-scene-template-design.md、2026-05-28-scene3d-a2ui-design.md
