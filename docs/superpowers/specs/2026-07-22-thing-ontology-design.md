# 设备本体中心化设计（Thing Ontology）

> 日期：2026-07-22（v2，经对抗性审查修订）
> 状态：已确认
> 背景：现有 AI 功能中 workspace 级知识图谱与资源设计偏离方向。IoT 场景更适合"本体智能"——知识定义下沉到物，围绕物本体组织全部 AI 功能。

## 修订记录 v3（2026-07-27，工程实施评审裁决）

实施阶段工程评审（/plan-eng-review 2026-07-27）发现实施与本文多处偏离，逐项裁决如下。**以下裁决与本文其余章节冲突时，以本节为准。**

| # | 偏离 | 裁决 |
|---|------|------|
| V1 | **模板 = 创建期蓝图，非运行时模型源**（D6）。物模型 = 每物自己的 thing_properties/thing_actions 实例；创建时从模板拷贝，之后模板不再被运行时查询。`template_id` 保留为创建来源血缘记录（市场/模板功能仍在用），不再有"模板变更→所有物标 dirty"传播（mark_dirty_for_template_change 已删除）。摘要输入从"模板定义"改为"物的实例定义"。原文章节"物模型=模板层结构定义"中的运行时语义作废。 | 接受实施，修订设计 |
| V2 | **资源表名保持 `resources`**（不更名 thing_resources）；列名 `type` → `resource_type`；`parse_status` 列随解析管线删除（code+schema 已清理）。 | 接受实施 |
| V3 | **设备属性/命令实例表更名**：device_properties → thing_properties、device_commands → thing_actions，真实数据按 ID 保留迁移（告警规则持续解析），00003 的合成种子数据（reboot/temperature 等虚构能力）已删除，旧表与 device_event_triggers 已 DROP。 | 修正实施 |
| V4 | **事件去重索引按状态类限定**（OV-2）：events 加 `is_status` 列，去重唯一索引只覆盖 is_status=1 的状态类 upsert 行；发生类事件纯 append 不受索引约束。原全表去重索引与 append 语义架构性冲突（第二个同事件即静默丢弃）。upsert 重复发生时刷新 event_level、occurrence_count 累加、重置 ack 状态。 | 修正设计 |
| V5 | **多租户隔离为硬性要求**（D3）：所有物管理面/Agent 工具/open API/ack API/资源挂载/确认令牌均按 workspace 作用域校验。 | 强化设计 |
| V6 | **E2 A2UI 本体渲染移出本期**：a2ui.rs 是死代码且伪造控件，已删除；E2 作为独立后续分支（TODOS）。E3 旧版 `commands` 键导入兼容同步放弃（映射从未接线，已删除）。 | 缩减范围 |
| V7 | **迁移安全网落地**（OV-1）：run_migrations 在有待应用迁移时自动 VACUUM INTO 备份到 data/backups/；迁移后 Rust 侧强制执行 foreign_key_check（PRAGMA 只返回行，sqlx 会丢弃），违规即中止启动并提示从备份恢复。 | 落地原设计承诺 |
| V8 | **事件管线的可观测性**：debug 级事件不落库；未知事件名降级 info + unknown_event 标记（以创建模板的事件定义为已知集，无模板不标记）；ingest/unknown/malformed/throttled 以结构化日志 metric 字段计数。 | 按设计补强 |
| V9 | **前端遗留页面拆除**（OV-3）：/knowledge（图谱页）与 /local-resources 路由及视图删除，devices.ts 删除；D13 动作确认弹窗与 D8 升级提示条本期补建。 | 按设计补齐 |
| V10 | **MCP/open 契约更名后的收口**：open send_command 不再向动作定义表 INSERT（改为校验后经 DataServer 下发）；Agent 内置物工具优先于同名 MCP handler；默认 denylist 更名 delete_thing；require_action_confirm 缺失工作区行时 fail-closed。 | 修正实施 |

遗留已知项（已入 TODOS）：events 表保留策略（须按状态/发生分类生命周期）；search_knowledge FTS5 trigram 升级；E2 A2UI 渲染。

## 核心决策汇总

| # | 决策点 | 结论 |
|---|--------|------|
| 1 | 本体定义层级 | 混合：结构（属性/事件/动作）定义在模板层，知识/资源挂实例层，模板可提供默认知识 |
| 2 | workspace 知识图谱 | 完全废弃，代码拆除，实体/关系/解析任务数据直接丢弃 |
| 3 | workspace 静态资源 | 迁移到物（不丢用户文件，见"迁移"节） |
| 4 | "物"与 Device 关系 | Device 原地泛化为 Thing：devices 表加 `thing_type`，车间/园区等概念物也是 devices 记录 |
| 5 | Agent 消费方式 | 完全工具化，不向 system prompt 注入本体上下文 |
| 6 | 事件与动作 | 动作 = 现有 commands 平移改名；事件新增定义，上报走现有 event 管线，告警可订阅 |
| 7 | 知识形态 | 文档挂载 + LLM 生成物的知识摘要（ontology_summary） |
| 8 | 交付范围 | 全量改造：后端泛化 + 前端改名 + 图谱拆除，一次到位（单一大分支，分支内按逻辑阶段提交） |
| 9 | 层级与分类 | parent_id 管归属（单父树），tags 管切面分类（扁平多对多） |
| 10 | API | 物的管理面一刀切只留 `/api/things`（含 `open/`、`mcp/` 同步切 thing 语义，OV4 裁决全破）；运行时数据面（遥测 ingest、网关协议端点）不受影响 |

