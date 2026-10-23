# 园区物体验修复包设计（派生引擎单一化 + 派生 UX + 场景可视化）

> 日期：2026-10-20
> 状态：已确认（用户逐段裁定，"全部一起做"）
> 背景：园区物派生属性（2026-10-19-park-thing-design.md）落地后真实使用暴露四类体验断层：派生驱动不可见、骨架与派生两张皮、物理物混合派生被拒（I-1 过度防守）、场景实例化是"盲建死树"。本包系统性修复。

## 核心决策汇总

| # | 决策点 | 结论 |
|---|--------|------|
| 1 | 混合模式（物理物部分属性计算） | **支持**。派生引擎单一化取代每物一个 DerivedDriver wrapper——I-1 的"带物理驱动拒派生 400"取消 |
| 2 | 派生引擎形态 | **Manager 内单一引擎**：全 workspace 一个 EventBus 订阅 + 反向索引 + 直接 sink.send；DerivedDriver wrapper 整个删除（D5 细化裁决"空间物挂虚拟驱动"随之作废——不再需要） |
| 3 | driverless 物的值落点 | `consume_driver_reading` 加 **device_cache 回退**：wrapper miss 时直接对缓存 Thing 走共享管线（apply_property_values + cache_and_publish） |
| 4 | 骨架与派生关系 | **就地升级模型**：数值型普通属性（含模板骨架）可「绑定表达式」原行转派生；派生行可「编辑表达式」/「解绑」（去表达式回退普通属性，行保留）；不再有"骨架 vs 新增"两张皮 |
| 5 | 派生可见性 | 详情页属性 Tab 顶部**派生计算状态卡**：N 个派生 / M 个等待数据 / 最近计算时间，stale 展开看原因 |
| 6 | 场景包与表计 | 实例化可视化编辑器：选场景包 → 交互树预览 → 树上加设备（电表/水表）、改参数、挂模拟驱动 → 一次性实例化（`extra_nodes` 与场景节点**同事务**） |
| 7 | 物树管理 | 树视图拖拽换父（落地本体设计 D12 原裁决：成环红框拒绝、合法落点调 API、服务端校验兜底）+ 树节点「+」新增设备（向导预填 parent_id） |
| 8 | 叶节点出数 | 实例化提供「为设备挂模拟驱动」开关（**默认开**）——温湿度传感器/表计开箱出数，死树变活树 |

## 一、派生引擎单一化（X4）

### 现状问题

每派生 owner 物注册一个 DerivedDriver wrapper 进 driver_cache（moka，按 thing_id 单格）：
- 带物理驱动的物再配派生 → wrapper 被顶掉（终审 I-1 发现），当时的防守是 API 层 400 拒绝——但"设备大多数属性驱动采集、个别属性计算"是合理需求，拒绝是错的
- wrapper 模型还带来 Task 7 台账递延 Minor×2（被删 owner 订阅泄漏、末条删除不摘除）

### 目标模型

```
EventBus PropertyChange ──→ DerivedEngine（DerivedDriverManager 内，全 workspace 单一订阅）
                               │ 反向索引 (thing_id, property_name) → 受影响派生 defs
                               ▼
                     device_cache 取引用属性当前值（按 thing_id）→ 表达式求值
                               │ 结果 ≠ 旧值
                               ▼
                     sink.send(DriverReading{ owner_thing_id, values })
                               │
              data_server consume_driver_reading：
                有 wrapper（物理物）→ 写进物理 wrapper 的 Thing（与驱动采集属性共存）✅
                无 wrapper（空间物）→ device_cache 回退：直接对缓存 Thing 走共享管线 ✅
```

- **删除**：DerivedDriver wrapper、ensure_driver 的 driver_cache 注册、I-1 的 API 400 拒绝与防御性跳过
- **保留**：DerivedDefsHandle 热更新语义、反向索引、stale 内存态、首值补偿（启动全量重算）、Lagged 计数+自愈、三类触发源入口（notify_*）
- **简化**：订阅生命周期 = Manager 一个任务，随 Manager 启停；被删 owner 从索引摘除即失效，无泄漏
- **D5 语义回落**：不再有任何虚拟驱动挂物——"derived 驱动"概念退场，派生是平台引擎能力而非驱动；`driver_name` 永不出现 derived

