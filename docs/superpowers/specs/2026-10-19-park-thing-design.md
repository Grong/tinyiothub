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
| 6 | API 面 | **复用 /api/v1/things/admin/{id}/properties**，补齐属性定义管理（POST/PUT/DELETE），不开 derived 子资源 |
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

顺带：CREATE INDEX idx_devices_parent_id ON devices(parent_id)  -- E6：descendant_count 树查询防患
```

- 派生属性就是物的一个普通属性实例：`data_type='number'`、`is_read_only=1`（值只允许 DerivedDriver 写）、`UNIQUE(device_id, name)` 免费保证 owner 内唯一
- 详情页、告警规则、Agent read_property、事件发布全部零特殊分支
- 保存时：解析表达式 → 校验 refs（同 workspace 内物/属性存在）→ DAG 环检测 → 落库
- **refs 以 thing_id 为准**：引用的物事后改名不影响计算；表达式原文中的名字会滞后，编辑器重新打开时按 refs 刷新显示
- **运行期反查索引（内存）**：`(thing_id, property_name) → [derived_property_id]`；启动时扫 `expression IS NOT NULL` 的行重建，配置增删改时增量维护。派生属性数量级小，不为此规范化 refs 子表（YAGNI）
- **stale 状态归属（E3/E4）**：`stale`/`stale_reason`/`last_eval_at` 是 Manager 内存态，**不落库**（重启后经全量重算自然重建）；GET properties 响应中的派生状态由 cloud 经 AppState 的 runtime 句柄向 Manager 查询（state.rs device_cache 注入先例）

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
- **输入上限**（CEO 审查 F-C）：expression ≤1000 字符、refs ≤50 个、`descendant_count()` 参数必须是合法 thing_type——超限保存时 400，防表达式被当成规则引擎滥写

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
| 启动全量重算 | 重启后首值补偿；refs 内存索引重建时遇 JSON 损坏行 → 该派生属性 fail-closed（标 stale + 结构化日志），不阻断启动（CEO 审查 F-F） |
| 子树结构变更 | 物增删/换父 → 只影响含 descendant_count() 的派生；物管理面变更点直接调 Manager（不走 EventBus） |
| 引用对象删除 | 引用的物/属性被删除 → 同一管理面钩子通知 Manager，受影响派生标 stale（reason：引用缺失）并热更新引用表（CEO 审查 F-B） |

**并发与顺序**：同一 DerivedDriver 的计算在 data_server 消费 sink 时串行化。EventBus 是 broadcast（slow consumer 可能丢消息）——可接受：计算输入永远是最新缓存值而非事件增量，下次任一引用变更即自愈，启动全量重算兜底。

**sink 消费结构（E3）**：ValueSink 的 mpsc 接收端由 data_server **独立 spawn** 消费（`tokio::select!` 于 shutdown 信号），不嵌入 100ms tick 轮询循环——推送流量不拉长轮询周期，轮询慢也不阻塞派生计算。

**ports 约束（E1）**：Manager 的全部 DB 读取（启动扫 `expression IS NOT NULL` 行重建索引、`descendant_count()` 树查询）必须经新增 port trait `DerivedPropertyQueries`（crates/runtime/src/ports.rs，ThingCacheSource 先例）——runtime crate 禁止直接 sqlx（b4ac6db7 runtime→db 隔离是硬架构约束）。

**可观测性**：`derived_eval_total` / `derived_eval_stale` / `derived_eval_error` / `derived_eventbus_lagged`（slow consumer 丢消息计数，CEO 审查 F-E）；结构化日志含 derived_property_id、owner_thing_id、表达式、结果。**Runbook**：派生属性 stale → ①查 stale_reason 指向的引用物是否在线/已删 → ②查 derived_eventbus_lagged 是否增长（lag 丢消息靠下次变更自愈）→ ③重启触发全量重算兜底。

## 四、管理面 API 与前端

### API（复用现有 properties 资源，补齐定义管理）

路由实际挂载点（2026-10-19 CEO 审查核实，`apps/cloud/src/api/mod.rs:105` + `admin/thing/properties.rs`）：

```
GET    /api/v1/things/admin/{id}/properties          现有——派生属性自动出现在列表
POST   /api/v1/things/admin/{id}/properties          新增——创建属性定义；payload 带 expression 即派生
PUT    /api/v1/things/admin/{id}/properties/{pid}    新增——更新定义（改表达式→重新校验+环检测）
DELETE /api/v1/things/admin/{id}/properties/{pid}    新增——删除属性定义
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

