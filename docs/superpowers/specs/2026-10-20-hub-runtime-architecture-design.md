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
│            E4 实交付：独立 driver.manifest.json + 双层实例声明(describe)
│            Hub 按声明校验：业务参数拒绝、超时钳制、未声明工具拒绝
│            采样：默认全采，仅 demandOnly 源门控；health 慢通道常开
│            验收：自动 profile 全套(pass/fail/warn/skip；manual 未做)
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
| **E4 接入层声明制**（已交付 E4a-E4e） | P3 最优解 | 独立 manifest + ABI v2（async/cancel/describe，v1 硬切）+ 双层声明；Hub 校验/超时钳制；保守需求驱动采样；多资源仲裁 + stop 在途取消；maps/place/zone 空间模型；自动验收 profile 全套 + inventory + 迁移计划 | demo-declarative 纯声明接入（Hub 零代码）并通过全部自动 profile；v1 `.so` 拒载；stop ≤1s cancelled/unknown；maps 为 zone 权威；迁移计划覆盖内置/`drivers/*`/外部 `.so`/数据/回滚 |
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
| 3 | 空间语义（zone/坐标/朝向） | **完整采纳（改写）**：maps/place/zone 为权威（改名/坐标/多边形/trust），层级树降为组织用途与迁移期回退 | E4d（E1b 层级派生降为回退） |
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

---

# CEO 审查输出（/plan-ceo-review 2026-10-20，SCOPE EXPANSION）

## 评审历程

- 模式：SCOPE EXPANSION（D1 用户选择）；裁定方式：逐项（D2.0）。
- 扩张裁定：X1/X2/X3/X4/X6/X7 Include，X5 Defer（D2.1-D2.7），总装 D2.final 确认（依赖闭合）。
- 对抗性 spec 评审 Round 1：6/10 FAIL，11 条发现（5 阻塞/4 重要/2 轻微），D3 批量裁定全修。
- Round 2：8/10，11/11 落实；残留 1 重要 + 4 轻微，6 条一行级修补全落（标 R2-x-x）；评审员确认无需第三轮。
- 文档批准：D4 批准 spec + CEO 摘要当前版本。
- TODO 裁定：D5 快照 token 计数=留在当前计划（E2 必需工作）；D6 watch 命中推送=TODOS.md；D7 watch/告警统一方向=留在当前计划（架构观察项，见 §6 后注）。
- 外部声音：codex CODEX_MODEL invalid（custom endpoint 未配 frontier 模型）；native fallback 完成但报告内容丢失（已知问题，第三次复现）——无可计外部覆盖，记录在案。

## NOT in scope

- 延期：X5 云边两级 Hub（D2.5，TODOS.md 已录，启动条件=edge 成熟+E1-E4）。
- 拒绝：无（7 项无一 Cut）。
- 沿用原裁定不采纳：OpState 程序计数器（#11）、哈希链账本（#12）、插件双哈希治理（#13）、手动接管/运动安全（#15）——触发条件见 §6 E5。
- D7（watch/告警规则评估统一）为架构观察项留在本计划：告警规则与 watch 条件是同构机制（声明条件+事件流求值+触发动作），远期方向=告警规则编译为 watch 条件；**本期不动告警系统**，E1 落地后重评。

## 0I 时序拷问（实施者视角）