### 混合模式语义

- 物理物的派生属性值与驱动采集值落在同一 Thing（consume 写进物理 wrapper 的 Thing）→ 缓存、事件、前端、告警自然一致
- 时序：派生重算由**其他物**（或本物其他属性）的 PropertyChange 触发；本物驱动读数触发自身派生同样成立（引用表按 (thing_id, property) 命中）
- 防 ping-pong：派生写回产生的 PropertyChange 若命中自己引用表 → DAG 校验在配置侧已禁止自环（owner 引用自身属性被拒），运行时无循环

## 二、派生 UX（X1+X2）

### 属性 Tab 结构（自上而下）

```
┌ 派生计算状态卡（X1，仅该物有派生属性时显示）
│   派生计算 · 3 个属性 · 1 个等待数据 · 最近计算 14:32:07
│   （stale 计数可展开：哪些引用缺数据/已删除）
├ 属性表
│   area            5000 ㎡           [绑定表达式]  ← 数值型普通属性（X2）
│   total_power     358 kWh  [派生]   [编辑表达式] [解绑]  ← 派生属性（X2）
│   device_count    — 等待数据 [派生]  [编辑表达式] [解绑]
└ + 新增派生属性（保留：模板没有的新指标）
```

- **绑定表达式**（普通数值行）：打开编辑器预填属性名/单位（只读），写表达式 → `PUT /api/v1/things/admin/{id}/properties/{pid}`（带 expression → 服务端重新校验+DAG，source 转 derived）——**原行就地升级，无 409、无重复行**
- **编辑表达式**（派生行）：同编辑器改表达式 → 同一 PUT
- **解绑**（派生行）：确认后 PUT 移除 expression（source 回退 'template'/手动，保留行与当前值为普通只读属性）
- **删除**：仅「新增派生属性」创建的属性提供整行删除（模板骨架行不提供删除——删模板骨架属另一语义，本期不做）
- 后端已就绪：PUT 支持加/改/去 expression（终审修复波落，含转派生 data_type 默认 number）；**唯一新增**：PUT body 接受 `expression: null` 显式解绑（区分"不更新 expression 字段"与"清空"——serde 三层语义：缺省=不动、null=解绑、字符串=重设）

### 状态卡数据

全现成：GET properties 每行带 derived_status（stale/stale_reason/last_eval_at）——前端聚合渲染即可，零后端改动。

## 三、场景实例化可视化编辑器（X3a）

### 交互流

```
市场 → 智慧园区 → 「实例化」
   ▼
实例化对话框（升级为编辑器）：
┌ 参数区（沿用现有：building_count/floor_count 等，改数→树实时重算）
├ 可视化预览树（交互式，替换现有静态预览）
│   每个节点：物名 + 类型徽标 + 属性数 + 驱动标记（模拟/物理/无）
│   节点操作：
│    · 参数化节点（楼栋/楼层）：树上 +/- 改数
│    · 任何容器节点：「+ 添加设备」→ 选设备模板（列表过滤 scenes）→
│      填名称/数量/驱动（默认 simulator）→ 加为待建节点（虚线框样式）
│    · 待建节点：可删除、可改驱动
├ 「为设备挂模拟驱动」总开关（默认开）：叶节点设备（含场景包自带温湿度、
│   待建表计）driver_name 统一置 simulator，interval=3000
└ [取消] [确认实例化（将创建 N 个本体）]
```

### 后端：实例化 API 扩展

`POST /api/v1/marketplace/thing-templates/{id}/instantiate` body 扩展：

```json
{
  "parameters": {"building_count": 2, "floor_count": 5},
  "parent_id": null,
  "attach_simulated_driver": true,
  "extra_nodes": [
    {"template_id": "builtin_power_meter", "parent_path": [], "name": "电表{index}", "count": 2, "driver_name": "simulator"},
    {"template_id": "builtin_water_meter", "parent_path": [], "name": "水表{index}", "count": 1, "driver_name": "simulator"}
  ]
}
```