**交互状态表**（CEO 审查 F-D，沿用本体设计 D7 先例——描述用户看到的）：

| 功能 | 加载中 | 空 | 错误 | 成功 | 部分 |
|---|---|---|---|---|---|
| 属性 Tab 派生行 | 行骨架 | —（无派生不显示区块） | 值计算失败=灰点+"等待数据"文字 | 实时值+「派生」徽标 | 部分引用无值=stale 灰点+缺失引用数 |
| 表达式只读展开 | — | — | 引用物已删=该引用红字划线+"已删除" | 表达式+可跳转引用 | — |
| 派生编辑器 drawer | 物选择器骨架 | 无候选物="本工作区还没有带数值属性的物"+引导 | 保存 400=行内红字定位（语法/环/上限） | 保存成功 toast+drawer 关闭 | 部分引用失效=保存前黄条提示 |
| 物选择器 | 树骨架 | "无匹配的物，清除搜索"链接 | 加载失败=错误条+重试 | 树形+搜索渲染 | — |

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

- **轮询回归先行（E2，CRITICAL）**：共享下游管线抽取**之前**，先为 data_server 轮询路径补行为回归测试（变更事件收集、device_cache 更新、命令分发三条），锁定现有行为后再重构——当前 data_server 无 mod tests，裸抽取等于无保护改热路径
- **EventBus 时序可控点（E5）**：涉 EventBus 的测试必须先确认订阅已建立再发布事件（屏障/ack 机制），禁止 sleep 碰运气
- **单元**：表达式解析器（合法/非法语法、优先级、括号、除零）、求值规则（无值→stale、非数值→stale）、DAG 环检测、refs 提取
- **集成**：
  - 全链路：模拟表计上报 → PropertyChange → 派生重算 → 园区属性值更新 + 事件发布（不接受 mock 管线——dead path 前科 R2）
  - 双模架构：轮询驱动行为不变（回归）+ 推送驱动 sink → 共享管线落值；两条路径产出的事件结构一致
  - 链式派生：楼栋派生 → 园区派生逐级触发；成环配置被拒
  - 生命周期：创建→驱动注册；改表达式→热更新；删最后一个派生→驱动回收；重启→全量重算首值
  - stale 流：停掉一块表 → stale 标记 + GET properties 返回 stale_reason
  - 引用对象删除（F-B）：删除被引用的物/属性 → 派生转 stale（reason：引用缺失）+ 引用表热更新
  - 索引重建容错（F-F）：refs JSON 损坏行 → 该派生 fail-closed 标 stale，启动不阻断
  - 输入上限（F-C）：expression 1001 字符 / refs 51 个 / 非法 thing_type 参数 → 400
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

---

## CEO 审查记录（2026-10-19，/plan-ceo-review）

### 模式与裁定

| 决策 | 结果 |
|---|---|
| D1 审查模式 | SCOPE REDUCTION（用户选定）——估计 ~20-24 文件越 >15 规则线，spec 已 YAGNI 硬裁，杠杆=分段而非砍能力 |
| D2 裁减裁定 | 编辑器全量保留；A2UI 旅程保留；**递延**：派生挂告警集成测试、链式派生集成测试（已记 TODOS.md） |
| D3 spec 修订 | 五项全采纳：F-B 引用删除触发、F-C 输入上限、F-D 交互状态表、F-E lag 指标+runbook、F-F 索引重建 fail-closed |
| F-A 笔误更正 | API 路径 /api/things/... → /api/v1/things/admin/...（事实性更正，无需裁定） |

### REALITY 行（PLAN_REALITY: phase=ceo rows=7/7 verdict=complete, unverified=2）

