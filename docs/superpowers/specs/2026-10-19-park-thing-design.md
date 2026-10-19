# 园区物设计（派生属性 + 驱动双模架构）

> 日期：2026-10-19
> 状态：已确认（用户逐节批准）
> 背景：物本体重构（2026-07-22-thing-ontology-design.md）落地后，园区作为顶层空间物缺少"园区级指标"能力——面积等静态属性已有，但用电量/用水量/设备数量这类**由子物数据推导的聚合属性**没有来源。本期把园区物完整跑通：派生属性（表达式计算）+ 资源知识（3D 模型/图片）+ A2UI 渲染全链路。

## 核心决策汇总

| # | 决策点 | 结论 |
|---|--------|------|
| 1 | 园区聚合属性值来源 | 配置**表达式**从指定物属性计算（如 园区总用电 = {电表1.用电数} + {电表2.用电数}） |
| 2 | 表达式定义层级 | **实例层具体绑定**（引用具体物+属性），不做模板层符号引用 |
| 3 | 计算时机 | **绑定的属性变更时触发计算**（事件驱动），结果写回园区物属性值 |
| 4 | 驱动架构 | **区分轮询驱动与推送驱动两种模式**（DriverMode::Poll/Push）；派生驱动是首个推送驱动 |
| 5 | 派生定义存储 | **复用 thing_properties 表加三列**（source/expression/refs），不建新表 |
| 6 | API 面 | **复用 /api/things/{id}/properties**，补齐属性定义管理（POST/PUT/DELETE），不开 derived 子资源 |
| 7 | 资源定位 | 3D 模型/图片/文档都是物的**知识**；AI 经知识检索发现资源，在 A2UI 中渲染 |
| 8 | D5 细化裁决 | 非 device 物允许挂平台内置虚拟驱动 derived（非物理连接、无协议、不可下发动作）；物理驱动连接仍限定 device |
| 9 | deviceInstances 3D 点位标注 | 本期**不做**（无设备三维坐标数据来源），记入 TODOS |

## 一、驱动双模架构

现有 data_server 只有轮询循环，硬套所有驱动。 bacnet/modbus/opcua/snmp 是拉模型，EventBus 订阅/MQTT 订阅是推模型。`ThingDriver` 增加模式标识：

```rust
enum DriverMode { Poll, Push }

trait ThingDriver {
    fn mode(&self) -> DriverMode { DriverMode::Poll }  // 默认轮询，存量驱动零改动
    /// 推送驱动实现：数据到达时经 sink 主动上报
    fn start_push(&mut self, sink: ValueSink, event_bus: Arc<EventBus>) { /* 默认空 */ }
}
```

运行时（data_server）分流：

```
┌─ 轮询驱动（bacnet/modbus/...）── 现有循环：can_read_now → read_data_once ─┐
│                                                                            │
├─ 推送驱动（DerivedDriver/将来的 MQTT）── 内部订阅数据源，数据到达 → sink ──┤
│                                                                            ▼
│                                   共享下游管线（本次重构核心）：
│                        ingest_driver_values(thing, values)：
│                          变更收集 → device_cache 更新 → PropertyChange 发布
└──────────────────────────────────────────────────────────────────────────────
```

- **共享下游管线**：把现内联在轮询循环里的"变更收集 → 缓存更新 → 事件发布"抽成独立函数，轮询与推送都走它——所有物的属性值永远只有一条处理管线
- **ValueSink**：运行时交给推送驱动的上报句柄（内部 mpsc channel），data_server 统一消费后过共享管线——推送驱动不直接碰缓存，单一写入者纪律不破
- **命令分发不变**：推送驱动同样接收 execute_command（DerivedDriver 返回"该物不支持动作"）
- **生命周期**：推送驱动的 spawn/关停由 data_server 统一管理（随物删除/工作区停用回收订阅）
- **范围纪律（YAGNI）**：本期只让架构支持双模 + 落地 DerivedDriver 一个推送驱动；存量轮询驱动一行不动（MQTT 驱动现为 stub，将来实现时自然走推模式）