扩展决策（CEO 审查 2026-07-22 接受）：

| # | 扩展项 | 结论 |
|---|--------|------|
| E1 | thing_templates 上架 marketplace | 接受，本期交付 |
| E2 | A2UI 本体驱动渲染（DeviceCard/DataChart/ControlPanel） | 接受，本期交付 |
| E3 | DTDL / WoT Thing Description 导入导出 | 接受，本期交付 |

## 〇、现有 Schema 对账（-brownfield 事实）

以下为对生产 SQLite schema 核实后的事实，设计必须与之对账，而非按绿地假设：

**devices 表现状**（已存在，非新增）：`name TEXT NOT NULL UNIQUE`（全局唯一）、`device_type`、`parent_id`（FK → devices(id) `ON DELETE SET NULL`）、`product_id`（FK → products）、`organization_id`（FK → organizations）、`state INTEGER NOT NULL DEFAULT 0`、`driver_name`/`protocol_type` 可空。

各现有列的命运：

| 现有列/表 | 命运 |
|---|---|
| `device_type` | 保留，降级为自由文本"子类型"标签（如 sensor/gateway），不再承担分类主语义；`thing_type` 是新的主分类 |
| `parent_id` | 语义沿用但升级为本体层级；FK 约束从 `ON DELETE SET NULL` 改为 `RESTRICT`（配合"删有子节点的物拒绝"规则） |
| `product_id` / products 表 | **本期收敛（用户裁决 2026-07-22）**：products 表删除，`devices.product_id` → `devices.template_id`（FK → thing_templates，`ON DELETE SET NULL`）。products 是空心型号表（6 行、无 workspace_id、无属性/动作定义、API 路由已移除），其职责完全被 thing_templates 覆盖。顺带补上原 spec 缺口：物此前没有任何列指向模板，template_id 让 ER 图中的 `thing_templates ──< devices` 落到实处。数据重映射：8 台设备按 device_type 匹配内置模板（环境传感器→SHT30 模板等），无匹配的置 NULL（无模型的物是合法状态） |
| `organization_id` / organizations 表 | 保留，分工明确化：organizations 管**行政/权限**维度，parent_id 树管**物理/空间归属**。两者并存不冲突 |
| `name UNIQUE`（全局） | 改为表达式唯一索引 `UNIQUE INDEX ON devices(COALESCE(workspace_id,''), name)`——内置/全局行（workspace_id NULL，现有 16 个内置模板同款情况）按 `''` 参与唯一，避免 SQLite 把 NULL 视为互不相同导致约束失效 |
| `state NOT NULL DEFAULT 0` | 保留；非 device 类型的物固定为 0（无连接态概念）。state 聚合查询**不按 thing_type 过滤**（D5 裁决：链路对全部物开放），非 device 物以 0 自然参与聚合 |
| `workspace_id ON DELETE SET NULL`（devices） | 语义变更：工作区删除改为**应用层拒绝**（含物时须先迁移/删除其物），不再依赖 SET NULL 产生"既不在唯一约束内、也按名查不到"的孤儿物 |

**tags 现状**：`tags.type` 有 `CHECK (type IN ('device','app'))`，`UNIQUE(type, name)` 是全局的（跨租户撞名），`tag_bindings.target_type` 为自由文本。方案：CHECK 放宽加 `'thing'`；唯一约束改为表达式索引 `COALESCE(tenant_id,'') + type + name`；新的物标签绑定用 `target_type='thing'`，查询时兼容 `IN ('device','thing')`，存量绑定不动。

**resources 表现状**（注意：实际表名是 `resources`，不是 workspace_resources）：`workspace_id NOT NULL`、`file_path NOT NULL DEFAULT ''`、`parse_status`（与将被删除的知识解析管线关联）、`tags TEXT`。迁移方案：全量行迁移到 `thing_resources`；`parse_status` 列不迁移（解析管线随图谱删除而消亡）；部署时在途解析任务随进程重启消亡，可接受。

**device_templates 现状**：`name` 同样全局 UNIQUE。E1 模板市场下跨工作区/市场安装必然撞名。方案：唯一约束改为表达式唯一索引 `COALESCE(workspace_id,'') + name`（内置模板 workspace_id 为 NULL，共 16 个，普通 `(workspace_id,name)` 约束对 NULL 无效）；市场安装撞名时自动加后缀（模板名+市场来源）。