- premise-table: finding unverified=2（资源 HTTP URL 服务、知识工具资源元数据——spec 自声明缺口，实现任务 T5/T6）
- already-done: pass（派生/表达式零既有实现，commit 72381d26 工作树 grep）
- surface-check: finding（API 路径笔误，已更正）
- binding-decisions / numbers / deferred-asks / pr-count: pass

### NOT in scope

- 递延（记 TODOS.md）：①派生属性挂告警集成测试（D2：告警读属性值走既有路径，风险低）；②链式派生集成测试（D2：DAG 校验已兜底防环）
- 拒绝（不计 TODOS）：编辑器降级为文本框（D2 用户保留全量编辑器）；A2UI 旅程递延（D2 用户保留——验收终点）
- 既有裁剪（设计阶段）：deviceInstances 3D 点位标注（无坐标数据源，记 TODOS）；模板层表达式（用户裁决实例层）；定时回写（用户裁决事件驱动）

### What already exists

EventBus 广播+订阅（crates/runtime/src/event_bus.rs:49,74）；PropertyChange 事件构造与"缓存先于发布"（crates/runtime/src/data_server.rs:171,214）；ThingDriver trait + wrapper can_read_now/read_data_once/event_bus_ref（crates/core/src/driver/mod.rs:254; crates/runtime/src/driver/wrapper.rs:71,91,99）；thing_properties 表（baseline.sql:1236）；GET properties + PUT value（admin/thing/properties.rs:28-29）；resources 挂载 API（thing/handler/mod.rs:48-49）；A2UI scene-3d/image 组件（web/src/ui/chat/a2ui/catalog/）；smart_campus 场景包+实例化（templates/builtin/scenes/）；MQTT 驱动为 stub（推模式无历史包袱）——全部复用，无重建。

### Dream state delta

12 个月理想：任意空间层级（区域→园区→楼栋→楼层）表达式派生指标 + 3D 数字孪生 + AI 空间语义。本计划交付：首个推送驱动、首个派生属性引擎、首个园区全链路 demo。留下：deviceInstances 点位、模板层表达式复用、派生历史曲线。

### Error & Rescue Registry / Failure Modes

```
CODEPATH                | FAILURE MODE            | RESCUED? | TEST? | USER SEES      | LOGGED?
------------------------|-------------------------|----------|-------|----------------|--------
保存-表达式解析          | 语法错误/超上限(F-C)      | Y 400行内 | Y     | 行内红字定位     | Y
保存-refs 校验           | 引用不存在/跨workspace   | Y 400    | Y     | 无效引用清单     | Y
保存-DAG 环检测          | 引用成环                 | Y 400    | Y     | 环路径展示       | Y
求值                     | 引用无值/非数值/除零      | Y stale  | Y     | stale 灰点+原因 | Y
EventBus 订阅            | lag 丢消息               | Y 自愈    | 部分  | 无感            | Y lag计数
启动索引重建              | refs JSON 损坏(F-F)      | Y stale  | Y     | stale           | Y
引用对象删除(F-B)         | 物/属性被删              | Y stale  | Y     | 引用红字划线     | Y
DerivedDriver 运行        | 驱动 panic              | Y 监督重启| 未指定 ← WARNING(实现期验证, T1) | 无感 | Y
PUT value 手写派生        | is_read_only            | Y 拒绝    | T3补  | 错误提示        | Y
```

CRITICAL GAPS: 0（无 RESCUED=N 且静默的行）。WARNING 1 行：驱动 panic 监督重启——spec 声称 data_server 监督重启，实现期必须证明该机制存在或补上（T1）。

### Implementation Tasks

- [ ] **T1 (P1, human: ~2h / CC: ~25min)** — runtime — 验证/补齐 DerivedDriver panic 监督重启
  - Surfaced by: S2 Error & Rescue — spec §6 声称"data_server 监督重启驱动"，现有轮询驱动读错误只标 offline，panic 监督机制未证实
  - Files: crates/runtime/src/data_server.rs, crates/runtime/src/driver/（派生驱动新文件）
  - Verify: 集成测试——注入 panic 的推送驱动被重启并重订阅+全量重算