## 二、数据模型与表达式语言

### thing_properties 加三列（轻量 ALTER，无表重建）

```
thing_properties（新增列）
├── source TEXT NOT NULL DEFAULT 'template'   -- 'template'（模板拷贝）| 'derived'（派生）
├── expression TEXT                            -- 表达式原文，NULL = 非派生
└── refs TEXT                                  -- 解析后引用 JSON：[{thing_id, property_name}, ...]
```

- 派生属性就是物的一个普通属性实例：`data_type='number'`、`is_read_only=1`（值只允许 DerivedDriver 写）、`UNIQUE(device_id, name)` 免费保证 owner 内唯一
- 详情页、告警规则、Agent read_property、事件发布全部零特殊分支
- 保存时：解析表达式 → 校验 refs（同 workspace 内物/属性存在）→ DAG 环检测 → 落库
- **refs 以 thing_id 为准**：引用的物事后改名不影响计算；表达式原文中的名字会滞后，编辑器重新打开时按 refs 刷新显示
- **运行期反查索引（内存）**：`(thing_id, property_name) → [derived_property_id]`；启动时扫 `expression IS NOT NULL` 的行重建，配置增删改时增量维护。派生属性数量级小，不为此规范化 refs 子表（YAGNI）

### 表达式语言（最小集，自写递归下降解析器 ~200 行，不引第三方库）

```
expression := term (('+'|'-') term)*
term       := factor (('*'|'/') factor)*
factor     := NUMBER | ref | func | '(' expression ')'
ref        := '{' 物名 '.' 属性名 '}'              -- 例：{电表1.用电数}
func       := 'descendant_count(' [thing_type] ')' -- 后代物计数，设备数量用
```

- 配置 UI 用"选物 → 选属性"选择器生成 ref，用户不手敲语法；表达式原文只做展示与二次编辑
- 不支持：变量、自定义函数、字符串运算、外部调用

### 求值规则

| 场景 | 行为 |
|---|---|
| 任一引用属性无值（从未上报） | 整体**不更新**（保持旧值），标 stale"等待数据"——避免"一块表没上线，园区总用电突然变小"的误导 |
| 引用属性值非数值 | 同上，不更新 + stale + 结构化日志 |
| 全部引用有值 | 求值 → 与旧值比较 → 变了才写入并发布 PropertyChange |
| descendant_count() | 无引用，每次重算实时查 parent_id 树；子树结构变更（物增删/换父）也触发重算 |
| 除零 | 不更新 + stale（reason：除零），不写 Inf/NaN 进缓存 |

**DAG 校验**：保存表达式时 refs 构成有向边（owner → 引用物），成环拒绝保存并展示环路径。

## 三、运行时管线

### DerivedDriver 生命周期

```
创建/更新/删除派生属性（API 落库）
        │
        ▼
DerivedDriverManager（runtime 层，持有内存反向索引）
        · owner 物还没有 derived 驱动 → 实例化 DerivedDriver 注册进 data_server
        · 已有 → 热更新其引用表（增量维护反向索引）
        · 最后一个派生属性删除/物删除 → 回收驱动与 EventBus 订阅
        │
        ▼
DerivedDriver.start_push(sink, event_bus)
        1. 订阅 EventBus，过滤 ThingEventType::PropertyChange
        2. 启动即全量重算一次（首值补偿，不等首次变更）
```

### 事件驱动计算流

```
EventBus: PropertyChange(电表1.用电数)
   │  DerivedDriver 查引用表命中
   ▼
从 device_cache 取 refs 全部当前值 → 求值规则判定（全有值才算）
   │  结果 ≠ 旧值
   ▼
sink.send(ResultValue{ total_power, 新值 })
   │
   ▼
data_server 统一消费 → 共享下游管线（变更收集 → 缓存更新 → PropertyChange 发布）
   │
   ├─→ 前端 WS 刷新详情页
   ├─→ 告警规则（可挂园区总用电阈值）
   └─→ 若更上层派生引用了 total_power → 链式触发（DAG 保证无环）
```

