# 设备本体中心化设计（Thing Ontology）

> 日期：2026-07-22
> 状态：已确认
> 背景：现有 AI 功能中 workspace 级知识图谱与资源设计偏离方向。IoT 场景更适合"本体智能"——知识定义下沉到物，围绕物本体组织全部 AI 功能。

## 核心决策汇总

| # | 决策点 | 结论 |
|---|--------|------|
| 1 | 本体定义层级 | 混合：结构（属性/事件/动作）定义在模板层，知识/资源挂实例层，模板可提供默认知识 |
| 2 | workspace 知识图谱 | 完全废弃，代码拆除，数据直接丢弃不迁移 |
| 3 | workspace 静态资源 | 全部下沉到物，由"物"概念承载跨设备场景 |
| 4 | "物"与 Device 关系 | Device 原地泛化为 Thing：devices 表加 `thing_type`，车间/园区等概念物也是 devices 记录 |
| 5 | Agent 消费方式 | 完全工具化，不向 system prompt 注入本体上下文 |
| 6 | 事件与动作 | 动作 = 现有 commands 平移改名；事件新增定义，上报走现有 event 管线，告警可订阅 |
| 7 | 知识形态 | 文档挂载 + LLM 生成物的知识摘要（ontology_summary） |
| 8 | 交付范围 | 全量改造：后端泛化 + 前端改名 + 图谱拆除，一次到位 |
| 9 | 层级与分类 | parent_id 管归属（单父树），tags 管切面分类（扁平多对多） |
| 10 | API | 一刀切，只留 `/api/things`，不保留 `/api/devices` 兼容入口 |

## 一、核心概念与数据模型

### 物（Thing）

devices 表原地泛化：

```
devices（泛化为物）
├── thing_type: 'device' | 'space' | 'line' | 'building' | ...（新增，默认 'device'）
├── parent_id: 物的父子层级（车间 → 产线 → 设备），单父约束
├── ontology_summary: LLM 生成的知识摘要（TEXT，可空）
├── summary_status: 'ok' | 'pending' | 'failed'（新增）
└── 连接类字段（driver、protocol_config、online_status…）对非 device 类型可空
```

- 设备链路（驱动/网关/遥测/心跳/告警）全部加 `thing_type='device'` 过滤，行为不变
- 空间类物只是层级与知识的载体，无连接能力
- 层级与分类分工：
  - **parent_id 管"归属"**：唯一、强制、树形，`get_thing` 返回面包屑路径（如 `园区B / 车间A / 产线1 / 温度传感器`）
  - **tags 管"切面"**：多个、可选、扁平标签（如 `高压`、`能耗监测`），不承担层级语义
- 原图谱的 monitors/manages 等语义关系本期丢弃，不加替代；将来需要再加轻量 `thing_links` 表（YAGNI）

### 物模型（Thing Model）= 模板层结构定义

`device_templates` 改名 `thing_templates`，代码内 `DeviceTemplate` → `ThingTemplate`，扩展为完整物模型：

| 要素 | 来源 | 说明 |
|---|---|---|
| Property 属性 | 现有 properties 保留 | 可读/可写状态，遥测上报 |
| Action 动作 | 现有 commands 平移改名 | 可调用操作，下发链路复用现有命令通道 |
| Event 事件 | 新增 | 设备主动上报的发生，含级别 info/warning/error、字段 schema |
| Knowledge 知识 | 新增，挂实例 | 文档/资源 + LLM 摘要，模板可提供默认知识 |

模板带 `thing_type`，空间类物也可建模板（如"车间模板"：属性=面积/负责人，事件=人员超限）。

### 数据模型 ER

```
thing_templates ──< devices(things) ──< thing_resources (文档/图片/3D, device_id 外键)
       │                  │
       │                  └── ontology_summary / summary_status
       │
  properties / actions / events (JSON 定义, 模板层)

thing_events (事件实例表: id, device_id, event_name, level, data, unknown_event, ts)
```

- `thing_resources`：id、device_id（外键→物）、type（document/image/scene3d）、content 或 file_path、tags
- 删除表：knowledge_entities、knowledge_relations、knowledge_parse_jobs、workspace_resources

## 二、运行时管线

### ① 事件流（新增）

```
设备上报 → 网关/驱动 → 事件路由 ──┬──→ thing_events 表（实例存储，供 Agent/前端查询）
                                 ├──→ event 模块现有管线（实时推送、概览）
                                 └──→ alarm 模块（告警规则订阅 event 类型+级别触发）
```

- 上报格式：MQTT topic 约定 `thing/{id}/event/{event_name}`，payload 含 event_name、level、data、ts
- 模板事件定义是"说明书"（名称、级别、字段 schema）
- 未知事件名降级为 info 级存原始数据，`unknown_event=true`，不报错给设备（固件可能先于模板更新）
- alarm 规则引擎新增触发源 `event`，条件 = thing + event_name + level
- 事件存储本期不设保留上限

### ② 动作下发（commands 平移）

现有命令下发链路不动，只改名（DB 字段、API、前端文案 command → action）。模板 actions 定义带参数 schema，Agent 的 invoke_action 工具按 schema 校验后走现有通道。非 device 类型物调用动作返回明确错误"该物不支持动作"。