- [ ] **T2 (P1, human: ~3h / CC: ~30min)** — thing 域 — 物/属性删除管理面钩子通知 DerivedDriverManager（F-B）
  - Surfaced by: S4 数据流 — 引用对象删除时派生转 stale 无触发路径
  - Files: apps/cloud/src/domains/thing/handler/crud.rs, apps/cloud/src/domains/thing/service/, crates/runtime/src/driver/（Manager）
  - Verify: 集成测试——删除被引用物 → 派生 stale（reason：引用缺失）+ 引用表热更新
- [ ] **T3 (P2, human: ~30min / CC: ~5min)** — admin API — PUT .../value 对派生属性拒绝的测试
  - Surfaced by: S2 registry 行"PUT value 手写派生"
  - Files: apps/cloud/src/domains/admin/thing/properties.rs（+测试）
  - Verify: API 测试——对 source='derived' 属性 PUT value 返回 4xx
- [ ] **T4 (P2, human: ~1h / CC: ~10min)** — runtime — derived_eventbus_lagged 指标（F-E）
  - Surfaced by: S8 可观测 — slow consumer 丢消息不可见
  - Files: crates/runtime/src/driver/（DerivedDriver 订阅处）
  - Verify: 单测/集成——构造 lag 后计数 +1
- [ ] **T5 (P2, human: ~1h / CC: ~10min)** — thing 域 — 资源文件 HTTP URL 服务缺口核实与补齐
  - Surfaced by: premise-table unverified #1 — resources.file_path → 前端/A2UI 可加载 URL
  - Files: apps/cloud/src/domains/thing/handler/resources.rs（核实后定）
  - Verify: 手测——上传 glTF 后前端可直接加载 URL
- [ ] **T6 (P2, human: ~1h / CC: ~10min)** — agent 工具 — 知识工具资源元数据（URL+类型）核实与补齐
  - Surfaced by: premise-table unverified #2 — AI 发现资源的前提
  - Files: apps/cloud/src/domains/agent/host/tools/thing/search_knowledge.rs（核实后定）
  - Verify: 工具测试——返回含 resource_type 与可访问 URL

---

## Eng 审查记录（2026-10-19，/plan-eng-review）

### Scope record

- feature answers: CEO D2 复用（递延 2 条集成测试，其余全保留），无新裁减
- structure: **Original arrangement**（D1）——expression.rs / derived.rs / derived_manager.rs 三新服务分文件
- accepted scope: spec 全量（双模重构+解析器+Manager+API+前端+模板物料+A2UI 链路）
- pending remedies: E1-E6 见下

### 核查出的事实（file:line 收据）

- `crates/runtime/src/ports.rs:18,53` —— runtime 不直连 db，一切经 port trait（b4ac6db7 隔离约束）
- `crates/runtime/src/data_server.rs:20` —— DriverCache = moka Cache<String, Arc<RwLock<DriverWrapper>>>；`:84-88` 100ms tick 扫全部驱动；`:64` device_cache 类型是 `Arc<dyn ThingCacheSource>`（port）
- `crates/runtime/src/data_server.rs` 无 `mod tests`；runtime 仅 `tests/ports_injection_test.rs`（3 例）
- `crates/db/migrations/20260819000001_baseline.sql` —— devices 表无 parent_id 索引（CEO S7 曾误称有，此条更正）

### Findings（置信度校准，引用动机行）

