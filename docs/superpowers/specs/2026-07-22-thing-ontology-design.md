# 设备本体中心化设计（Thing Ontology）

> 日期：2026-07-22（v2，经对抗性审查修订）
> 状态：已确认
> 背景：现有 AI 功能中 workspace 级知识图谱与资源设计偏离方向。IoT 场景更适合"本体智能"——知识定义下沉到物，围绕物本体组织全部 AI 功能。

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
| 10 | API | 物的管理面一刀切只留 `/api/things`；运行时数据面（遥测 ingest、网关协议端点）不受影响 |

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
| `product_id` / products 表 | 本期保留不动。products 与 thing_templates 概念重叠（都是"型号"），后续迭代评估合并，记入 TODOS |
| `organization_id` / organizations 表 | 保留，分工明确化：organizations 管**行政/权限**维度，parent_id 树管**物理/空间归属**。两者并存不冲突 |
| `name UNIQUE`（全局） | 改为表达式唯一索引 `UNIQUE INDEX ON devices(COALESCE(workspace_id,''), name)`——内置/全局行（workspace_id NULL，现有 16 个内置模板同款情况）按 `''` 参与唯一，避免 SQLite 把 NULL 视为互不相同导致约束失效 |
| `state NOT NULL DEFAULT 0` | 保留；非 device 类型的物固定为 0。所有按 state 聚合的查询必须过滤 `thing_type='device'`（见审计清单） |
| `workspace_id ON DELETE SET NULL`（devices） | 语义变更：工作区删除改为**应用层拒绝**（含物时须先迁移/删除其物），不再依赖 SET NULL 产生"既不在唯一约束内、也按名查不到"的孤儿物 |

**tags 现状**：`tags.type` 有 `CHECK (type IN ('device','app'))`，`tag_bindings.target_type` 为自由文本。方案：CHECK 放宽加 `'thing'`；新的物标签绑定用 `target_type='thing'`，查询时兼容 `IN ('device','thing')`，存量绑定不动。

**resources 表现状**（注意：实际表名是 `resources`，不是 workspace_resources）：`workspace_id NOT NULL`、`file_path NOT NULL DEFAULT ''`、`parse_status`（与将被删除的知识解析管线关联）、`tags TEXT`。迁移方案：全量行迁移到 `thing_resources`；`parse_status` 列不迁移（解析管线随图谱删除而消亡）；部署时在途解析任务随进程重启消亡，可接受。

**device_templates 现状**：`name` 同样全局 UNIQUE。E1 模板市场下跨工作区/市场安装必然撞名。方案：唯一约束改为表达式唯一索引 `COALESCE(workspace_id,'') + name`（内置模板 workspace_id 为 NULL，共 16 个，普通 `(workspace_id,name)` 约束对 NULL 无效）；市场安装撞名时自动加后缀（模板名+市场来源）。

**SQLite 约束修改 = 表重建**（必须在迁移计划中显式编排）：SQLite 无法 ALTER CHECK/FK/UNIQUE 约束，以下三项都是"建新表 → 拷数据 → 删旧表 → 改名"：
1. `devices` 重建：name 唯一约束改表达式索引 + parent_id FK 改 RESTRICT。重建最重：12 个索引、4 个外向 FK（parent_id 自引用、product_id、organization_id、workspace_id），且有 **8 张表持有指向 devices 的内向 FK**（device_alarm_rules、device_properties、device_commands、device_alarms、messages、device_traces、jobs 等），拷贝期间必须 `PRAGMA foreign_keys=OFF`，重建后恢复并跑 `PRAGMA foreign_key_check`
2. `tags` 重建：CHECK 放宽加 `'thing'`
3. `device_templates` 重建（改名 thing_templates 时一并完成）：name 唯一约束改 `(workspace_id, name)`