**SQLite 约束修改 = 表重建**（必须在迁移计划中显式编排）：SQLite 无法 ALTER CHECK/FK/UNIQUE 约束，以下三项都是"建新表 → 拷数据 → 删旧表 → 改名"：
1. `devices` 重建：name 唯一约束改表达式索引 + parent_id FK 改 RESTRICT + `product_id` 改 `template_id`（FK → thing_templates）。重建最重：12 个索引、4 个外向 FK（parent_id 自引用、template_id、organization_id、workspace_id），且有 **8 张表持有指向 devices 的内向 FK**（device_alarm_rules、device_properties、device_commands、device_alarms、messages、device_traces、jobs 等），拷贝期间必须 `PRAGMA defer_foreign_keys=ON`（sqlx Migrator 把每个迁移包在事务里，`foreign_keys=OFF` 在事务内是 no-op；defer 才是事务内合法的延迟校验开关），commit 前跑 `PRAGMA foreign_key_check`
2. `tags` 重建：CHECK 放宽加 `'thing'`
3. `device_templates` 重建（改名 thing_templates 时一并完成）：name 唯一约束改表达式索引 `COALESCE(workspace_id,'')+name`

**name 查找作用域变更的爆炸半径**：`find_by_name`/`get_device_by_name` 被 role、permission、template、device（service/properties handler）、gateway 等 10+ 处使用。方案：
- 所有 name 查找改为 workspace 作用域（`find_by_name(workspace_id, name)`）
- 按名读属性的 HTTP 路由迁移到 `/api/things/{id}/properties` 体系（按名查询保留为 workspace 内解析）
- 网关自动注册（hostname 作为 name）：查找改为该网关所属 workspace 作用域；撞名行为维持现状语义（已存在则复用挂载，不新建）

**device_alarm_rules 现状**（与初审猜测不同，已核实）：`property_id` **本来就可空**（全属性规则），`rule_type` **无 CHECK 约束**，`alarm_level` 为 4 级（info/warning/error/critical）。因此事件触发源只需：新增 `rule_type='event'` 的代码支持 + `condition_config` 的事件条件 schema（`{event_name, min_level}`），无需表结构迁移。

**级别枚举统一**：事件级别采用告警的 4 级枚举 `info/warning/error/critical`（原 spec 的 3 级作废）。

**事件体系统一**（外部声音 OV1 用户裁决，已核实三张表全部 0 行）：repo 现存两套事件体系——`events`（append-only 日志，event_repository_impl 两个写入点）与 event service 的 `real_time_events`（当前态去重视图，唯一写入方 real_time_status_handler 的 upsert_status，含 acknowledged/occurrence_count 确认流）+ `lost_events`（**零代码写入方的死表**，仅迁移文件存在）+ `event_performance_metrics`（real_time 触发器写入）。本期统一为 events 单表体系：
- `events` 表吸收当前态能力：加列 `occurrence_count INTEGER DEFAULT 1`、`acknowledged BOOLEAN DEFAULT 0`、`acknowledged_by TEXT`、`acknowledged_at TEXT`，加去重表达式索引支撑 upsert 语义（source 维度：`event_type+event_subtype+device_id`）
- `real_time_events` / `lost_events` / `event_performance_metrics` 三表删除；ack API（modules/event/handler/real_time.rs）与 real_time_status_handler 改读写 events 表
- 物事件路由函数为**唯一写入入口**：append 日志语义与 upsert 当前态语义由路由按事件性质分发（状态类事件 upsert，occurrence_count 累加；发生类事件纯 append）
- AI 子系统的 DLQ 需求（TODOS P1）不复用 lost_events，走独立 DeadLetterQueue 设计

## 一、核心概念与数据模型

### 物（Thing）

devices 表原地泛化，新增列：

```
devices（泛化为物，新增部分）
├── thing_type: 'device' | 'space' | 'line' | 'building' | ...（默认 'device'，存量行 backfill 'device'）
├── ontology_summary: LLM 生成的知识摘要（TEXT，可空）
└── summary_status: 'ok' | 'pending' | 'failed'（可空）
```

- 链路透视（工程评审 D5 用户裁决）：心跳/告警/监控等链路**对全部物开放，不加 thing_type='device' 过滤**——车间也可有属性、事件与告警；仅真正设备专属的能力（driver 连接、动作下发 invoke_action）限定 device 类型（非 device 调动作返回"该物不支持动作"）
- 层级与分类分工：parent_id 管"归属"（唯一、树形），tags 管"切面"（扁平标签）
- 面包屑路径：查询时沿 parent_id 递归上溯，深度上限 10（防环兜底，成环在写入侧已被拒绝）
- 原图谱的 monitors/manages 等语义关系本期丢弃；将来需要再加轻量 `thing_links` 表（YAGNI）

**全物行为审计清单**（D5 裁决后语义反转——不是"加 device 过滤"，而是核查每个模块对非 device 物的行为是否符合预期）：batch、heartbeat、gateway、monitoring、driver_health、marketplace、`open/`、`mcp/`、alarm、event。重点核查项：心跳对无连接态的物是否跳过、driver_health 是否只看有 driver 的物、告警/监控对空间类物是否正确展示。