- **E1 [P1] (9/10)** ports.rs:18 —— spec 未提 ports 约束：Manager 启动索引重建（扫 expression 行）与 descendant_count 树查询必须新增 port trait（如 `DerivedPropertyQueries`），runtime 内禁止直接 sqlx
- **E2 [P1] (9/10)** data_server.rs 无 mod tests —— 共享下游管线抽取是对**无直接单测的热路径**重构（REGRESSION RULE）：抽取前必须先锁定轮询行为回归（变更收集/缓存更新/命令分发）
- **E3 [P2] (8/10)** data_server.rs:84-88 —— sink 消费结构 spec 未指明：ValueSink mpsc 的消费应独立 spawn（select! 于 shutdown），不嵌进 100ms tick 轮询循环，避免推送流量拉长轮询周期
- **E4 [P2] (8/10)** spec 未写清 stale 状态归属：stale/stale_reason/last_eval_at 为 Manager 内存态（重启经全量重算重建），cloud GET properties 经 AppState 的 runtime 句柄查询（state.rs device_cache 先例）；不落库
- **E5 [P2] (7/10)** EventBus 时序测试需可控暂停点（订阅确认后再发布），防 flaky
- **E6 [P3] (9/10)** baseline.sql 索引清单 —— devices 无 parent_id 索引：顺手加 `idx_devices_parent_id`（CREATE INDEX 零成本，descendant_count 树大时防患）

### 测试覆盖图（新增路径）

```
CODE PATHS                                              USER FLOWS
[+] runtime/expression.rs (解析+求值)                   [+] 园区详情页属性 Tab
  ├── [★★★ 计划] 语法/优先级/括号/ref/func 穷举单测      ├── [★★ 计划] 派生行徽标+stale 灰点渲染(vitest)
  ├── [★★★ 计划] 无值/非数值/除零 → stale              └── [GAP→手测] 实时刷新(WS)——验收旅程 step4
  └── [★★★ 计划] 上限 F-C(1000字符/50refs/类型)        [+] 派生编辑器 drawer
[+] runtime/driver/derived.rs + derived_manager.rs         ├── [★★ 计划] 保存 400 行内红字定位(vitest)
  ├── [★★★ 计划] 事件命中→重算→sink(集成,真实DB)         └── [GAP→手测] 物选择器树+搜索
  ├── [★★★ 计划] 启动全量重算/索引损坏 fail-closed(F-F)  [+] 验收旅程 6 步 [→E2E 手测]
  ├── [★★★ 计划] 引用删除→stale(F-B)/子树变更→重算       [→E2E] 模拟表计上报→园区值更新(集成,禁mock管线)
  ├── [★★  计划] panic 监督重启(T1)                     [→EVAL] 无 LLM 变更,不需要 eval
  └── [★★  计划] lag 计数(F-E)
[+] data_server 共享管线抽取
  └── [GAP→CRITICAL] 轮询行为回归(E2：抽取前锁定)
[+] admin properties POST/PUT/DELETE
  ├── [★★★ 计划] 校验 400(语法/环/上限)/唯一约束 409
  ├── [★★  计划] workspace 隔离 404/拒绝跨域引用
  └── [★★  计划] PUT value 拒绝派生(T3)

COVERAGE: 计划覆盖 15/17 路径 | GAPS: 2 手测项(WS刷新/选择器) + 1 CRITICAL(轮询回归 E2)
```

### Dispositions

E1-E6 **accepted**（D2 批量裁定全采纳，2026-10-19）：E1 ports 约束已落 §三；E2 轮询回归先行已落 §七（CRITICAL，重构前必经）；E3 sink 独立消费已落 §三；E4 stale 内存态归属已落 §二；E5 时序可控点已落 §七；E6 parent_id 索引已落 §二迁移。

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAR | mode: SCOPE_REDUCTION, 0 critical gaps |
| Outside Review | codex (auto) | Independent 2nd opinion | 2 | unavailable | codex 模型路由未配置 + native fallback 两轮超时，no completed external review |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | ISSUES OPEN | 6 issues, 0 critical gaps（E1-E6 全采纳落 spec） |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | 未运行（REDUCTION 模式不推荐） |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | 未运行 |

- **OUTSIDE COVERAGE:** codex, plan-review, unavailable（两轮）——custom endpoint 未配 frontier 模型（CODEX_MODEL invalid）；native fallback 各 5 分钟无完整报告；无外部覆盖
- **VERDICT:** CEO CLEARED + ENG ISSUES OPEN——6 项 eng 发现已全部裁定采纳并落入 spec 正文，无未决项；可进入 writing-plans 实现规划

NO UNRESOLVED DECISIONS