### 三类触发源

| 触发源 | 说明 |
|---|---|
| PropertyChange 事件 | 主路径，引用属性变更即算 |
| 启动全量重算 | 重启后首值补偿 |
| 子树结构变更 | 物增删/换父 → 只影响含 descendant_count() 的派生；物管理面变更点直接调 Manager（不走 EventBus） |

**并发与顺序**：同一 DerivedDriver 的计算在 data_server 消费 sink 时串行化。EventBus 是 broadcast（slow consumer 可能丢消息）——可接受：计算输入永远是最新缓存值而非事件增量，下次任一引用变更即自愈，启动全量重算兜底。

**可观测性**：`derived_eval_total` / `derived_eval_stale` / `derived_eval_error` 计数；结构化日志含 derived_property_id、owner_thing_id、表达式、结果。

## 四、管理面 API 与前端

### API（复用 /api/things/{id}/properties 资源，补齐定义管理）

```
GET    /api/things/{id}/properties          现有——派生属性自动出现在列表
POST   /api/things/{id}/properties          新增——创建属性定义；payload 带 expression 即派生
PUT    /api/things/{id}/properties/{pid}    新增——更新定义（改表达式→重新校验+环检测）
DELETE /api/things/{id}/properties/{pid}    新增——删除属性定义
```

- POST body：`{name, display_name, unit, data_type, expression?}`；带 expression 时服务端解析 refs、校验、DAG 检测，`source='derived'`；不带则是普通实例层手动属性（顺带补齐本体设计 V1 裁决"物模型=每物自己的实例"一直缺的管理面）
- 状态无单独端点：GET properties 响应中派生属性附带 `derived: true`、`stale: bool`、`stale_reason`、`last_eval_at`
- `PUT .../value`（手动写值）对派生属性拒绝（is_read_only=1）
- 全部沿用现有 workspace 隔离校验（D3 硬性要求）

### 前端（沿用 home.css tokens 与手写 TS 视图模式，不引新组件库——D10 裁决）

1. **物详情页 · 属性 Tab**：派生属性行带「派生」徽标 + stale 状态点（灰点+文字"等待数据"）；点击展开表达式只读视图，引用物名可点击跳转
2. **派生属性编辑器**（详情页内 drawer，不新增页面）：
   - 属性名/display_name/单位
   - 表达式构建区：「+ 添加引用」→ 物选择器（树形+搜索）→ 选数值属性 → 自动插入 `{物名.属性名}`
   - 运算符按钮（+ − × ÷ 括号）+ descendant_count() 插入按钮
   - 实时表达式预览 + 保存时服务端校验错误行内红字定位
3. **创建后即时反馈**：保存成功 → 属性行出现，首值计算完成前显示"计算中…"，stale 时显示原因（哪几个引用无数据）

## 五、资源/A2UI 链路与跑通物料

### 资源链路（大部分现有能力，本期补缺口而非新建）

```
上传 glTF 模型 + 图片（现有资源上传）
   → resources 表（resource_type='scene3d'/'image'）
   → POST /{id}/resources 挂载园区物（现有 API + 知识 Tab UI）
   → Agent 知识工具（get_thing_profile/search_knowledge）返回资源元数据：类型、URL、名称
   → AI 发现"园区物配了三维模型" → A2UI 推送 scene-3d / image 组件渲染
```

- **本期核实并补齐的缺口**：①资源文件的可访问 HTTP URL（file_path → 前端/A2UI 能加载的地址）；②知识工具返回里资源元数据是否带 URL 和类型
- **裁剪**：scene-3d 组件的 deviceInstances（物标注到 3D 坐标+状态色点）本期不做——无设备三维坐标数据来源。本期只渲染模型本身 + 图片；点位标注记入 TODOS