### 物模型（Thing Model）= 模板层结构定义

`device_templates` 改名 `thing_templates`，代码内 `DeviceTemplate` → `ThingTemplate`：

| 要素 | 来源 | 说明 |
|---|---|---|
| Property 属性 | 现有 properties 保留 | 可读/可写状态，遥测上报 |
| Action 动作 | 现有 commands 平移改名 | 可调用操作，下发链路复用现有命令通道 |
| Event 事件 | 新增 | 设备主动上报的发生，4 级级别 + 字段 schema |
| Knowledge 知识 | 新增，挂实例 | 文档/资源 + LLM 摘要，模板可提供默认知识 |

模板带 `thing_type`，空间类物也可建模板（如"车间模板"：属性=面积/负责人，事件=人员超限）。

**commands → actions 改名的具体面**（"平移"的精确定义）：
- `device_templates.commands` 列 → `thing_templates.actions`（迁移改列名，列内 JSON 数组结构不变）
- 模板 JSON schema 中 `commands` 键 → `actions`
- 模板导入兼容：导入时接受旧 `commands` 键并映射为 `actions`（导出格式加 `format_version: 2`，旧版文件仍可导入）
- API 字段与前端文案 command → action
- 运行时命令下发内部结构（网关协议、通道）**不改名**——那是传输层，不属于本体语义

### 数据模型 ER

```
thing_templates ──< devices(things) ──< thing_resources (文档/图片/3D)
       │                  │
       │                  └── ontology_summary / summary_status('ok'|'dirty'|'failed')
       │
  properties / actions / events (JSON 定义, 模板层)

物事件实例：复用现有 events 表（不建 thing_events）
  event_type='device', event_subtype=<模板事件名>, event_level=int(2-5),
  device_id=物 id, content=事件 data JSON, metadata.unknown_event=bool
```

- `thing_resources`：id、device_id（FK→物，**可空**，NULL=未指派归属）、**workspace_id NOT NULL**（多租户隔离，未指派资源仍归属工作区）、type（document/image/scene3d）、content 或 file_path、tags
- **未指派是永久的合法状态**（非仅迁移过渡）：有些资源天然暂不归属某个物，前端资源列表持续展示未指派分组并引导指派
- 删除物的级联语义：events 表历史行保留（device_id 成为悬空引用，按时间清理）；`thing_resources.device_id` → `ON DELETE SET NULL`（文档不随物消失，回落为未指派，UI 删除确认中明示）；device_alarm_rules 维持现有 CASCADE 不变
- 删除表：knowledge_entities、knowledge_relations、knowledge_parse_jobs、resources（数据迁移后删）
- **删除 events 表的 1 万行清理触发器**（cleanup_old_events）：物事件进入后上限会很快被打满，本期不设存储上限；保留策略记入 TODOS（首个运维迭代处理）

### 资源与数据迁移

- 知识图谱数据（entities/relations/parse_jobs）：**丢弃**，用户已确认
- `resources` → `thing_resources`：**迁移不丢**。`device_id` 置 NULL（未指派），`parse_status` 列不迁移（解析管线已删），前端资源列表对未指派资源显示提示，引导用户指派到物
- `devices` 存量行：`thing_type` backfill `'device'`

## 二、运行时管线

### ① 事件流（新增）

```
设备上报 → 网关/驱动 → 事件路由 ──┬──→ events 表（event_subtype=事件名，实例存储）
                                 └──→ alarm 模块（rule_type='event' 规则匹配触发）
```

- 上报格式：MQTT topic 约定 `thing/{id}/event/{event_name}`，payload：`{event_name, level, data, ts}`，data 为 JSON object（按模板事件 schema 校验，未知字段保留），ts 为 RFC3339 UTC（缺省由服务端填）
- **ingest 入口（明确两条，工程评审 D2 裁决维持新 topic 约定）**：①平台 MQTT 客户端（shared/mqtt_client.rs）新增订阅 `thing/+/event/+`，消息解析后交事件路由；②driver 直连设备无 MQTT 路径，走进程内调用同一事件路由函数。既有 `tinyiothub/{ws}/gateway/{gw}/event` 通道本期不动（消息仍不处理，属 gateway 模块既有 TODOS 范围，与本管线无关）
- **设备侧契约（本期就做，TODOS 提案用户裁决 C）**：文档化设备/固件事件上报契约（topic 约定、payload schema、4 级枚举、未知事件降级行为、节流语义），并在 `examples/` 下提供参考发布实现——让"真实设备上报"有落地路径，不只平台侧空订阅
- 存储映射（复用现有 events 表，**不建 thing_events、不双写**）：`event_type='device'`、`event_subtype=event_name`、`event_level` int（info=2/warning=3/error=4/critical=5，debug 级不入）、`device_id`=物 id、`workspace_id`=物所属工作区（OV6 裁决新增列）、`content`=data JSON、`metadata.unknown_event`=bool；实时推送走 event 模块既有机制
- `device_event_triggers` 旧触发表：与新 `rule_type='event'` 告警规则概念重叠，本期**废弃**——存量数据不迁移（核实线上无有效使用后直接弃用），代码随图谱拆除一并清理
- 未知事件名降级为 info 级存原始数据，`unknown_event=true`，不报错给设备（固件可能先于模板更新）
- 畸形 payload（非 JSON、缺 event_name/level 字段）：拒绝落库，记 `malformed` 计数与结构化日志（含 topic、物 id、payload 前 200 字符）
- 事件风暴节流：单物 60 条/分钟上限（**分级节流，外部声音 OV3 裁决：只计 info/warning 级；error/critical 豁免直落库**——否则风暴窗口内的 critical 事件被丢弃会使告警订阅失效），超出丢弃并计 `throttled` 计数（不报错给设备，防止固件 bug 洪泛打爆存储）——复用共享 events 表后节流是硬要求
- alarm 规则新增 `rule_type='event'`，`condition_config = {event_name, min_level}`
- **验收标准（防 dead event path 前科）**：真实 MQTT 上报 → events 表落库 → 真实告警触发，全链路集成测试，不接受 mock