**name 查找作用域变更的爆炸半径**：`find_by_name`/`get_device_by_name` 被 role、permission、template、device（service/properties handler）、gateway 等 10+ 处使用。方案：
- 所有 name 查找改为 workspace 作用域（`find_by_name(workspace_id, name)`）
- 按名读属性的 HTTP 路由迁移到 `/api/things/{id}/properties` 体系（按名查询保留为 workspace 内解析）
- 网关自动注册（hostname 作为 name）：查找改为该网关所属 workspace 作用域；撞名行为维持现状语义（已存在则复用挂载，不新建）

**device_alarm_rules 现状**（与初审猜测不同，已核实）：`property_id` **本来就可空**（全属性规则），`rule_type` **无 CHECK 约束**，`alarm_level` 为 4 级（info/warning/error/critical）。因此事件触发源只需：新增 `rule_type='event'` 的代码支持 + `condition_config` 的事件条件 schema（`{event_name, min_level}`），无需表结构迁移。

**级别枚举统一**：事件级别采用告警的 4 级枚举 `info/warning/error/critical`（原 spec 的 3 级作废）。

## 一、核心概念与数据模型

### 物（Thing）

devices 表原地泛化，新增列：

```
devices（泛化为物，新增部分）
├── thing_type: 'device' | 'space' | 'line' | 'building' | ...（默认 'device'，存量行 backfill 'device'）
├── ontology_summary: LLM 生成的知识摘要（TEXT，可空）
└── summary_status: 'ok' | 'pending' | 'failed'（可空）
```

- 设备链路（驱动/网关/遥测/心跳/告警）全部加 `thing_type='device'` 过滤，行为不变
- 层级与分类分工：parent_id 管"归属"（唯一、树形），tags 管"切面"（扁平标签）
- 面包屑路径：查询时沿 parent_id 递归上溯，深度上限 10（防环兜底，成环在写入侧已被拒绝）
- 原图谱的 monitors/manages 等语义关系本期丢弃；将来需要再加轻量 `thing_links` 表（YAGNI）

**state 聚合审计清单**（以下模块引用 devices，需逐一核查 `thing_type` 过滤）：batch、heartbeat、gateway、monitoring、driver_health、marketplace、`open/`、`mcp/`、alarm、event。

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
thing_templates ──< devices(things) ──< thing_resources (文档/图片/3D, device_id 外键, 可空=未指派)
       │                  │
       │                  └── ontology_summary / summary_status
       │
  properties / actions / events (JSON 定义, 模板层)

thing_events: id, device_id, event_name, level(4级), data(JSON), unknown_event(bool), ts(RFC3339 UTC)
  索引: (device_id, ts DESC)
```

- `thing_resources`：id、device_id（FK→物，**可空**，NULL=未指派归属）、type（document/image/scene3d）、content 或 file_path、tags
- **未指派是永久的合法状态**（非仅迁移过渡）：有些资源天然暂不归属某个物，前端资源列表持续展示未指派分组并引导指派
- 删除物的级联语义：`thing_events` → `ON DELETE CASCADE`（物删除后事件历史无意义）；`thing_resources.device_id` → `ON DELETE SET NULL`（文档不随物消失，回落为未指派，UI 删除确认中明示）；device_alarm_rules 维持现有 CASCADE 不变
- 删除表：knowledge_entities、knowledge_relations、knowledge_parse_jobs、resources（数据迁移后删）

### 资源与数据迁移

- 知识图谱数据（entities/relations/parse_jobs）：**丢弃**，用户已确认
- `resources` → `thing_resources`：**迁移不丢**。`device_id` 置 NULL（未指派），`parse_status` 列不迁移（解析管线已删），前端资源列表对未指派资源显示提示，引导用户指派到物
- `devices` 存量行：`thing_type` backfill `'device'`

## 二、运行时管线

### ① 事件流（新增）

```
设备上报 → 网关/驱动 → 事件路由 ──┬──→ thing_events 表（实例存储，供 Agent/前端查询）
                                 ├──→ event 模块现有管线（实时推送、概览）
                                 └──→ alarm 模块（rule_type='event' 规则匹配触发）