### ③ LLM 知识摘要（ontology_summary）

```
触发时机（异步任务，不阻塞主流程）：
  · 物的文档资源增/删/内容改 → 重算
  · 模板变更（属性/事件/动作定义改）→ 该模板所有物重算
  · 手动重新生成

输入：物名称/类型/面包屑路径 + 物模型定义 + 各文档资源前 2000 字符（单物最多拼 5 篇）
输出：≤500 字中文摘要，写回 devices.ontology_summary
防抖：同一物 60s 内多次触发只算一次；失败重试 3 次后 summary_status='failed'
```

摘要只服务 Agent 的 get_thing 工具，不进 system prompt。文档只存原文，不再有 LLM 实体/关系解析，摘要管线是唯一保留的 LLM 环节。

## 三、Agent 工具集

Agent 不注入任何本体上下文，全部通过工具按需获取：

| 工具 | 参数 | 返回 | 说明 |
|---|---|---|---|
| list_things | thing_type?, parent_id?, tags?, q? | 物的扁平列表（id/名称/类型/路径） | 发现有哪些物 |
| get_thing | thing_id | 面包屑路径、tags、ontology_summary、物模型定义（属性/事件/动作 schema） | 轻量，"这个物是什么、能做什么" |
| get_thing_profile | thing_id | get_thing 全部内容 + 各属性当前值（含时间戳）+ 最近 10 条事件 + 知识文档列表（标题/id/类型，不含正文） | 聚合快照，一次拿全 |
| get_thing_tree | root_id?, depth? | 树形结构（仅 id/名称/类型），默认深度 3 | 全局视野 |
| read_property | thing_id, property_name | 当前值 + 时间戳 | 读现有遥测最新值缓存，不做实时召测；无缓存返回 null + 提示 |
| invoke_action | thing_id, action_name, params | 下发结果/异步任务 id | 参数按模板 schema 校验；非 device 类型报错 |
| query_events | thing_id, event_name?, level?, since?, limit | 事件实例列表 | 查 thing_events |
| search_knowledge | thing_id?, q, tags?, limit | 命中文档列表（标题/所属物/片段） | 全文检索 thing_resources |
| read_document | resource_id | 文档正文 | 按需取全文 |

配套变更：

- 删除现有工具：agent/tools/knowledge.rs（图谱版 search_knowledge）、search_resources.rs
- invoke_action 属副作用操作，本期加工作区级开关 `require_action_confirm`（默认开），执行前需确认
- 工具描述用中文写清"什么时候用哪个工具"，引导 LLM 先 list_things/get_thing_tree 建立视野再深入
- 移除 agent 构建 system prompt 时调用 build_context 的逻辑

## 四、图谱拆除与改名范围

### 后端拆除（直接删，不迁移数据）

- `workspace/types/knowledge.rs`、`workspace/service/knowledge.rs`、`workspace/repo/knowledge.rs`、`workspace/handler/knowledge.rs`
- `agent/tools/knowledge.rs`、`agent/tools/search_resources.rs`
- DB 表：knowledge_entities、knowledge_relations、knowledge_parse_jobs、workspace_resources
- system prompt 的 build_context 注入逻辑

### 改名范围

- DB：devices 表名保留（加 thing_type/parent_id/ontology_summary/summary_status 列）；device_templates → thing_templates
- API：只留 `/api/things` 泛型接口，一刀切，不保留 `/api/devices`；模板 API 路径随改名调整
- 代码：template module 内 DeviceTemplate → ThingTemplate；device module 名保留（管连接，语义成立）

### 前端（web/）

- 导航与文案："设备" → "物"，设备列表页变为物列表（类型过滤 + 树视图）
- 新增物详情页，Tab：概览（summary + 属性当前值）｜属性｜事件｜动作｜知识（文档管理 + 摘要）
- 模板管理页：属性/事件/动作三段编辑
- 删除：知识图谱管理页、workspace 资源管理页（功能并入物的"知识"Tab）

## 五、错误处理

- 未知事件名：降级 info 存储 + unknown_event 标记，不报错给设备
- 动作下发：非 device 类型 → 明确错误；参数不符 schema → 校验错误明细；设备离线 → 复用现有命令通道行为
- 摘要管线：LLM 失败重试 3 次 → summary_status='failed'，物正常可用；get_thing 遇摘要为空返回"该物暂无摘要"
- 层级约束：parent_id 成环拒绝；删除有子节点的物拒绝，需先迁移子节点
- 遥测读取：无缓存值返回 null + "该属性暂无上报数据"

## 六、测试策略

- 单元：物模型 schema 校验、成环检测、摘要输入拼装（2000 字符截断、5 篇上限）、事件降级逻辑
- 集成（sqlx 真实 DB，沿用项目现有模式）：
  - 事件全链路：模拟上报 → thing_events 落库 → alarm 规则触发
  - 摘要管线：挂文档 → 触发 → mock LLM → 摘要写回
  - Agent 工具：9 个工具的参数校验与返回结构
- 拆除验证：图谱相关 API 返回 404、旧表不存在（migration 测试）
- 前端：物详情页各 Tab 渲染、树视图交互