### 模板与模拟数据

1. `templates/builtin/scenes/smart_campus.json` 扩充：园区属性骨架加 `total_power`（总用电 kWh）、`total_water`（总用水 m³）、`device_count`（设备数量）——模板层只给骨架，表达式一律实例层配置
2. 内置电表/水表设备模板（若不存在）+ 挂模拟驱动，持续产生遥测

## 六、错误处理

| 场景 | 行为 |
|---|---|
| 表达式语法错误 | 保存时 400，行内定位出错位置 |
| 引用不存在的物/属性 | 保存时 400，列出无效引用 |
| 引用图成环 | 保存时 400，展示环路径 |
| 引用的物/属性事后被删 | 派生属性转 stale（reason：引用缺失），不静默算错数；详情页红字标注缺失引用 |
| 引用值非数值/无值 | 不更新 + stale + 结构化日志 |
| 除零 | 不更新 + stale，不写 Inf/NaN |
| EventBus 消息丢失 | 自愈：计算基于当前缓存值；启动全量重算兜底 |
| 派生驱动崩溃 | data_server 监督重启驱动，重订阅 + 全量重算 |
| 手动写派生属性值 | PUT .../value 拒绝（is_read_only） |

## 七、测试策略

沿用项目铁律：集成测试 sqlx 真实 DB，禁 mock-only（LLM 才可 mock）。

- **单元**：表达式解析器（合法/非法语法、优先级、括号、除零）、求值规则（无值→stale、非数值→stale）、DAG 环检测、refs 提取
- **集成**：
  - 全链路：模拟表计上报 → PropertyChange → 派生重算 → 园区属性值更新 + 事件发布（不接受 mock 管线——dead path 前科 R2）
  - 双模架构：轮询驱动行为不变（回归）+ 推送驱动 sink → 共享管线落值；两条路径产出的事件结构一致
  - 链式派生：楼栋派生 → 园区派生逐级触发；成环配置被拒
  - 生命周期：创建→驱动注册；改表达式→热更新；删最后一个派生→驱动回收；重启→全量重算首值
  - stale 流：停掉一块表 → stale 标记 + GET properties 返回 stale_reason
  - workspace 隔离：跨 workspace 引用被拒、跨 workspace 读派生状态 404
  - 子树变更：增删后代物 → descendant_count() 派生自动重算
- **前端手测**：验收旅程 6 步全过

## 八、验收旅程（= 验收标准）

| 步骤 | 预期 |
|---|---|
| 1. 实例化智慧园区场景包 | 园区物 + 楼栋子树落库 |
| 2. 挂 2 块模拟电表 + 1 块模拟水表 | 表计持续上报遥测 |
| 3. 园区物配 3 个派生属性 | total_power={电表1.用电}+{电表2.用电}、total_water={水表1.用水}、device_count=descendant_count('device')（只计设备类后代，不含楼栋） |
| 4. 园区详情页 | 派生值随表计上报实时刷新；停一块表 → total_power 转 stale"等待数据" |
| 5. 上传园区 glTF + 图片挂为知识 | 知识 Tab 可见 |
| 6. AI 对话："看看园区的三维模型和概况" | Agent 经知识检索发现资源 → A2UI 渲染 3D 场景 + 图片 + 实时属性卡 |

## 九、风险登记

- R1：驱动双模重构触碰 data_server 热路径——缓解：共享管线抽取后先做轮询回归测试，推送路径独立验证
- R2：dead path 前科——派生管线验收必须真实全链路（模拟驱动上报，不接受 mock）
- R3：EventBus broadcast 丢消息导致派生值延迟——缓解：计算基于缓存当前值自愈 + 启动全量重算；接受最坏情况为"下次引用变更时刷新"
- R4：表达式被当成通用规则引擎滥用——缓解：语法最小集硬限制（无变量/无自定义函数），需求超出时另行设计