```

- 上报格式：MQTT topic 约定 `thing/{id}/event/{event_name}`，payload：`{event_name, level, data, ts}`，data 为 JSON object（按模板事件 schema 校验，未知字段保留），ts 为 RFC3339 UTC（缺省由服务端填）
- 级别映射：thing events 用 4 级文本枚举；现有 event 模块（events 表/EventLevel）为整数级别含 Debug。映射：info/warning/error/critical 直映射对应整数值，**debug 级不入 thing_events**（属诊断噪声）
- `device_event_triggers` 旧触发表：与新 `rule_type='event'` 告警规则概念重叠，本期**废弃**——存量数据不迁移（核实线上无有效使用后直接弃用），代码随图谱拆除一并清理
- 未知事件名降级为 info 级存原始数据，`unknown_event=true`，不报错给设备（固件可能先于模板更新）
- alarm 规则新增 `rule_type='event'`，`condition_config = {event_name, min_level}`
- **验收标准（防 dead event path 前科）**：真实 MQTT 上报 → thing_events 落库 → 真实告警触发，全链路集成测试，不接受 mock
- 事件存储本期不设保留上限；保留策略记入 TODOS（首个运维迭代处理）

### ② 动作下发（commands 平移）

现有命令下发链路不动。模板 actions 定义带参数 schema，Agent 的 invoke_action 按 schema 校验后走现有通道。非 device 类型物调用动作返回明确错误"该物不支持动作"。

### ③ LLM 知识摘要（ontology_summary）

```
触发时机（异步任务，不阻塞主流程）：
  · 物的文档资源增/删/内容改 → 重算
  · 模板变更（属性/事件/动作定义改）→ 该模板所有物重算
  · 物改名或改父节点（面包屑是摘要输入）→ 重算，**并级联重算整个子树**（后代的面包屑也变了），去抖合并为一次子树任务
  · 手动重新生成

