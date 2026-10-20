# Agnes Harness（AGH）深度对标与裁定（watch 唤醒 / 快照摘要 / Devices 工作台路线）

- 日期：2026-10-20
- 分支：ai-consolidation（对标期间 HEAD 691faf1e）
- 状态：裁定表 15 项已与作者逐表确认（2026-10-20 会话）
- 对标对象：[AgnesAI-Labs/agnes-harness](https://github.com/AgnesAI-Labs/agnes-harness)，main 分支 2026-10-20 快照（pre-alpha，Apache-2.0，TypeScript monorepo）

## 背景与方法

动机：AGH 是面向多设备协同的 agent harness（"LLM 是脑、Harness 是记忆、MHS 是身体"），其 mars-world 示例展示了 11 台虚拟设备的三条多设备协同链（侦察采样 / 风暴收拢 / 救援），与 tinyiothub 的园区多设备目标同向。本文回答两个问题：**我们距离 AGH 差在哪**（事实层，§2）和**每一项怎么办**（决策层，§3）。

方法：两份调查报告交叉——
- AGH 侧：README、`docs/develop/architecture.md`、`docs/develop/source-map.md`、`docs/guide/{mhs,sessions,skills,security,packages,limitations}.md`、`packages/mhs/{README.md, spec/{mhs-spec,mos-spec,mhs-conformance}.md, server/hub-api.md}`、`packages/core/src`（step/session.ts、op-state.ts、resume.ts、log/integrity.ts、storage.ts）、`packages/protocol/schema/session-v1.json`、`examples/mars-world/README.md`。
- 我方侧：crates/{agent,llm,runtime,scheduler,policy,skills,memory,plugin-sdk,core} 源码普查 + apps/cloud AI 域 + web/src/ui AI 视图 + TODOS.md + ai-consolidation spec。

## 1. 最本质的差距（结论先行）

**AGH 把 Hub 当"运行时"，我们把 Hub 当"数据库+轮询器"。**

AGH 的 AgnesHub 内存中活着每设备的实时状态、资源占用、job 进展、watch 条件、健康/位置派生摘要；brain 不轮询——它在四个被固定规则决定的时刻拿快照（读设备/工具返回/turn 开头/被唤醒），其余时间由事件唤醒（hub-api.md §15："brain 看到多少取决于发生多少，而不是数据多快"）。

我们的 DataServer 同样有 device_cache 与 EventBus，但 AI 侧的认识方式是**周期心跳轮询 + 主动 `get_thing_profile` 逐个查**。设备对 AI 而言不是"活着会说话的实体"，而是"等被查询的行"。这个哲学差异派生出 §2 中大多数具体差距。

## 2. 逐维度事实对比

### 2.1 Agent Loop 恢复语义（差距最大）

| | AGH | 我们 |
|---|---|---|
| 执行状态 | **OpState 程序计数器**（phase: checkpoint/inference/tools/compaction/deferred/failure_drain），与事件批**同事务 CAS 提交**——"崩溃没有 between"（session.ts；session-v1.json） | runner 内存态（RunContextInner，`crates/agent/src/runtime/thing_agent/runner.rs:64-84`）+ 完成时落 RunReport；run 中途崩溃即丢失 |
| 取消 | 取消是账本已决事实，任何 phase 看到 cancel 记录后不得再发模型请求 | CancellationToken best-effort + 30s 兜底（runner.rs:29） |
| 恢复 | 每个未完成 effect 逐个裁定 rerun/rerun-idempotent/retry/poll/settle；嵌套调用图按 parentEffectId 遍历（step/resume.ts） | 无 run 级恢复概念 |
| 活性 | 相位边不推进记 `run-no-progress`；phase 抛错记诊断并结束 turn | 连败 5 次暂停（`governor.rs`），粒度是 run 不是相位 |
| 可替换段 | **ReplacementOperation**：Inbox/Budget/Inference/Approval/ToolExecution/StopGate 六段可整体替换（封闭契约，不 fork core 即可 override 关键决策） | 无对应物，改 loop 行为只能改源码 |

### 2.2 事件账本（审计级 vs 记录级）

AGH：append-only 事件链（31 种内建事件类型），每条带 `origin`（principal/model/system/tool:*…）与 **`trust` 污点标注**（不可信内容进过上下文沿账本传播）；JCS（RFC 8785 风格）+ SHA-256 **哈希链**，verified-head 增量校验，完整性失败时一切请求拒 `E_LEDGER_INTEGRITY`（log/integrity.ts、sessions.md）。

我们：`agent_runs` 表 + ToolTraceEntry 轨迹 + RunRecorded 重试/DLQ。留痕够用，无篡改检测、无事件级信任标注。

### 2.3 设备抽象（声明制 vs 模板拷贝制）

AGH：设备连接时**自我声明** tools（参数 schema/timeout/motion 标记）/ state fields（含 alert 阈值）/ data sources / resources / placement（MHS §5 `mhs/register`）；Hub 按声明**校验+钳制**每次调用，参数越界在 Hub 侧拦住（MHS §6.4）。

我们：能力来自 ThingTemplate **一次性快照拷贝**（`apps/cloud/src/domains/marketplace/scene_instantiator.rs`），实例化后脱钩；`invoke_action` 直接透传驱动（`apps/cloud/src/domains/agent/host/tools/thing/invoke_action.rs:40-44`），无运行时声明校验层。

### 2.4 Brain 与设备的接口形态

| | AGH | 我们 |
|---|---|---|
| 工具集 | **固定 7 个**（list_devices/read_device/call_device/set_device/stop_device/watch_device/unwatch_device）；设备自有工具是 `call_device` 的参数，**工具列表不随设备增减变化**（hub-api.md §15.1） | 9 个 thing 工具 + MCP 设备工具（`apps/cloud/src/domains/agent/host/tools/thing/mod.rs`）；`invoke_action`≈`call_device`；无 watch/stop |
| 上下文 | 每 turn 开头注入 ≤6KB 快照：地图+places 在前，然后每设备健康/位置/在干什么（hub-api.md §15.2 context hook） | 四段式 prompt（`crates/agent/src/prompt/runtime.rs`），设备信息靠 AI 主动查 |
| 读设备 | `read_device` 不带 sources 只回 state+health+position 摘要 + 一句写给模型的 `text` | `read_property` 逐属性读，无摘要层 |
| 唤醒 | job 结束/watch 命中/estop/人接管 → 唤醒对应会话（1s 内合并）（hub-api.md §15.3） | 无；run 结束即结束 |

### 2.5 条件等待 watch（我们完全缺失的原语）

AGH：**确定性比较**（state 字段 lt/gt/eq/ne；source 字段阈值；detections 出现某标签；health/trust 变化），one-shot，每 caller 上限 20，超时上限 3600s，命中唤醒对应会话。明确"watch 里不跑模型，需要理解的条件由 source 陈述"（hub-api.md §12）。

我们：TODOS 中的 **GoalTrigger**（"车间温度维持 20-26°C"式持续目标，P3）本质就是它，未做；Trigger 接口已预留（`crates/agent/src/runtime/thing_agent/trigger/mod.rs:18-24`）。这是多设备协同最关键的缺失——mars-world 的"风暴预警→收太阳能板→降落 hopper→rover 回坞"链式协同靠 watch+唤醒实现，我们只能等下一个心跳 tick。

### 2.6 仲裁与人机优先级（完全缺失）

AGH：caller 分级（level 0 stop=任何人 / 1=ui 人 / 2=agent）；设备声明 **resources**，job 占用资源，`busy` 拒绝带持有者名（"held by drive_to j9 (web)"）；人接管立即打断 AI 的 motion job，松手后 3s 内 AI motion 调用被拒；`hub/stop` 永远可用（hub-api.md §13）。

我们：`invoke_action` 无占用、无仲裁；两个 run 同时操作同一设备无互斥。

### 2.7 空间模型（有骨架无语义）

AGH：map = 命名 2D 帧（`{id, name, bounds, anchor}`，anchor 支持挂到更大帧或地球锚定）；place = landmark（点+朝向）或 zone（≥3 点多边形）；设备声明地图，Hub 合并保留（断连不丢，`maps.json` 持久）；位置 trust 三态（trusted/uncertain/lost）+ 所在 zone 自动推算（MOS §3.5、hub-api.md §4.4/§6.2）。

我们：`parent_id` 层级树（园区→建筑→楼层，`crates/core/src/models/thing.rs:85-116`）+ `position` 字段 + 场景包模板。层级语义清晰，但无坐标帧/区域/朝向概念，AI 无法回答"哪些设备在 X 区域"类空间问题。

### 2.8 审批语义（各有胜负）

AGH 更细：拒绝原因六分类（含 **unavailable ≠ rejected**——"没人可问"不算拒绝且不执行）；会话级 grant（同名工具后续调用免批）；审批卡截断展示规则；"审批不免除未来 turn 的验证"（security.md）。

我们更细：工作区信任三档 + AutonomyPolicy（allow/deny 清单 + 速率熔断，fail-closed，kill-switch 每动作新鲜读档，`autonomous_invoke.rs:278-280`）。AGH 审批主要保护命令执行，我们门控覆盖全部 run。

### 2.9 插件/包治理（代差）

AGH：信任绑定**内容哈希+能力哈希双值**（`package trust <id> <INTEGRITY> <CAPABILITY_HASH>`），能力声明变化必须重新信任；desired/actual 双状态；安装预览强制展示 integrity/warnings/blocker；回滚保留有界（packages.md、plugins.md）。

我们：plugin-sdk 是 FFI ABI 契约（`crates/plugin-sdk/src/driver.rs:9-38`，`DRIVER_ABI_VERSION=1`）；`drivers/` 不在 workspace 编译门禁内（TODOS P2）。无哈希、无能力声明、无生命周期治理。

### 2.10 一致性验收（我们为零）

AGH `mhs-check`：单一 JSON Schema（`mhs-v1.json`，gen:check 防类型漂移）+ 测试 hub + 日志校验器三层；10 个 profile（core/motion/manual/pause/perception/maps/derived/streaming/video/clip）；五级结果（pass/fail/warn/skip/manual）；物理行为用 odometry 自动判定或 `--interactive` 问操作员；`--no-motion` 保证不动设备且记 skip 而非 pass。

我们：设备接入验收 = 连上看有没有数据。

### 2.11 会话共享与恢复

AGH：CLI/Web/SDK 同一后端 session，`/resume <id>` 跨端续接；恢复用**当前**权限（过去授权不复活）；导出/导入带附件哈希校验；LEDGER_FORMAT_VERSION 版本门。

我们：自研 chat（`apps/cloud/src/domains/agent/host/chat/`），SSE 直转，session_key fail-closed 校验；OpenClaw 接入仅在计划文档中；已知缺口：chat 无 per-session 广播（TODOS P2）。

### 2.12 我们领先的方面（如实记录）

- **judge/policy 结构化决策已在线**（`crates/policy` gate_check）——AGH 的 Jev 仍在"集成中"
- **信任三档 + kill-switch** 语义完整度更高
- **AI 中心 UX**（ai-consolidation 的 Run 流/需要你/今日自述）比 AGH Trace 视图更面向运维
- **A2UI + Scene3D**：AI 主动生成界面，AGH 无此能力
- **场景包模板实例化**（智能园区一键复制）AGH 无对应物
- **派生表达式引擎**（`crates/runtime/src/expression.rs` 跨物计算）AGH 无对应物

### 2.13 AGH 自认的空白（不必抄的部分）

pre-alpha 无发布承诺；公网部署未验证；进程内插件无隔离；无硬件认证；外部副作用无 exactly-once；Windows 支持不全（limitations.md）。**这些恰是 IoT 平台必须自己补的**。

## 3. 裁定表（作者已确认，2026-10-20）

| # | AGH 机制 | 裁定 | 理由 |
|---|---|---|---|
| 1 | watch 条件唤醒 | **采纳，S1 最高优先级** | 多设备协同运行时地基；TODOS GoalTrigger 即此；Trigger 接口已预留 |
| 2 | turn 开头设备/健康快照注入（Hub 派生摘要） | **采纳，S2** | 低成本高杠杆；AI 不再逐个 get_thing_profile |
| 3 | 空间语义（zone/坐标/朝向，叠加现有层级树） | **改造采纳，并入 S3** | 不搬 maps 帧概念；给 position 加坐标系语义+zone 归属推算 |
| 4 | Devices 工作台（设备卡+Flow+2D 地图） | **采纳，S3，依赖 S1** | Flow 内容=watch/job 链；无 S1 则工作台只是静态点位图 |
| 5 | 审批原因分类（unavailable≠rejected 六分类） | **采纳，小项，并入 ai-consolidation** | 语义更准，改动小 |
| 6 | 资源占用/仲裁（busy 带持有者、同设备互斥） | **改造采纳，S4 或独立小项** | 多 run 同设备互斥是真需求；人接管打断对楼宇固定设备弱需求，不做 |
| 7 | 固定 7 工具形态 | **已对齐，不另立项** | invoke_action≈call_device；S1 顺带补 stop/watch 工具 |
| 8 | 设备能力运行时声明（注册声明+Hub 校验钳制） | **采纳，S4 远期** | 动 ABI 与驱动模型，最重；S1-S3 验证协同价值后再投 |
| 9 | 一致性验收套件（mhs-check 式） | **采纳，远期** | 等驱动生态成形 |
| 10 | 需求驱动采样（无订阅者关源） | **记录，暂不采纳** | 轮询架构不同，改造面大收益不明确 |
| 11 | OpState 程序计数器+同事务相位边 | **不采纳（现阶段），记录** | 我们 run 以只读分析为主，崩溃=幂等重跑；投入产出不匹配 |
| 12 | 哈希链账本（篡改检测） | **不采纳（现阶段），记录** | 审计需求未出现；RunReport+DLQ 够用 |
| 13 | 插件双哈希信任治理 | **不采纳（现阶段），记录** | driver ABI 治理是独立大课题 |
| 14 | 跨端共享 session resume | **记录** | 与 OpenClaw 接入决策绑定，届时重评 |
| 15 | 手动接管/运动安全（estop/deadman/断连自停） | **不采纳** | 楼宇固定设备为主，无移动机器人 |

## 4. 子项目路线图

```
S1 watch 条件唤醒原语 ──► S2 Hub 侧设备摘要+快照注入 ──► S3 Devices 工作台 ──► S4 设备能力声明
   （Trigger 接口预留位；     （health/zone 派生 + 每 run      （2D 地图+设备卡+Flow；      （模板→运行时声明；
    顺带补 stop/watch 工具）   开头 ≤N KB 快照注入 prompt）    含空间语义改造）              含资源占用仲裁）
```

- 每个子项目独立走 spec → plan → 实施流程，本文档是它们的评审校准尺。
- 裁定 #5（审批原因分类）不单独立项，建议并入 ai-consolidation 实施期。
- S3 依赖 S1：Flow 视图的数据源是 watch/job 事件链。
- S4 依赖 S1-S3 的价值验证，且与 drivers/ 入 workspace 门禁（TODOS P2）联动。

## 5. 明确不采纳清单（防再议）

现阶段不采纳：OpState 程序计数器（#11）、哈希链账本（#12）、插件双哈希治理（#13）、手动接管/运动安全（#15）。记录待重评：需求驱动采样（#10）、跨端 session resume（#14）。重评触发条件：出现审计合规需求（#12）、run 中途崩溃造成实际事故（#11）、OpenClaw 接入立项（#14）、驱动生态 ≥5 个第三方驱动（#13）、引入移动机器人设备类型（#15）。

## 6. 附录

### 6.1 AGH 值得记住的协议细节（S1-S4 实施时回看）

- watch 条件词表（hub-api.md §12）：state 比较 / source 字段阈值 / detections 标签 / health/trust 变化；one-shot；only new data counts。
- 唤醒合并：同会话 1s 内事件合并为一条（§15.3）。
- 快照预算：≤6000 bytes，地图占一半以内，放不下的 places 只计数（§15.2）。
- 健康派生规则（§6.1）：alert 阈值/problem+faults/源流速不足/连接 RTT 四类原因，worst-first。
- 位置 trust 三态判定（§6.2）：trusted/uncertain/lost + age；fixed 设备永远 trusted。
- busy 拒绝文案带持有者（"held by drive_to j9 (web)"）——人读得懂的拒绝。
- MOS derived 源对齐规则：`of_seq` + `t` 等于输入采集时间；pose 失锁必须 `ok:false` 且继续按速率发——"沉默表示定位器停了而非丢了"。
- mhs-check 设计原则：schema 严格处严格开放处开放；跨字段规则不进 schema 进 checker；`--no-motion` 记 skip 不记 pass。

### 6.2 参考索引

- AGH 仓库：https://github.com/AgnesAI-Labs/agnes-harness
- 关键文档：docs/develop/architecture.md（请求路径与状态归属）、packages/mhs/server/hub-api.md（brain 接口全集）、packages/mhs/spec/mhs-spec.md（设备侧协议）、docs/guide/security.md（审批/信任/沙箱）、docs/guide/sessions.md（恢复语义）、docs/guide/packages.md（包治理）、examples/mars-world/README.md（多设备协同示例）
- 我方关键代码：`crates/agent/src/runtime/thing_agent/`（loop 本体）、`crates/runtime/src/data_server.rs`（设备运行时）、`apps/cloud/src/domains/agent/host/tools/thing/`（AI 工具集）、`crates/policy/src/autonomy.rs`（自治门控）
- 相关 spec：2026-10-12-ai-consolidation-design.md（概念模型与 Run 归一）、2026-07-06-ai-harness-loop-design.md、2026-10-08-ai-run-transparency-design.md、2026-08-31-scene-template-design.md、2026-05-28-scene3d-a2ui-design.md