- **HOUR 1（地基）**：LiveState 挂 EventBus 的事件类型清单（PropertyChange 等，E1 spec 枚举）；**W1 证明项：watch 唤醒信号与 scheduler 的优先级语义**——scheduler 有 30s 合并窗（非 Critical）+ 小时预算 20/ws + ready 队列 50（满时 Critical 也丢，TODOS 结转项）；watch 命中若被合并窗压 30s，"秒级反射弧"承诺破产。**Implementation owner must prove**：watch 唤醒走独立 WakeSignal 变体、绕过 30s 合并窗、计入独立配额（E1 spec 阶段裁定）。合并顺序：ai-consolidation 先落地（同批文件 runner/manager），E1a 在其上生长。
- **HOUR 2-3（核心逻辑）**：watch 条件字段路径映射（AGH state field/source field → 我们 thing property/遥测流的词表对齐）；GoalWatch 目标→watch 编译的模型输出 schema；快照 6KB 是英文参数，中文字节预算 E2 实测重定 [estimated]。
- **HOUR 4-5（集成）**：E1d 检测算法选型（基线漂移/阈值学习——E1d spec 前置调研）；X7 模板规则引用与场景包 `{index}` 计数参数的解析交互。
- **HOUR 6+（打磨）**：watch 命中率/误唤醒率/唤醒延迟指标；账本体积增长估算（状态变更全量落；参照 agent_runs 480 行/天/工作区先例）。
- **估计**：E1 主体已交付；E4 单独 human ~3-4 周 / CC+gstack ~4-6 天 [estimated]（含 E4a-e 扩张）。

## 评审节发现（Sections 1-11，strategy-only 深度）

- **S1 架构**：依赖图=呈现→Hub→接入，Brain→Hub，接缝在 Brain 层；新耦合（Brain 对 Hub 快照/唤醒的依赖）即架构目的，成立。10x 先断点=事件总线吞吐与账本写盘；SPOF=Hub 运行时与 DataServer 同进程（重启语义已覆盖）。回滚=关触发源开关，心跳巡检不受影响。发现 2 项（W1 调度优先级、账本写放大）均转为证明项/指标项。
- **S2 错误与救援**：能力行映射——watch 注册失败（schema 非法→拒绝+原因）、事件流中断（watch 超时 matched:false）、唤醒投递失败（重试+DLQ，复用 RunRecorded 模式）、LiveState Lagged（health 派生标 unknown）、目标编译模型失败（拒绝目标+原因）、知觉权误报风暴（成本闸门 fail-closed）、快照装配失败（不得炸 run——AGH "hook never throws" 同款）、仲裁持有者崩溃（**job 需租约 TTL**，参照 AGH 10min 结果保留）、run 进行中唤醒（steer 入队）。0 CRITICAL GAP（各项有主）。
- **S3 安全**：watch 条件注册=确定性比较器无代码执行，注入面小；快照进 prompt 复用现有不可信围栏模式；**目标表自然语言→模型编译出的 watch 规则必须落库可审计+给人看**（新增要求）；caller 配额防资源耗尽。3 项发现均有归宿。高severity 0。
- **S4 数据流/交互边界**：同会话重复条件→幂等/计数；N watch 同时命中→1s 合并（AGH 先例）；重启重织竞态→重织幂等（注册表 upsert）；设备删除时有活跃 watch→失效事件 matched:false+原因。4 项边界全映射。
- **S5 代码质量**：双读面过渡（device_cache 驱动侧写面/LiveState AI 侧读面）已裁定；catalog 单一事实源顺手消债。No issues found。
- **S6 测试**：退出标准 R2 修复后可测；金字塔=watch 比较器纯函数单测（大量）+唤醒链路集成+退出标准端到端。**LLM/prompt 变更**（快照注入+目标编译）→ 必须跑 evals/ 场景库并加基线对比。缺口 2（E1d 成本硬顶实测、重织测试）已进 E1 退出标准。
- **S7 性能**：LiveState 内存=工作区设备数×状态大小（moka 10k 先例）；快照注入=每 run 一次的 token 成本主预算项；**账本写放大**——高频遥测变更设备的状态变更全量落需要去抖/聚合上限（implementation owner must prove：状态变更落账本的频率上限）；watch 评估按字段倒排索引避免 O(事件×watches)。2 项。
- **S8 可观测**：指标组=watch 注册数/命中率/误唤醒率/唤醒延迟/重织耗时/LiveState 事件年龄/快照字节实测；day-1 dashboard=watch 拓扑（E3 Flow 即此）；watch 命中/超时/重织事件落账本即可回放=自 observability。1 组 gap 已录。
- **S9 部署**：迁移 additive（目标表新表）；绞杀者并行（心跳与 watch 共存无切换窗口）；feature flag=watch 引擎总开关复用 mode 门控；回滚=关开关。1 项风险（账本体积）已在 S7 记录。
- **S10 长期轨迹**：债务 2（双读面过渡期、临时 watch 无持久化——均已显式声明）；Reversibility 4/5（E1-E3 关开关即回滚；E4 协议变更 3/5）；平台潜力=watch 引擎是可被告警系统消费的通用能力（D7 观察项）；1-year 可读性=P1-P6+分层图。 
- **S11 设计/UX**：E3 工作台有 UI 范围。IA=设备卡/Flow/World 三 tab 对应三种投影；状态覆盖矩阵（loading/empty/error/partial）与移动规格留给 E3 spec；**建议 E3 实现前跑 /plan-design-review**。