### ② 动作下发（commands 平移）

现有命令下发链路不动。模板 actions 定义带参数 schema，Agent 的 invoke_action 按 schema 校验后走现有通道。非 device 类型物调用动作返回明确错误"该物不支持动作"。

### ③ LLM 知识摘要（ontology_summary，懒计算）

摘要只有一个消费者（get_thing 工具）且对陈旧容忍度高，因此**读时计算 + 脏标记**，不建预计算管线：

```
脏标记（summary_status='dirty'）的设置时机：
  · 物的文档资源增/删/内容改 → 该物标 dirty
  · 模板变更 → 该模板所有物标 dirty（一条 UPDATE，不触发计算）
  · 物改名/改父节点 → 该物及整个子树标 dirty（面包屑变了）

读时计算（get_thing / get_thing_profile 调用时）：
  · summary_status='ok' → 直接返回缓存摘要
  · 'dirty' 或摘要为空 → 同步调 LLM 重算（10s 超时），成功写回并返回；
    失败返回旧值（或"该物暂无摘要"），summary_status='failed'
  · 同物并发读时计算用 single-flight 去重

输入：物名称/类型/面包屑路径 + 物模型定义 + 各文档资源前 2000 字符（单物最多拼 5 篇）
输出：≤500 字中文摘要
防注入：文档内容以 <user_document> 围栏包裹进 prompt（沿用知识解析先例），输出写库前做安全过滤
```

被删除的复杂性（外部声音意见，已采纳）：防抖窗口、jobs 队列、扇出并发限流、失败重试状态机、子树级联重算——全部不需要。dirty 标记是廉价的，计算只在有人读时发生。

摘要只服务 Agent 的 get_thing 工具，不进 system prompt。文档只存原文，不再有 LLM 实体/关系解析。

### 可观测性（两条新管线的硬性要求）

- 事件管线：计数指标 `events_ingested` / `events_unknown` / `events_malformed` / `events_throttled`（按物维度可下钻）；结构化日志含 thing_id、event_name、level
- 摘要管线：指标 `summary_success` / `summary_failed` / `summary_duration`；每次重算记录触发原因（资源变更/模板变更/改名/手动）
- 物操作审计：创建/删除/改父/invoke_action 记审计日志（操作者、时间、目标物）

## 三、Agent 工具集

Agent 不注入任何本体上下文，全部通过工具按需获取：

| 工具 | 参数 | 返回 | 说明 |
|---|---|---|---|
| list_things | thing_type?, parent_id?, tags?, q? | 物的扁平列表（id/名称/类型/路径） | 发现有哪些物 |
| get_thing | thing_id | 面包屑路径、tags、ontology_summary、物模型定义 | 轻量，"这个物是什么、能做什么" |
| get_thing_profile | thing_id | get_thing 全部 + 各属性当前值（含时间戳）+ 最近 10 条事件 + 知识文档列表（不含正文） | 聚合快照，一次拿全 |
| get_thing_tree | root_id?, depth? | 树形结构（仅 id/名称/类型），默认深度 3 | 全局视野 |
| read_property | thing_id, property_name | 当前值 + 时间戳 | 读 `app_state.device_cache`（既有缓存服务，mcp/tools/device.rs:158 同款路径）；无缓存返回 null + 提示 |
| invoke_action | thing_id, action_name, params | 下发结果/异步任务 id | schema 校验；非 device 类型报错 |
| query_events | thing_id, event_name?, level?, since?, limit | 事件实例列表 | 查 events 表（event_subtype=事件名过滤） |
| search_knowledge | thing_id?, q, tags?, limit | 命中文档列表（标题/所属物/片段） | 全文检索 thing_resources |
| read_document | resource_id | 文档正文 | 按需取全文 |