输入：物名称/类型/面包屑路径 + 物模型定义 + 各文档资源前 2000 字符（单物最多拼 5 篇）
输出：≤500 字中文摘要，写回 devices.ontology_summary
防抖：同一物 60s 内多次触发只算一次；失败重试 3 次后 summary_status='failed'
```

摘要只服务 Agent 的 get_thing 工具，不进 system prompt。文档只存原文，不再有 LLM 实体/关系解析。

## 三、Agent 工具集

Agent 不注入任何本体上下文，全部通过工具按需获取：

| 工具 | 参数 | 返回 | 说明 |
|---|---|---|---|
| list_things | thing_type?, parent_id?, tags?, q? | 物的扁平列表（id/名称/类型/路径） | 发现有哪些物 |
| get_thing | thing_id | 面包屑路径、tags、ontology_summary、物模型定义 | 轻量，"这个物是什么、能做什么" |
| get_thing_profile | thing_id | get_thing 全部 + 各属性当前值（含时间戳）+ 最近 10 条事件 + 知识文档列表（不含正文） | 聚合快照，一次拿全 |
| get_thing_tree | root_id?, depth? | 树形结构（仅 id/名称/类型），默认深度 3 | 全局视野 |
| read_property | thing_id, property_name | 当前值 + 时间戳 | 读遥测最新值缓存；无缓存返回 null + 提示 |
| invoke_action | thing_id, action_name, params | 下发结果/异步任务 id | schema 校验；非 device 类型报错 |
| query_events | thing_id, event_name?, level?, since?, limit | 事件实例列表 | 查 thing_events |
| search_knowledge | thing_id?, q, tags?, limit | 命中文档列表（标题/所属物/片段） | 全文检索 thing_resources |
| read_document | resource_id | 文档正文 | 按需取全文 |

配套变更：
- 删除现有工具：agent/tools/knowledge.rs（图谱版）、search_resources.rs
- invoke_action 加工作区级开关 `require_action_confirm`（默认开）
- 工具描述用中文写清"什么时候用哪个工具"
- 移除 agent system prompt 的 build_context 注入逻辑

## 四、图谱拆除与改名范围

### 后端拆除

- `workspace/types/knowledge.rs`、`workspace/service/knowledge.rs`、`workspace/repo/knowledge.rs`、`workspace/handler/knowledge.rs`
- `agent/tools/knowledge.rs`、`agent/tools/search_resources.rs`
- DB 表：knowledge_entities、knowledge_relations、knowledge_parse_jobs、resources（迁移完成后）

### 改名与 API 边界

- DB：devices 表名保留；device_templates → thing_templates
- 现有 `/api/devices/**` 路由逐条处置：

| 路由类别 | 处置 |
|---|---|
| 物 CRUD / 列表 / 详情（管理面） | 删除，由 `/api/things` 取代 |
| 按名读属性等管理形读取 | 迁移到 `/api/things/{id}/...`，按名解析改为 workspace 作用域 |
| 遥测 ingest / 心跳 / 网关协议端点（运行时数据面） | 不动，不是管理 API |
| `open/`、`mcp/` 对外接口 | 本期保持向后兼容，列入审计清单 |

- 代码：template module 内 DeviceTemplate → ThingTemplate；device module 名保留

### 前端（web/）

- 导航与文案："设备" → "物"，设备列表页变为物列表（类型过滤 + 树视图）
- 新增物详情页，Tab：概览｜属性｜事件｜动作｜知识
- 模板管理页：属性/事件/动作三段编辑
- 删除：知识图谱管理页、workspace 资源管理页（并入物的"知识"Tab；未指派资源在列表提示）

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

## 七、迁移顺序（expand → migrate → contract，含 SQLite 表重建编排）

1. **Expand**：建新表 thing_events / thing_resources；devices 加列（thing_type/ontology_summary/summary_status）
2. **表重建**（每张都是 建新表→拷数据→删旧表→改名，拷贝期间 `PRAGMA foreign_keys=OFF`，完成后恢复并 `PRAGMA foreign_key_check`）：
   - devices：name 唯一约束改 `(workspace_id, name)` + parent_id FK 改 RESTRICT（注意内向 FK：device_alarm_rules/device_properties/device_commands）
   - tags：CHECK 放宽加 `'thing'`
   - device_templates → thing_templates：改名的同时 name 唯一约束改 `(workspace_id, name)`
3. **Migrate**：devices backfill thing_type='device'；resources → thing_resources（device_id=NULL，不迁 parse_status）
4. **Deploy**：代码全量上线（工具集/管线/前端），name 查找全部 workspace 作用域化
5. **Contract**：删 knowledge_* 与 resources 表、删图谱代码（与 Deploy 同分支，靠合并顺序保证）

## 八、测试策略

- 单元：物模型 schema 校验、成环检测、摘要输入拼装（2000 字符/5 篇截断）、事件降级、导入兼容（旧 commands 键）
- 集成（sqlx 真实 DB，禁 mock-only）：
  - 事件全链路：真实 MQTT 上报 → thing_events 落库 → rule_type='event' 告警触发
  - 摘要管线：挂文档 → 触发 → mock LLM（仅 LLM 可 mock）→ 摘要写回
  - 迁移：name 冲突场景、resources 迁移、RESTRICT 删除拒绝
  - Agent 工具：9 个工具的参数校验与返回结构
- 拆除验证：图谱 API 404、旧表不存在
- 前端：物详情页各 Tab、树视图、未指派资源提示

## 九、风险登记

- R1：mega-PR 评审与回滚成本（用户知情接受；缓解=分支内按逻辑阶段提交，先落地 fix/ai-deep-review 再开工）
- R2：dead event path 前科——事件管线验收必须真实全链路（见二·①）
- R3：mock 测试掩盖真实 DB 问题——集成测试必须 sqlx 真实 DB（见八）
- R4：知识图谱两个月即拆的前车之鉴——本体落地后需尽快跑真实 Agent 任务验证
- R5：products 与 thing_templates 概念重叠遗留——记入 TODOS 后续评估