## Dream state delta

本计划落地后：watch 网值守+目标维持+协同链可溯可回放成立（12 个月理想的主干）；仍未到：云边自治（X5）、治理层（E5 等触发）、composite 原语的成熟抽象（E2 只交框架）。

## What already exists（复用清单）

- Trigger 接口（`crates/agent/src/runtime/thing_agent/trigger/mod.rs:18-24`）——watch/知觉权唤醒的挂载点
- DataServer device_cache+EventBus+cache-before-publish 不变量（`crates/runtime/src/data_server.rs`）——LiveState 底座
- 派生表达式引擎（`crates/runtime/src/expression.rs`）——zone 推算复用
- runner 硬预算（`crates/agent/src/runtime/thing_agent/runner.rs:25-28`）——E1d 成本闸门退化形态附着点
- policy crate gate_check——Policy 接缝现成实现
- ai-consolidation 统一 Run/RunReport/ DLQ 重试模式（persist.rs）——E2 前身与唤醒投递可靠性模式
- 场景包实例化管线（dry-run/幂等）——X7 模板分发通道

## Error & Rescue Registry（strategy-only 能力行）

| 能力 | 失败机制 | 用户影响 | 已有护栏 | 实施前验证责任人 |
|---|---|---|---|---|
| Watch 引擎 | 条件 schema 非法 | 注册被拒+原因 | 词表注册表校验 | E1 spec |
| Watch 引擎 | 事件流中断 | watch 超时 matched:false | 超时上限 3600s | E1 spec |
| 唤醒通道 | 投递失败 | run 未唤醒 | 重试+DLQ（persist.rs 模式） | E1 spec（W1 优先级裁定同） |
| LiveState | EventBus Lagged | 摘要空洞 | health 派生标 unknown | E1 spec |
| GoalWatch | 目标编译失败 | 目标被拒+原因 | 编译结果落库可审计（S3 新增） | E1c spec |
| 知觉权 | 误报风暴 | token 烧穿 | 成本闸门 fail-closed（E1 退化形态） | E1d spec |
| 快照注入 | 装配失败 | —（不得炸 run） | hook never throws 同款 | E2 spec |
| Job 仲裁 | 持有者崩溃 | 资源死锁 | **job 租约 TTL**（S2 新增） | E1 spec |
| 重织 | 重启重织竞态 | 重复/丢失 watch | 重织幂等 upsert | E1c spec |

## Failure Modes Registry

```
  CODEPATH      | FAILURE MODE        | RESCUED? | TEST?  | USER SEES?      | LOGGED?
  --------------|---------------------|----------|--------|-----------------|--------
  watch 注册    | schema 非法          | Y 拒绝   | E1 单测 | 注册失败+原因    | 账本
  watch 评估    | 事件流中断           | Y 超时   | E1 集成 | 超时事件         | 账本
  watch 唤醒    | scheduler 节流吞掉   | W1 待证  | E1 集成 | 延迟≤独立配额    | 指标
  LiveState     | EventBus Lagged     | Y 降级   | E1 单测 | health=unknown  | 指标
  GoalWatch     | 编译模型输出非法     | Y 拒绝   | E1c 单测| 目标被拒+原因    | 账本
  知觉权        | 误报风暴             | Y 闸门   | E1d 实测| 成本硬顶生效     | 指标
  快照注入      | 装配异常             | Y 吞掉   | E2 单测 | run 照常         | trace
  仲裁          | 持有者崩溃           | Y TTL    | E1 集成 | busy 带持有者    | 账本
  重织          | 重启竞态             | Y 幂等   | E1c 集成| 无感            | 账本
```