配套变更：
- 删除现有工具：agent/tools/knowledge.rs（图谱版）、search_resources.rs
- invoke_action 加工作区级开关 `require_action_confirm`（默认开）——存储为 workspaces 表新增列 `require_action_confirm BOOLEAN DEFAULT 1`（工程评审 D6 裁决，跟随 heartbeat_trust_config 列先例，不放 agent_config JSON）
- 列表类工具（list_things/search_knowledge/query_events）统一分页：limit 默认 50、最大 200
- 工具描述用中文写清"什么时候用哪个工具"
- 移除 agent system prompt 的 build_context 注入逻辑

### 代码归属

新建 `cloud/src/modules/thing` 模块承载管理面（物 CRUD/层级/本体/资源/摘要管线触发），遵循 handler/service/repo/types 分层；device 模块只保留连接运行时（驱动/遥测/心跳）。Agent 工具调 thing 模块。

## 四、图谱拆除与改名范围

### 后端拆除

- `workspace/types/knowledge.rs`、`workspace/service/knowledge.rs`、`workspace/repo/knowledge.rs`、`workspace/handler/knowledge.rs`
- `agent/tools/knowledge.rs`、`agent/tools/search_resources.rs`
- DB 表：knowledge_entities、knowledge_relations、knowledge_parse_jobs、resources（迁移完成后）、real_time_events、lost_events、event_performance_metrics（事件体系统一，OV1 裁决）

### 改名与 API 边界

- DB：devices 表名保留；device_templates → thing_templates
- 现有 `/api/devices/**` 路由逐条处置：

| 路由类别 | 处置 |
|---|---|
| 物 CRUD / 列表 / 详情（管理面） | 删除，由 `/api/things` 取代 |
| 按名读属性等管理形读取 | 迁移到 `/api/things/{id}/...`，按名解析改为 workspace 作用域 |
| 遥测 ingest / 心跳 / 网关协议端点（运行时数据面） | 不动，不是管理 API |
| `open/`、`mcp/` 对外接口 | **同步切 thing 语义，对外契约一起破**（外部声音 OV4 用户裁决：全破，不搞兼容层）——open/ 设备端点改 things 语义，mcp device 工具改 thing 工具，`examples/bacnet-driver` 等调用方同步改新端点 |

- 代码：template module 内 DeviceTemplate → ThingTemplate；device module 名保留

### 前端（web/）

**设计系统约束**（设计评审 D10 裁决）：实现复用 `home.css` 既有 tokens（`--home-bg-deep` 深底、cyan `#00d4ff` 主强调、Noto Sans SC 字体）与 `web/src/ui/views/` 手写 TS 视图模式——不引入新组件库、新字体、新色系；新组件沿用既有 glassmorphic 暗色管理台风格。

**响应式与无障碍最小集**（设计评审 D11 裁决）：①桌面优先，不承诺移动端；窄屏下表格与树视图横向滚动兜底，不破版；②所有状态色点（在线点、知识灰/绿点、事件级别色点）必须同时配文字标签，不纯色编码；③确认弹窗 Esc=取消、Enter=确认，打开时焦点圈定在弹窗内；④按钮/行/树节点等交互元素有可见 hover 与 focus 态。

**交互状态表**（设计评审 D7 裁决，描述用户看到的，不是后端行为）：

| 功能 | 加载中 | 空 | 错误 | 成功 | 部分 |
|---|---|---|---|---|---|
| 物列表/树 | 骨架行 5 条 | "还没有物——创建第一个物"主按钮+一句话引导 | 错误条+重试按钮 | 表格渲染 | 过滤无结果="无匹配，清除过滤"链接 |
| 详情页-概览 | 卡片骨架 | 无属性="该物暂无属性上报"灰字 | 摘要卡显示上次成功摘要+失败徽标（stale 降级，不阻塞整页） | 全卡片渲染 | 部分属性无值=该格显"—" |
| 详情页-事件 | 时间线骨架 | "暂无事件——配置事件上报"文档链接 | 错误条+重试 | 时间线渲染 | 未知事件条目带"未知事件"徽标 |
| 详情页-动作 | 按钮组骨架 | "该物无可用动作"（无模板/非 device） | 下发失败 toast+错误详情 | 下发成功 toast（异步任务 id 可复制） | 部分参数校验失败=行内红字定位字段 |
| 详情页-知识 | 文档列表骨架 | "还没有知识文档——上传第一篇"主按钮+未指派横幅 | 摘要失败="摘要生成失败，稍后自动重试"提示，文档正文不受影响 | 列表+摘要渲染 | 部分文档解析失败=该条带警告徽标 |
| 模板编辑 | 段骨架 | 空段="添加第一个属性/事件/动作"行内按钮 | 保存失败 toast+未保存标记保留 | 保存成功 toast | 部分段校验失败=Tab 上红点定位 |
| A2UI 三组件 | 组件骨架屏 | 无数据="暂无数据"卡片内提示 | **渲染失败降级为 JSON 原文折叠块**（不白屏） | 本体驱动渲染 | 物模型缺段=对应组件不渲染并注明原因 |
| 确认弹窗（invoke_action） | — | — | 令牌过期="确认已超时，请重新发起" | 确认后显示下发进度 | — |