- `parent_path`：从根开始的节点 key 路径（[] = 挂根/园区；["building_1"] = 挂 1 号楼）——与场景包节点 key 空间一致；实例化器在**同一事务**内建场景节点 + extra_nodes（含属性拷贝 Fix A 路径 + driver_name/driver_options 落库 + 驱动注册进 data_server）
- `attach_simulated_driver: true` 时场景包自带叶节点设备（thing_type='device' 且无 driver_name 的）统一挂 simulator
- dry-run 预览同步支持 extra_nodes（计数与树含待建节点）
- 校验：extra_nodes 的 template 必须存在且非 scenes 类；parent_path 必须命中场景节点 key；count 上限 20/节点；节点总数仍受配额约束（plan_enterprise=0 无限）

### 实例化后：自动挂驱动出数

实例化完成后 driver_name='simulator' 的物由 data_server 既有驱动加载路径（initialize_drivers）注册——**确认该路径对实例化新建物即时生效**（若只在启动时加载，则实例化返回后需触发一次驱动注册/热加载；实现时核实并补齐，开箱出数是本项的核心承诺）。

## 四、物树拖拉管理（X3b，落地 D12）

- 物列表树视图（X3a 的交互树组件复用）：节点可拖拽
- 拖到成环目标（自身或后代）：实时红框 + 提示拒绝，不落库
- 合法落点松手：`PUT /api/v1/things/{id}`（parent_id 变更）；服务端成环校验兜底（已有）+ `notify_tree_changed` 双侧触发（旧父/新父，descendant_count 派生重算——Task 8 已接线换父钩子，核实覆盖）
- 树节点「+」：打开建物向导并预填 parent_id=该节点；建完树局部刷新

## 五、错误处理

| 场景 | 行为 |
|---|---|
| X4 引擎订阅任务崩溃 | Manager 监督重启（复用 T1 结论：派生路径无 panic 面；重启=重订阅+全量重算） |
| consume cache 回退时缓存也无该物 | 丢弃 + warn 日志（物已删的迟到读数） |
| 绑定表达式校验失败（语法/环/跨 workspace） | PUT 400 行内红字（编辑器内展示） |
| 解绑时有告警规则引用该派生属性 | 放行（告警对普通属性同样可挂，语义不破） |
| extra_nodes parent_path 不命中/模板不存在/超 count | 400 明细（哪个节点什么原因） |
| 实例化事务中途失败 | 整事务回滚（场景节点+extra_nodes 同生共死） |
| 拖拽换父成环 | 前端红框拒绝；绕过前端直连 API 成环 → 服务端 400（既有） |

## 六、测试策略

- X4：混合模式集成（物理物 modbus 采集值与派生值共存于同一 Thing/缓存）；consume cache 回退（无 wrapper 物写值成功）；I-1 拒绝移除后物理物配派生 200；引擎单订阅（subscriber_count=基线+1，删除全部派生后回落）；既有派生测试全绿（引擎形状改变不改为行为）
- X2：绑定升级（骨架→派生，无 409，原行 source 转 derived）；解绑（回退普通属性行保留）；PUT expression 三层语义（缺省/null/字符串）
- X3a：extra_nodes 同事务（失败回滚无半棵树）；模拟开关叶节点 driver_name=simulator；dry-run 计数含 extra_nodes；实例化后驱动注册生效（缓存出值）
- X3b：拖拽换父 API 路径（成环 400/合法 200/双侧 notify_tree_changed）——前端拖拽交互 vitest 模拟 DnD 事件

## 七、范围外（本期不做）

- 场景包模板本身的编辑器（改模板定义另存——实例化是快照，本期只编辑"实例化定制"）
- 模板骨架属性的整行删除（另一语义）
- 派生历史曲线（values 只存当前值，无历史——既有平台约束）
- 3D deviceInstances 点位标注（沿用前 spec 裁剪）