9 行，0 CRITICAL GAP（W1 行为语义待 E1 spec 裁定，有主）。

## Diagrams

**watch 生命周期状态机**：

```
registered ──事件命中──► matched（唤醒会话，one-shot 结束）
    │────超时(≤3600s)──► timeout（matched:false 事件）
    │────unwatch───────► cancelled
    │────Hub 重启──────► 易失消失（目标表 watch 由重织重建；临时 watch 不重建）
    非法转移：matched 后再命中（one-shot 已结束，注册表已移除）
```

**唤醒数据流（含影子路径）**：

```
事件流 → LiveState 更新 → watch 词表求值（纯函数）
  │命中                       │设备离线/字段缺失=nil 路径：挂起至超时
  ▼                           │空工作区=empty 路径：无 watch，快照为空（正常）
唤醒信号（合并 1s 窗）→ WakeSignal 变体 → scheduler（W1：独立变体绕 30s 合并窗）
  │                                        │调度器故障=error 路径：重试+DLQ
  ▼                                        ▼
run 触发（含快照注入）              DLQ 事件+指标告警
```

**部署/回滚**：E1a→E1b→E1c→E1d 各自 feature flag（mode 门控）独立开关；回滚=关开关，心跳巡检路径不受影响（绞杀者并行期）。

## Implementation Tasks（strategy-only：下一步=设计/调研动作）

- [ ] **T1 (P1, human: ~1w / CC: ~1d)** — E1 — 编写 E1 值守中枢子 spec：watch 词表/LiveState/唤醒 W1 优先级裁定/账本捕获实现形态/仲裁 TTL/重织
  - Surfaced by: S1 W1 证明项 + S2 仲裁 TTL + 0I HOUR 1
  - Files: to be determined（新建 docs/superpowers/specs/2026-XX-XX-e1-watch-hub-design.md）
  - Verify: spec 对抗评审 PASS
- [x] **T2 (P2, human: ~3d / CC: ~2h)** — E2 — 五接缝封闭契约设计文档（Trigger/Budget/Approval/Policy/ToolExec 输入输出契约+替换规则）
  - Surfaced by: §4 Brain 层 + P4
  - Files: `docs/superpowers/specs/2026-10-21-e2-brain-seams-design.md` + `crates/agent/src/runtime/thing_agent/seams/`
  - Verify: 契约评审 + 每接缝真实替换实现（`seams::*::tests`，2026-10-21 完成）
- [x] **T3 (P2, human: ~2d / CC: ~1h)** — E3 — 工作台设计评审前置（/plan-design-review）
  - Surfaced by: S11
  - Files: `docs/superpowers/specs/2026-10-21-e3-presentation-workbench-design.md`（4 裁定：IA 全套/SSE/全量状态覆盖/场景包超集）
  - Verify: 设计评审完成（2/10 → 8/10，2026-10-21；mockup 因缺 OpenAI key 走文本评审）
- [ ] **T4 (P2, human: ~3d / CC: ~4h)** — E1d — 知觉权检测算法选型调研（基线漂移 vs 阈值学习 vs 统计异常检测）+ 成本模型
  - Surfaced by: 0I HOUR 4-5 + E1d 退出标准
  - Files: evals/（调研产物）
  - Verify: 选型报告 + 成本上限实测方案
- [x] **T5 (P3, human: ~1d / CC: ~30min)** — E2 — 快照中文 token 预算实测基线（D5 已裁定进当前计划）
  - Surfaced by: D5 + 0I HOUR 2-3
  - Files: `crates/agent/src/prompt/snapshot.rs`（6KB 字节硬上限 + 中文 50 台基线用例）+ `RunReport.snapshot` 指标
  - Verify: `prompt::snapshot::tests::chinese_baseline_stays_within_6kb`（bytes/chars/est_tokens 实测，2026-10-21）

## TODOS.md updates