**关键用户旅程**（设计评审 D8 裁决，STEP | 用户做什么 | 用户感受 | spec 落点）：

| 旅程 | 步骤与感受 | 落点裁决 |
|---|---|---|
| 升级老用户首登 | 导航"设备"变"物"→"我的东西还在吗？" | 物列表页顶部一次性提示条："设备已升级为物，全部数据已迁移"，可关闭且关闭后不再显示（localStorage 标记） |
| 新建第一个物 | 空态主按钮→建完→"接下来看哪？" | 创建成功后直接跳转该物详情页概览 Tab（不停留列表页） |
| 首次动作确认弹窗 | 被弹窗打断→"为什么弹我？以后还弹吗？" | 弹窗文案固定含一句："可在工作区设置中关闭动作确认"，附设置入口链接 |
| 挂文档后等摘要 | 等待最长 10s→"卡住了还是在算？" | 摘要计算中超 3s 显示"AI 正在生成摘要…"进度文案（非裸骨架），10s 超时走 stale 降级 |

- 导航与文案："设备" → "物"，设备列表页变为物列表（类型过滤 + 树视图）。**改名范围**（设计评审 D14 裁决）：导航项、页面标题、面包屑、按钮文案全改，前端路由 `/devices` → `/things` 同步改，旧 URL 302 重定向；全局文案一次扫库替换，不留半改状态
- **物列表页形态**（设计评审 D3 裁决）：单页两视图切换——页面顶部视图切换「列表｜树」，列表视图=类型过滤+搜索+批量操作的表格，树视图=全量层级树（默认展开 2 层，当前工作区根起）；两视图共享同一份过滤条件，切换不丢上下文。**树交互**（设计评审 D12 裁决）：单击节点=直接进入该物详情页，展开/收起由节点旁箭头独立承担；换父支持拖拽——拖到成环目标时实时红框+提示拒绝（不落库），合法落点松手即调更新 API，服务端成环校验兜底
- 新增物详情页，Tab：概览｜属性｜事件｜动作｜知识
- **确认弹窗形态**（设计评审 D13 裁决）：居中 modal——标题=动作名，副题=目标物名，主体=参数键值表（只读），底部取消/确认按钮，danger 类动作确认钮红色；实现为单一通用组件，所有动作复用
- **概览 Tab 首屏层级**（设计评审 D4 裁决）：第一眼=头部条（面包屑层级路径 + 物名称 + 类型徽标 + 在线状态点）+ AI 本体摘要卡（置顶，带"AI 生成"徽标与摘要时间）；第二眼=关键属性实时值网格（大数字+单位+上报时间戳）；第三眼=最近事件时间线（级别色点）+ 快捷动作按钮组
- 模板管理页：属性/事件/动作三段编辑（设计评审 D5 裁决：**编辑器顶部 Tab 分三段**，每段独立全宽表格+行内编辑；跨段参照（事件载荷引用属性字段）通过段内只读摘要条提示，无需跳转）
- 删除：知识图谱管理页、workspace 资源管理页（并入物的"知识"Tab；未指派资源提示形式——设计评审 D6 裁决：**物列表"知识"列灰点（未挂载）/绿点（已挂载）徽标 + 知识 Tab 顶部常驻"N 篇文档未指派到任何物"横幅（含一键指派入口）**）

## 五、扩展项（E1/E2/E3）

- **E1 模板市场**：marketplace 模块增加 thing_templates 类目，上架包含属性/事件/动作/默认知识；安装即建模板
- **E2 A2UI 本体驱动渲染**：get_thing_profile 返回结构驱动 DeviceCard/DataChart/ControlPanel；invoke_action 前渲染确认面板。依赖 A2UI 渲染侧既有能力，落地时先验证渲染成熟度
- **E3 标准互通**：物模型支持 DTDL / WoT Thing Description 导入导出（导入映射为模板定义，导出生成标准文档）

## 六、错误处理

- 未知事件名：降级 info + unknown_event 标记，不报错给设备
- 动作下发：非 device 类型 → 明确错误；参数不符 schema → 校验明细；设备离线 → 复用现有命令通道行为
- 摘要管线：LLM 失败重试 3 次 → summary_status='failed'；get_thing 遇摘要为空返回"该物暂无摘要"
- 层级约束：parent_id 成环写入拒绝（应用层检测）；删除有子节点的物拒绝（FK RESTRICT + 应用层提示）
- 遥测读取：无缓存值返回 null + "该属性暂无上报数据"

## 七、迁移顺序（预发布直接重建，含 SQLite 表重建编排）

生产库现状：8 设备 / 3 资源 / 0 知识实体——预发布阶段数据量极小，**不做 expand/migrate/contract 分阶段迁移**，启动时一次重建到位。