2 项已录：X5 云边两级 Hub（P2）、watch 命中推送「需要你」（P2）。

## Completion Summary

```
  +====================================================================+
  |            MEGA PLAN REVIEW — COMPLETION SUMMARY                   |
  +====================================================================+
  | Mode selected        | SCOPE EXPANSION                             |
  | System Audit         | binding decisions 无冲突;learnings 2 条应用  |
  | Step 0               | 扩张 6 收 1 延;两轮 spec 评审 17 条全修      |
  | Section 1  (Arch)    | 2 issues noted (W1 调度/账本写放大→证明项)   |
  | Section 2  (Errors)  | 9 能力行映射, 0 GAPS                         |
  | Section 3  (Security)| 3 issues found, 0 High severity              |
  | Section 4  (Data/UX) | 4 edge cases mapped, 0 unhandled             |
  | Section 5  (Quality) | No issues found                              |
  | Section 6  (Tests)   | Diagram produced, 2 gaps(已入 E1 退出标准)   |
  | Section 7  (Perf)    | 2 issues found                               |
  | Section 8  (Observ)  | 1 组 gap(指标组已录)                          |
  | Section 9  (Deploy)  | 1 risk flagged(账本体积)                      |
  | Section 10 (Future)  | Reversibility: 4/5, debt items: 2            |
  | Section 11 (Design)  | E3 UI scope——建议实现前 /plan-design-review  |
  +--------------------------------------------------------------------+
  | NOT in scope         | written (5 items)                            |
  | What already exists  | written                                      |
  | Dream state delta    | written                                      |
  | Error/rescue registry| 9 rows, 0 CRITICAL GAPS                      |
  | Failure modes        | 9 total, 0 CRITICAL GAPS                     |
  | TODOS.md updates     | 2 items proposed, 2 accepted                 |
  | Scope proposals      | 7 proposed, 6 accepted (EXPANSION)           |
  | CEO plan             | written (~/.gstack/.../ceo-plans/)           |
  | Outside voice        | codex unavailable + native lost; no coverage |
  | Lake Score           | N/A（无 coverage 计分题）                     |
  | Diagrams produced    | 3 (state machine / wake dataflow / rollback) |
  | Stale diagrams found | 0                                            |
  | Unresolved decisions | 0                                            |
  | Reality rows         | 7/7 rows pass, 0 unverified                  |
  +====================================================================+
```

## Eng Review — Scope Record（2026-10-20，/plan-eng-review）

- **目标**：本架构 spec（CEO CLEARED 版，commit fc8aac90）
- **feature answers**：无新 cut 提案
- **structure**：**Original arrangement**（D1）——架构 spec 保持 E1-E5 整体，实施按 E1a→d 子阶段各出实施计划；与 ai-consolidation eng D1 先例一致
- **accepted scope**：spec 全量（CEO 批准版）
- **pending remedies**：无
- **Reality rows**：premise-table(unverified=0)/already-done/surface-check/binding-decisions/numbers/deferred-asks/pr-count 全 pass；tier2 transaction-semantics（新表 additive）/perf-table（无 benchmark 声明）pass；关键实测：EventBus broadcast(1000) Lagged 丢弃（event_bus.rs:39,63）、scheduler QUEUE_CAPACITY=50/MERGE_WINDOW=30s/HOURLY_WAKE_LIMIT=20（scheduler.rs:46-50）
- **复杂度**：E1 ~15-20 文件+4 新服务类 [estimated]；触发结构门禁，D1 裁定 Original

## Eng Review — Decision ledger

### R1: watch 唤醒信号的调度语义（W1 从 prove-note 升级为裁定）
Finding: [P1] (confidence: 9/10) crates/agent/src/runtime/thing_agent/scheduler.rs:46-50 — `QUEUE_CAPACITY=50`、`MERGE_WINDOW=30s`、`HOURLY_WAKE_LIMIT=20`；非 Critical 同 dedup_key 进 30s 合并窗、超预算丢弃。watch 命中若走普通通道：延迟 30s 且值守场景（N watch 频繁命中）迅速烧穿 20/h 预算。
Plan baseline: spec 0I W1 prove-note「建议：独立 WakeSignal 变体、绕过 30s 合并窗、计入独立配额」——未裁定。
Runtime evidence: scheduler.rs:46-50 实测常量；TriggerSource 五变体 types.rs:13；Critical 绕合并窗与节流（scheduler.rs:12-20 注释）。
Comparison grid:

| Choice | Current | A | B | C |
|---|---|---|---|---|
| watch 唤醒通道 | 未裁定 | 独立变体+独立配额 | 复用 Critical 通道 | 普通通道（现状语义） |
| 30s 合并窗 | 非 Critical 合并 | 绕过 | 绕过 | 被合并（+30s 延迟） |
| 20/h 预算 | budgeted 超即丢 | watch 独立配额不占用 | 绕过（与告警共享 Critical 豁免） | 超预算被丢 |
| Critical 语义纯度 | Critical=告警 | 不染 | 稀释（watch≠告警） | 不染 |
| 反射弧延迟承诺 | 秒级目标 | 成立 | 成立 | 破产（分钟级） |

History: D2（2026-10-20）三选项（独立变体/复用 Critical/普通通道）被用户全部推翻，裁定方向=「30s 合并窗本身是坏设计，应改合并窗设计（配置化或其他手段），而非让 watch 绕道」——根因修复取向。D3 按此方向重做选项。

State: approved
Actual answer: D3=C（去掉合并窗）+ D4=A（同键速率限制替代）— 2026-10-20
Accepted scope: **scheduler 废除 30s 合并窗（merger task 退役），所有信号即来即醒；防风暴语义由同 dedup_key 速率限制承接（每分钟每键 N 次，workspace 可配，超出丢弃+指标 `agent_wake_rate_limited`）**——一并清偿 TODOS P2「thing+rule 级速率闸」旧债。watch 唤醒无需特例变体（无窗可绕）。**回归面**：心跳/事件类信号失去 30s 聚合（每次上报都可能唤醒）——速率限制参数（N 默认值、是否按 trigger 类型分档）与小时预算的联动在 E1 spec 裁定；现有 scheduler 测试套件中合并窗相关用例需重写为限速语义（REGRESSION：合并窗行为保持清单项由 ai-consolidation spec 继承，本次为显式行为变更，需用户已知悉——D3/D4 即此裁定）。


### R2: LiveState 的 EventBus Lagged 恢复语义
Finding: [P2] (confidence: 8/10) crates/runtime/src/event_bus.rs:39,63 — `broadcast::channel(1000)`，慢订阅者在 recv 侧 Lagged 丢弃；LiveState 以 EventBus 为唯一输入（R1-3-2 裁定），滞后丢事件→派生摘要空洞。
Plan baseline: spec §4「LiveState Lagged→health 派生标 unknown」（CEO 轮 S2 行）——只有降级，无恢复。
Runtime evidence: data_server.rs `cache_and_publish`（R2 评审员复核 :292）先写 device_cache 再发事件——cache 始终是完整真相，可支撑全量重同步。
Comparison grid:

| Choice | Current | A | B |
|---|---|---|---|
| Lagged 后行为 | 未规定 | 检测 Lagged→从 device_cache 全量重同步+health 恢复 | 只标 unknown 不补 |
| 摘要正确性 | — | 空洞自愈（秒级） | 空洞持续到下一次该设备变化 |
| 实现成本 | — | 小（cache 是完整真相，重读即可） | 零 |

State: approved
Actual answer: D5=A（Lagged 检测+全量重同步）— 2026-10-20
Accepted scope: LiveState 订阅 EventBus 时检测 Lagged；触发即从 device_cache 全量重同步（cache-before-publish 不变量支撑），重同步可加去抖；health 派生在空洞期标 unknown、重同步后恢复。

## Eng Review — Sections 1-4（2026-10-20）