0. **备份**：迁移前自动 `cp` SQLite 文件到 `data/backups/`（带时间戳）；迁移在启动时、服务开放前完成，失败则中止启动并提示从备份恢复
1. **表重建**（每张都是 建新表→拷数据→删旧表→改名，拷贝期间 `PRAGMA defer_foreign_keys=ON`（事务内合法，见〇节），commit 前 `PRAGMA foreign_key_check`）：
   - devices：name 唯一约束改表达式索引 `COALESCE(workspace_id,'')+name` + parent_id FK 改 RESTRICT（注意内向 FK：device_alarm_rules/device_properties/device_commands 等 8 张表）+ 加列 thing_type/ontology_summary/summary_status
   - tags：CHECK 放宽加 `'thing'` + 唯一约束改表达式索引 `COALESCE(tenant_id,'')+type+name`
   - device_templates → thing_templates：改名的同时 name 唯一约束改表达式索引 `COALESCE(workspace_id,'')+name`
   - resources → thing_resources：加 workspace_id NOT NULL（回填所属工作区）、device_id 允许 NULL（未指派为合法状态）、删 parse_status 列
   - events：删 cleanup_old_events 触发器（不设保留上限，保留策略记入 TODOS）；加列 occurrence_count/acknowledged/acknowledged_by/acknowledged_at + source 去重表达式索引（事件体系统一，OV1 裁决）；加列 `workspace_id NOT NULL`（OV6 裁决，回填自 devices.workspace_id，device_id 悬空行回填其最近工作区或中止提示）
   - real_time_events / lost_events / event_performance_metrics：三表删除（全部 0 行）；real_time.rs ack API 与 real_time_status_handler 改读写 events
   - device_alarm_rules：rule_type 支持 `'event'`，condition_config 增 `{event_name, min_level}` 形态
   - workspaces：加列 `require_action_confirm BOOLEAN DEFAULT 1`（invoke_action 确认开关，D6 裁决）
2. **Backfill**：devices.thing_type='device'；devices.template_id 按 device_type 从 product_id 重映射（无匹配置 NULL）；thing_resources.workspace_id 从原 resources.workspace_id 平移；products 表删除（随 devices 重建同批完成）
3. **Deploy**：代码全量上线（工具集/管线/前端），name 查找全部 workspace 作用域化；同分支删除 knowledge_* 表与图谱代码

## 八、测试策略

- 单元：物模型 schema 校验、成环检测、摘要输入拼装（2000 字符/5 篇截断）、事件降级（未知事件名）、事件节流窗口计数（60/min/物）、畸形 payload 拒收、导入兼容（旧 commands 键）
- 集成（sqlx 真实 DB，禁 mock-only）：
  - 事件全链路：真实 MQTT 上报 → events 表落库 → rule_type='event' 告警触发；连续 61 条 info 上报断言第 61 条被节流丢弃且 metric 计数 +1；风暴窗口内 critical 事件豁免直落库（OV3）
  - 摘要管线：挂文档 → 触发 → mock LLM（仅 LLM 可 mock）→ 摘要写回；LLM 超时 → 返回 stale 摘要 + summary_status='failed'；并发读 single-flight 去重；template 变更 → 引用实例批量标脏；改名/换父 → 子树标脏
  - 迁移：name 冲突场景、resources 迁移、RESTRICT 删除拒绝、workspaces.require_action_confirm 默认 1
  - Agent 工具：9 个工具的参数校验与返回结构（含列表分页边界：默认 50、上限 200、越界 clamp）；invoke_action 确认流（开关开→返回待确认令牌→确认后下发；开关关→直接下发；非 device 物→"该物不支持动作"）
- 拆除验证：图谱 API 404、旧表不存在
- 全物行为（D5）：无连接态 space 物 → 心跳跳过、告警规则可挂且可触发、state 聚合不报错（集成）
- 事件体系统一（OV1）：状态类事件 upsert（occurrence_count 累加 + 去重索引）、ack 流（acknowledge → acknowledged_by/at 写回 events）、real_time_events/lost_events/event_performance_metrics 三表不存在（集成）
- 扩展项：E1 市场撞名加后缀安装（集成）；E3 DTDL/WoT 导出→导入 round-trip + Azure 模型库样例导入（集成）
- 前端：物详情页各 Tab、树视图、未指派资源提示、E2 A2UI 渲染真实物数据（手测）

## 九、风险登记

- R1：mega-PR 评审与回滚成本（用户知情接受；缓解=分支内按逻辑阶段提交，先落地 fix/ai-deep-review 再开工）
- R2：dead event path 前科——事件管线验收必须真实全链路（见二·①）
- R3：mock 测试掩盖真实 DB 问题——集成测试必须 sqlx 真实 DB（见八）
- R4：知识图谱两个月即拆的前车之鉴——本体落地后需尽快跑真实 Agent 任务验证
- R5：~~products 与 thing_templates 概念重叠遗留~~ **已关闭（用户裁决 2026-07-22）**：本期收敛，products 表删除、product_id → template_id（见〇节对账表）