### Section 1 架构
- F1（P1，scheduler 合并窗）→ **accepted**：D3=C 去掉合并窗 + D4=A 同键速率限制替代（R1 台账）。连带收益：merger task 退役使调度器少一类机制；TODOS「thing+rule 级速率闸」旧债清偿；「Critical 队列满被丢」结转项的语义空间打开（E1 spec 一并回答）。
- F2（P2，EventBus Lagged）→ **accepted**：D5=A Lagged 检测+全量重同步（R2 台账）。
- 分发架构：无新 artifact（全部能力在现有 cloud binary 内）；edge 不涉及（X5 已 Defer）。
- SPOF/回滚：CEO 轮已覆盖（关触发源开关即回滚）；本轮补充——合并窗删除是行为变更，回滚=恢复窗口常量+速率限制开关（feature flag 复用 mode 门控）。

### Section 2 代码质量
- F3（P3，ports 约束）→ **记录**（事实性补强，无行为变更）：目标表/账本持久化必须走 `crates/runtime/src/ports.rs` port trait（runtime 禁止直接 sqlx，learning runtime-db-via-ports-only）——E1 spec 写明。
- **watch 引擎 crate 归属**（E1 spec 裁定项，记录备查）：评估器（条件词表+纯函数比较器）宜在 crates/runtime（近 EventBus/DataServer，零 sqlx）；唤醒适配器（Trigger 实现，发 WakeSignal）在 crates/agent。跨 crate 数据经 ports。
- catalog 单一事实源消债（CEO 轮已录，E2 顺手做）。

### Section 3 测试
框架：Rust `cargo test`（workspace）+ web `vitest`（124 个测试文件现存）。

覆盖图（新增路径）：

```
CODE PATHS                                              USER FLOWS
[+] watch 评估器（runtime, 纯函数）                      [+] 运维设目标→值守
  ├── 条件词表注册/比较器  [GAP→表驱动单测]                 ├── [GAP→E2E] 设目标→越限→提案出现（E1 退出标准）
  ├── 字段路径解析          [GAP→单测含非法路径]             └── [GAP→E2E] 重启后 watch 网重织（E1c 退出标准）
  └── one-shot/超时语义    [GAP→单测]                      [+] watch 命中→「需要你」推送（D6 TODOS 已录）
[+] LiveState                                            [+] 告警 flapping（回归面）
  ├── Lagged 检测+重同步   [GAP→集成测试（D5 批准）]         └── [CRITICAL→REGRESSION] 合并窗删除后限速语义
  └── health/zone 派生     [GAP→单测]                          （D3/D4 批准范围：scheduler 合并窗用例重写为限速用例；
[+] scheduler 限速器                                         保留行为=60s 指令去重/小时预算/Critical 绕过/队列容量）
  ├── 同键 N/min 计数      [GAP→单测+边界]                 [+] LLM 链路
  └── 配额耗尽丢弃+指标    [GAP→单测]                        ├── [GAP→EVAL] 快照注入 prompt 变更→evals/ 场景库+基线
[+] 仲裁（单资源退化形态）  [GAP→集成：并发 busy 带持有者]     └── [GAP→EVAL] 目标→watch 编译输出 schema→evals/ 用例
[+] 快照装配               [GAP→单测（预算超限截断）+不得炸 run]
```

**LLM/eval scope**：快照注入（E2）与目标编译（E1c）触及 prompt——evals/scenarios.json 需加快照注入基线用例与目标编译合法/非法输出用例，对比基线=现四段式 prompt 的现有场景。

**Tests made obsolete by this plan**（提案，实现期走 /test-audit 退休卡）：scheduler 合并窗相关用例（merger task 测试）——语义被限速器替代，重写非删除。

### Section 4 性能
- F4（P3，账本写放大）→ **pending**（E1 spec 裁定数值框架：每物每分钟状态变更落账上限+去抖；owner=E1 spec 作者）。
- watch 评估热路径：按字段倒排索引（spec 已录），评估复杂度 O(命中字段的 watch 数) [estimated]。
- LiveState 内存：设备数×状态大小，moka 10k 容量先例；Lagged 重同步去抖（D5 范围）。
- 无 N+1 新风险（LiveState 内存读；快照装配一次/turn）。

### Suppressed findings
无（所有发现 ≥5/10 置信均已在主报告）。

## Eng Review — Implementation Tasks

- [ ] **T1 (P1, human: ~1w / CC: ~1d)** — E1 — E1 值守中枢子 spec：并入本轮裁定（合并窗废除+同键限速器、Lagged 全量重同步、仲裁 TTL、重织）+ F4 账本数值框架 + watch crate 归属（评估器 runtime / 适配器 agent）
  - Surfaced by: R1/R2 台账 + F3/F4
  - Files: to be determined（新建 docs/superpowers/specs/2026-XX-XX-e1-watch-hub-design.md）
  - Verify: eng 复审收敛 F4；spec 对抗评审 PASS
- [ ] **T2 (P2, human: ~1d / CC: ~1h)** — scheduler — 合并窗删除+限速器的回归测试重写（保留行为：60s 指令去重/小时预算/Critical 绕过/队列容量 50；merger 用例走退休卡）
  - Surfaced by: Section 3 REGRESSION（D3/D4 批准范围）
  - Files: crates/agent/src/runtime/thing_agent/scheduler.rs（测试）
  - Verify: `cargo test -p tinyiothub_agent scheduler`
- [ ] **T3 (P2, human: ~0.5d / CC: ~30min)** — evals — 快照注入+目标编译的 evals 场景用例与基线
  - Surfaced by: Section 3 LLM/eval scope
  - Files: evals/scenarios.json
  - Verify: eval 跑通且基线对比在案

## Eng Review — Completion summary

- Step 0: Scope Challenge — scope accepted as-is（structure: Original arrangement，D1）
- Reality rows: 7/7 tier-1 + 2 tier-2 emitted（全 pass），0 unverified premises
- Architecture Review: 2 issues found（F1/F2，均裁定落地）
- Code Quality Review: 1 recorded（F3 ports 约束，事实性）
- Test Review: diagram produced，gaps 已转化为 E1 退出标准/回归契约/eval 范围
- Performance Review: 1 issue（F4 pending→E1 spec）
- NOT in scope: written（CEO 节）
- What already exists: written（CEO 节）
- TODOS.md updates: 0 new（本轮无新候选）
- Failure modes: 0 critical gaps
- Unresolved decisions: 1（F4 账本写放大数值框架，owner=E1 spec）
- Outside voice: codex unavailable + native 报告丢失；no coverage
- Parallelization: Sequential implementation, no parallelization opportunity.（E1a→d 严格串行，子阶段共享 scheduler/runtime 模块）
- Lake Score: N/A（无 coverage 计分题）
- Test Plan Artifact: ~/.gstack/projects/Grong-tinyiothub/chenguorong-ai-consolidation-eng-review-test-plan-20261020-204046.md

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 2 | CLEAR | 7 proposals, 6 accepted, 1 deferred |
| Outside Review | codex (auto) | Independent 2nd opinion | 10 | unavailable | CODEX_MODEL invalid（修复：`gstack-config set model_frontier_openai <id>`）；native fallback 报告内容丢失（本日 4 次复现）——无可计覆盖 |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 6 | ISSUES OPEN | 4 issues, 0 critical gaps（F1/F2 裁定落地，F3 记录，F4 递延 E1 spec） |
| Design Review | `/plan-design-review` | UI/UX gaps | 2 | CLEAR | 2026-10-12 AI 中心轮（旧对象）；E3 工作台实现前需新一轮 |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

- **OUTSIDE COVERAGE:** codex, plan-review phase, **unavailable**——CODEX_MODEL invalid；native fallback（Plan subagent）完成但最终消息只剩 "Complete."，报告内容丢失。无可计外部覆盖。
- **VERDICT:** CEO CLEARED；ENG 本轮 4 issues 中 3 裁定落地（合并窗废除+限速器、Lagged 重同步、ports 约束）、1 递延 E1 spec（F4）——**E1 子 spec 完成后 eng 复审收敛 F4 方可实施**。

**UNRESOLVED DECISIONS:**
- F4：账本写放大的数值框架（每物每分钟状态变更落账上限+去抖）——owner=E1 spec 作者（T1）
