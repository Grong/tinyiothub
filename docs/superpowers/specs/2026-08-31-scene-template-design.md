# 场景包模板设计（应用商店本体模板扩充）

- 日期：2026-08-31
- 状态：已确认（用户逐节批准；CEO 审查 EXPANSION 模式，E2/E4/E5/E6 已并入范围）
- 背景：平台概念从"物联网平台"升级为"本体智能平台"，应用商店需扩充本体模板：园区、单体建筑、楼层等空间组合模板。
- CEO 计划：`~/.gstack/projects/Grong-tinyiothub/ceo-plans/2026-08-31-scene-templates.md`

## 1. 需求摘要

| 决策点 | 结论 |
|---|---|
| 模板粒度 | 完整场景包：本体树 + 告警规则 + AI Agent 配置 + 仪表盘/视图 |
| 模板体系 | 扩展现有 ThingTemplate，一张表管所有模板，不建独立场景包实体；组合模板由 `device_info` JSON 内含非空 `children` 判定 |
| 实例化产物 | 一次性快照，与模板脱钩，用户可自由修改；模板升级不影响已实例化内容 |
| 参数化 | 简单计数参数（int + min/max/default）+ `{index}` 命名展开 |
| 子树编码 | 递归嵌套 `children`，与"园区→建筑→楼层"心智模型一致；叶节点可 `template_ref` 引用现有设备模板 |
| 分发 | v1 仅内置官方场景包（`templates/builtin/scenes/`），marketplace 前端展示 + 实例化 |
| 实例化预览 | 支持 dry-run：参数表单实时预览"将创建 N 个本体"+ 树结构，确认后才落库（CEO 审查 E2） |
| 场景包组合 | 节点可 `scene_ref` 引用其他场景包（含参数映射、循环引用检测）（CEO 审查 E6） |
| 反向导出 | 支持把现有本体子树导出为模板（"另存为场景包"）（CEO 审查 E5） |
| category 词表 | campus/building/floor/room 等命名对齐 RealEstateCore/Brick 行业本体（附录 A，CEO 审查 E4） |

## 2. 数据模型：统一的本体节点定义 Schema

核心思路：不新增 scene 专用字段/表，把模板格式升级为统一的递归「本体节点定义」。device 模板和场景包共用同一 schema——场景包只是"带 children 的模板"。

### 2.1 模板文件格式

在现有内置模板格式（`templates/builtin/*/*.json`）上扩展，以下字段为新增可选字段：

```json
{
  "name": "smart_campus",
  "display_name": {"zh": "智慧园区", "en": "Smart Campus"},
  "version": "1.0.0", "author": "TinyIoT",
  "category": "scenes",
  "thing_category": "campus",
  "tags": ["campus", "building"],

  "parameters": [
    {"name": "building_count", "type": "int", "default": 2, "min": 1, "max": 10,
     "display_name": {"zh": "楼栋数量"}},
    {"name": "floor_count", "type": "int", "default": 5, "min": 1, "max": 15,
     "display_name": {"zh": "每栋楼层数（最终节点数受 500 上限约束）"}}
  ],

  "device_info": {"default_name_pattern": "{scene_name}", "required_fields": []},
  "properties": [],
  "commands": [],
  "events": [],
  "default_knowledge": "你是园区管家…",
  "resources": [
    {"name": "campus_map", "type": "image", "uri": "builtin://scenes/smart_campus/map.png"}
  ],
  "dashboard": {"cards": [{"property": "area"}, {"property": "plot_ratio"}]},
  "alarm_rules": [],

  "children": [
    {
      "key": "building",
      "category": "building",
      "count_param": "building_count",
      "device_info": {"default_name_pattern": "{index}号楼"},
      "properties": [], "events": [],
      "default_knowledge": "你是楼栋管家…",
      "resources": [], "alarm_rules": [], "dashboard": {},
      "children": [
        {
          "key": "floor", "category": "floor",
          "count_param": "floor_count",
          "device_info": {"default_name_pattern": "{index}F"},
          "children": [
            {"template_ref": "temperature_humidity_sensor", "count": 2}
          ]
        }
      ]
    }
  ]
}
```

### 2.2 字段语义

- **同一 schema 递归**：每个 child 节点支持顶层模板的全部能力块——`properties` / `commands` / `events` / `default_knowledge` / `resources` / `dashboard` / `alarm_rules` / `children`。Rust 侧为一个递归类型 `ThingNodeDef`，不区分"设备节点"与"空间节点"。
- **`key`**：节点在模板内的局部标识，v1 仅作可读标识与导出对照（跨节点告警引用**不实现**，见 §8 非目标）。
- **根节点的本体分类**：顶层 `category: "scenes"` 是**商店分类**；根节点产出本体的 category 用新增根级字段 `thing_category`（如 `campus`）声明，`thing_type` 可选（缺省按附录 A 由 `thing_category` 映射）。根节点 `display_name` 直接等于实例化输入的 `scene_name`。
- **`count_param`**：引用顶层 `parameters` 中的参数名，实例化时将该节点展开 N 份。与 `count` 互斥（同时出现 → 400，见 §6）。
- **`count`**：叶节点直接写死份数（如每层 2 个传感器）。
- **命名模式**：`device_info.default_name_pattern` 中支持 `{index}` 与 `{scene_name}`。`{index}` 在**每个父节点内独立从 1 开始**（1号楼的楼层与2号楼的楼层各自从 1F 起）；多级模式的替换在展开该层时进行（父层的 `{index}` 已在父层替换完毕，子层只替换自己的）。
- **`template_ref`**：叶节点快捷引用现有设备模板。查找顺序：当前 workspace 模板 → builtin 模板（按 `name`）；都找不到 → 400（模板缺陷）。引用时把该模板的 properties/commands/events 内联；节点缺省 `thing_type="device"`，`category` 取被引用模板的 `category`（原 `device_type` 列已在重命名迁移中移除，不可依赖）。
- **`scene_ref`**：节点快捷引用其他**场景包模板**（按 `name`，同样的 workspace→builtin 查找顺序），实例化时把被引用模板的根子树内联到该位置。可带 `param_mapping`（`{"被引用模板参数名": "本模板参数名"}`，把本模板参数值传过去；缺省用被引用模板默认值；**映射指向的本模板参数不存在 → 400**，指出参数名）。展开器维护引用栈，检测到环（A→B→A）报 400 并指出引用链路径；引用深度上限 5 层。首批内置模板保持内联不引用（简单可读），`scene_ref` 能力在 schema 与展开器中支持。
- **`parameters`**：仅允许 `type: "int"`，必填 `default`/`min`/`max`。
- **`resources`**：通用资源定义 `{name, type, uri}`。`type` 取值 `image` / `model3d` / `document` / `file`；实现时给现有 `ResourceType` 枚举（`crates/core/src/models/workspace.rs`）补 `image`/`model3d` 两个值，并同步放开创建接口校验（`apps/cloud/src/domains/tenant/workspace/handler.rs` 当前硬编码仅接受 `File`）与前端资源创建 UI；DB `resources.resource_type` 无 CHECK 约束，无需迁移。
- **`dashboard`**：v1 最小 schema：`{"cards": [{"property": "<属性名>"}]}`——属性卡片列表。不做 3D。
- **`alarm_rules`**：节点级告警规则定义，模板文件用简写 `{name, rule_type, condition, alarm_level, notification_config, property_ref}`。`property_ref` 引用**本节点**的属性名，实例化展开后映射为真实 `property_id`；`notification_config` 缺省为 `{}`（不启用通知，避免渠道必填校验失败）。`condition` 对齐 `AlarmCondition` 枚举的 serde JSON，例：`{"type":"threshold","operator":"greater_than","value":80.0}`。**实例化器映射**：简写 → `AlarmRule::new(name, description=None, thing_id, property_id, rule_type, condition, alarm_level, notification_config, workspace_id)`，写 DB 时列名为 `rule_name`/`condition_config`。v1 `rule_type` 仅允许 DB CHECK 约束内的取值（`threshold`/`range`/`change`/`offline`/`event`），模板作者用其他类型会在校验期报 400。

### 2.3 存储

- **不占用 `thing_type` 列做模板分类**：`thing_type` 按本体设计（2026-07-22-thing-ontology-design）的既定语义填**根节点产出的本体类型**（如 `space`），存量语义不变。
- **组合模板判定**：`thing_templates.device_info` 列对组合模板存**完整模板 JSON 全文**（根级含 `parameters`/`children`）；对常规设备模板仍存 `ThingInfo` JSON。判定规则：解析该列，JSON 根级含**非空** `children` 数组即组合模板；`children: []` 视为单本体模板。
- **解析类型分离**：现有 `ThingInfo` 是强类型结构（`default_name_pattern` 必填、无 parameters/children），**不扩展它**。新类型 `SceneTemplateFile` / `ThingNodeDef` 定义在 `crates/db/src/scene_template.rs`（与 `thing_template.rs` 同层，遵循"类型随 repo 住 db"约定），实例化器直接引用。entity 模板仍按现有 `ThingInfo` 解析。marketplace 列表在应用层解析 `device_info` 原文得出 `is_composition` 与 `parameter_count`。
- `parameters` / `children` 随 `device_info` JSON 列存储。**不新建表、不新增列**。
- **seed**：`template_categories` 是 `thing_templates.category` 的外键，需在 seed SQL（`crates/db/src/seed/system.sql`）中新增 `scenes` 类别一行。
- **文件字段名映射**：模板文件沿用现有内置文件的 `commands` 键；DB 列名为 `actions`，加载/写入时做 `commands ↔ actions` 映射（与现有 loader 一致）。

### 2.4 实例化产物落点

| 模板节点块 | 实例化后落点 |
|---|---|
| properties / commands | 现有 thing_properties / thing_commands 表 |
| `thing_type` | **DB 写入层直接设置**：db crate 新增内部函数 `create_thing_row_with_type(tx, req, thing_type)`（专供实例化器，不改 `core::CreateThingRequest` 公开 DTO）；节点可显式声明 `thing_type`，缺省按附录 A 映射（campus/floor/room/zone→`space`，building→`building`，设备节点→`device`）；`template_ref` 内联节点缺省 `thing_type="device"`，`category` 取被引用模板的 `device_type`（缺失时取其 `category`） |
| name / display_name | `name` 为命名模式替换后的机器名；`display_name` 取多语言模式替换后的 `zh` 值，缺 `zh` 取 `en`，再缺取 `name` |
| default_knowledge | v1 写入 things 的 `linked_data` JSON，仅记录；Thing Agent 人设消费路径随 Thing Agent 配置演进接入 |
| events（事件定义元数据） | v1 同样写入 `linked_data` JSON；运行时 `events` 表是日志，不存定义 |
| resources | 现有 `resources` 表（`ThingResource`）：`file_path = uri` 原样记录（含 `builtin://` 占位），`content = null`，`resource_type` 按模板 `type` 存储 |
| alarm_rules | 现有 alarm rules 表，`property_ref` 映射为真实 `property_id`，`thing_id`/`workspace_id` 由实例化上下文填充 |
| dashboard | v1 写入 `linked_data` JSON，前端读取渲染 |

**`linked_data` 合并策略**：`linked_data` 是**每个 Thing 行自己的列**——每个节点写入各自本体的 linked_data，节点之间无覆盖问题。单节点内按顶层键合并；`knowledge` / `event_defs` / `dashboard` 为实例化保留命名空间，冲突时实例化数据覆盖；其他已有键不动。

**`required_fields`**：设备模板中的 `required_fields`（如 address）仅供人工创建向导校验；场景包实例化时**忽略**（自动生成 name，address 留空），不阻断。

## 3. 实例化流程

### 3.1 入口与 API

`POST /api/marketplace/thing-templates/{id}/instantiate`（路由前缀 `/api`，以 `apps/cloud/src/router.rs` 实际挂载为准，无 `/v1` 版本前缀）

```json
{
  "scene_name": "张江科技园",
  "parent_id": null,
  "parameter_values": {"building_count": 2, "floor_count": 10},
  "dry_run": false
}
```

- `scene_name`：必填，根节点名称。
- `parent_id`：可选，把整棵树挂到现有本体下。**校验**：parent 存在且属于当前 workspace（不满足返回 400）。
- `parameter_values`：可选，缺省用参数默认值。
- `dry_run`：可选，默认 false。为 true 时只展开不落库，返回预览。

**dry_run=true 响应**：`{node_count, tree_preview, warnings[]}`。

**dry_run=false 响应**：`{node_count, root_thing_id, tree_preview, warnings[]}`。

`node_count` 两种模式同语义：展开后的本体总数，**含根节点**。

**`tree_preview` 格式**：纯文本，每行一个节点，2 空格缩进表示层级，格式为 `<display_name> (<category>)`：

```
张江科技园 (campus)
  1号楼 (building)
    1F (floor)
      温湿度传感器 1 (sensors)
```

约束：同一参数输入下 dry-run 的展开结果与实际落库**必须一致**（同一展开器、同一纯函数）。`tree_preview` 仅供展示，前端不解析；生成时剔除 display_name 中的换行与首尾空格，括号替换为全角。

### 3.2 后端流程

新增 `SceneInstantiator`，位于 `apps/cloud/src/domains/marketplace/`，与现有 `ThingTemplateInstaller` 并列：

1. **加载与校验**：取模板（`device_info` 原文按 `SceneTemplateFile` 解析）→ 校验参数值（类型、min/max）→ 校验所有 `template_ref`/`scene_ref` 引用存在（workspace→builtin 顺序）→ 检测 scene_ref 环与深度。
2. **展开（纯函数，无 IO）**：递归遍历根节点，按 `count_param`/`count`/`scene_ref` 展开 children，输出 `ExpansionResult { nodes: Vec<ExpandedNode>, total_count, tree_preview, warnings }`。`ExpandedNode` 含 name/display_name/category/thing_type/properties/commands/event_defs/knowledge/resources/dashboard/alarm_rules 及**临时父子链接**（展开序号）。`{index}`、`{scene_name}` 在此替换。
3. **落库（单事务）**：显式开启一个 sqlx `Transaction`，贯穿全部写入；各表（things/properties/commands/resources/alarm_rules）新增接受 `&mut Transaction` 的内部插入函数（现有单条插入接口不直接复用，避免隐式各自提交）。按拓扑序（父先于子）创建 Thing，临时链接映射为真实 `parent_id`，同时写入 `thing_type`；随后创建属性/命令/资源/alarm_rules，最后写 `linked_data`（knowledge/event_defs/dashboard）。
4. **名称冲突处理**：`things.name` 有 `(workspace, name)` 唯一索引。算法：先剥离原名末尾的 `-N` 后缀得到 base，再**在事务内 SELECT 探测** `base`、`base-2`、`base-3`…，取第一个空闲名插入，记入 warnings；探测超过 10 个仍冲突返回 400「同名冲突过多，请手动指定名称」。SELECT 探测是快路径，**保留唯一约束捕获作兜底**：并发下两个事务可能探到同一名（TOCTOU），插入撞唯一约束时重新探测重试（同上限 10 次）。
5. **返回结果**。

### 3.3 关键决策

- **先全量展开、再单事务落库**：任何失败整体回滚，不留半棵树；展开器是纯函数，可直接单元测试，dry-run 与真实落库共用同一展开路径与同一 `ExpansionResult`。
- **规模护栏**：`total_count > 500` 直接拒绝（400），防止参数组合爆炸。scene_ref 展开计入。
- **warnings 非阻断**：次级问题（如引用模板已停用）跳过该节点并记入 warnings，主流程继续。

### 3.5 可观测性

- **结构化日志（tracing）**：展开与落库的入口/出口各一条——模板 id、参数值、node_count、耗时、warnings 数；失败时带错误类别与引用链路径。
- **指标**：`scene_instantiations_total{template, result}` 计数器（result = success/validation_error/too_large/tx_failed）。
- 目标：上线 3 周后仅凭日志可复盘任何一次失败实例化。

### 3.4 反向导出（E5：另存为场景包）

`POST /api/things/{id}/export-as-template`：把指定本体及其整棵子树导出为场景包模板 JSON（§2.1 文件格式）。**校验：该 thing 必须属于调用者 workspace，否则 404**（防 IDOR）。

- **收集**：遍历子树，每个本体收集 properties/commands/linked_data（knowledge/event_defs/dashboard 还原）/resources/alarm_rules，转为节点定义。
- **category / thing_type 还原**：节点 `category` 直接取 `things.category`；`thing_type` 仅当实际值与附录 A 缺省映射**不一致**时才显式写入节点（保持一致则省略，由缺省映射还原）。
- **命名泛化（启发式，明确规则）**：同一父节点下的兄弟节点，v1 **只处理单段数字前缀或后缀**（如 `1号楼`/`2号楼`、`1F`/`2F`）：去掉该数字串后剩余部分全部相等、且数字构成从 1 开始的连续序列 → 泛化为 `{index}号楼` + `count=N`。数字在中间（`A1室`）或多段数字（`1F-01`）**不泛化**，保留原名并记入 warnings。
- **落点**：导出结果为模板文件 JSON 下载；用户编辑后走现有 `import_export` 流程注册为 workspace 模板。导出端点本身不入库。
- **边界**：子树 > 500 节点拒绝；设备本体不逆向还原 `template_ref`，仅导出属性/命令/事件定义原文。

## 4. 首批内置场景包

存放 `templates/builtin/scenes/`，随现有 seed 流程注册（`is_builtin=1`）。**seed 方式：SQL 内嵌**——当前 `crates/db/src/seed.rs` 只执行 `seed/system.sql`，现有 `templates/builtin/*/*.json` 本就是 SQL seed 的源文件、运行时不读文件；保持一致，场景包 JSON 内容内嵌进 system.sql（`device_info` 列存完整 JSON 全文），并新增 `scenes` 类别行。各模板 seed 行的 `thing_type`：smart_campus=`space`、smart_building=`building`、smart_floor=`space`。

首批 3 个，覆盖"由大到小"三个粒度，全部只引用现有 `temperature_humidity_sensor` 设备模板，不为场景包新增设备模板；均不使用 `scene_ref`（保持内联可读）：

1. **智慧园区（smart_campus）**：园区 → 楼栋 ×N → 楼层 ×N → 温湿度传感器 ×2/层。参数 `building_count`（默认 2）、`floor_count`（默认 5）。园区级：属性（占地面积、容积率）、knowledge（园区管家人设）、alarm_rules（能耗异常）；楼栋级：knowledge、alarm_rules（温度超阈值）；楼层级：resources（楼层平面图占位）。
2. **单体建筑（smart_building）**：建筑 → 楼层 ×N → 温湿度传感器 ×2/层。参数 `floor_count`（默认 10）。
3. **楼层（smart_floor）**：楼层 → 房间 ×N（category=room，纯空间节点）。参数 `room_count`（默认 8）。

dashboard 块 v1 只放 `{"cards": [...]}` 属性卡片配置，不接 3D 场景。

## 5. API 与前端

### 5.1 API 变更（均在现有 marketplace 域内；前缀 `/api`，以 router.rs 实际挂载为准）

| 端点 | 变更 |
|---|---|
| `GET /api/marketplace/thing-templates` | 列表项增加 `is_composition`（bool，应用层解析 `device_info` 原文判定）与 `parameter_count`；支持 `?composition=true` 过滤。**过滤语义**：v1 模板量少，全量取出后应用层过滤再分页（DB 层不加过滤条件，分页计数基于过滤后结果） |
| `GET /api/marketplace/thing-templates/{id}` | 新增详情端点：组合模板含 `parameters`（供前端渲染表单）与结构摘要 |
| `POST /api/marketplace/thing-templates/{id}/instantiate` | 新增；对非组合模板调此端点返回 400（走原有 install 流程）。支持 `dry_run` |
| `POST /api/things/{id}/export-as-template` | 新增（E5 反向导出）：把该本体及其子树导出为场景包模板 JSON 下载 |

**结构摘要定义**：`{parameter_count, max_depth}`——`parameter_count` 为顶层 `parameters` 个数；`max_depth` 为**模板定义层的静态深度**（根为 1；`template_ref` 叶计 1 层，`scene_ref` 不深入被引用子树、计 1 层）。注意口径：`max_depth` 是静态结构指标，`node_count` 是运行时展开口径，两者不比较。示例：smart_campus（campus→building→floor→sensor）`max_depth = 4`。前端卡片显示如"2 参数 · 4 层结构"。

### 5.2 前端（`web/src/ui/views/marketplace.ts`、`web/src/api/marketplace.ts`）

- 商店页分 Tab：「设备模板」/「场景包」（按 `is_composition` 过滤；场景包卡片显示结构摘要）。
- 场景包「使用模板」→ 参数对话框：按 `parameters` 动态生成表单（整数输入 + min/max 校验 + 多语言 display_name）+ 根节点名称输入 + 可选父本体选择；参数变化时调 `dry_run=true` 实时显示"将创建 N 个本体"与 `tree_preview` 文本树（**300ms 防抖，dry-run 进行中禁用提交按钮**）。
- 提交调 instantiate API（**提交后禁用按钮防双击**，服务端 v1 不做幂等）；成功跳转新根本体详情页（展示 `tree_preview`）；有 warnings 先展示警告列表再跳转。
- 本体详情页加「另存为场景包」入口，调 export-as-template 下载模板 JSON。

**不做**：商店卡片可视化树预览（用文本摘要）、用户自制场景包上传 UI。

## 6. 错误处理

复用现有 `MarketplaceError` / `TemplateError`：

| 场景 | 行为 |
|---|---|
| 参数缺失/类型错误/超 min/max | 400，逐字段返回错误信息 |
| 节点同时声明 `count` 与 `count_param` | 400，指出节点 key |
| 对非组合模板调 instantiate | 400，提示走 install 流程 |
| `template_ref` / `scene_ref` 引用不存在（workspace 与 builtin 都没有） | 400，指出引用名 |
| `scene_ref` 检测到引用环 / 深度 > 5 | 400，指出引用链路径 |
| 展开后 `total_count` > 500 | 400，提示缩小参数 |
| `parent_id` 不存在或不属于当前 workspace | 400 |
| 名称冲突（同 workspace 下重名） | 事务内 SELECT 探测下一个可用 `-N` 后缀（≤10），记 warning；超限返回 400「同名冲突过多，请手动指定名称」 |
| 反向导出：命名模式无法泛化 | 保留原名导出，记入 warnings |
| 落库中途失败 | 事务回滚，500，不留半棵树 |
| 次级问题（引用模板已停用等） | 跳过该节点，记入 warnings，主流程继续 |

## 7. 测试

1. **展开器单元测试**（纯函数，重点）：计数参数展开、`{index}` 每层独立序号、`{scene_name}` 替换、多层嵌套父子链接、`template_ref` 内联（workspace→builtin 查找顺序）、`scene_ref` 内联与参数映射、引用环/超深拒绝、超上限拒绝、缺参数报错、空 children 判为非组合。
2. **dry-run 一致性测试**：同一输入下 dry-run 的 `node_count`/`tree_preview` 与实际落库结果一致。
3. **模板文件校验测试**：`templates/builtin/scenes/*.json` 全部可按 `SceneTemplateFile` 解析并通过 schema 校验。
4. **反向导出 round-trip 测试**：实例化 → 导出 → 重新实例化，断言结构等价（节点数、层级、属性集）；命名泛化启发式的正/反例。
5. **集成测试**：调 instantiate API → 断言库中树结构正确（数量、层级、`thing_type` 映射、属性、alarm_rules、resources 表记录、linked_data）；事务回滚用例（注入中途失败）；名称冲突自动追加序号用例。**测试环境必须跑完整迁移链**（baseline 中子表 FK 指向 `devices`，经 `20260825000001` 重命名为 `things`，仅跑 baseline 会 FK 不一致）。
6. **前端手测**：参数表单渲染、dry-run 实时预览、warnings 展示、跳转、另存为场景包下载。

## 8. 非目标（v1 明确不做）

- 用户自制场景包上传/分享 UI、模板版本升级推送
- 商店卡片的可视化树预览（卡片用文本摘要；实例化的 dry-run 树预览**属于范围**）
- 3D 场景视图接入
- 资源文件的真实上传与托管（v1 resources 仅记录 uri）
- 实例化后自动挂模拟数据流（CEO 审查 E1 裁定跳过）
- Agent 人设自动继承层级上下文（CEO 审查 E3 裁定跳过）
- `default_knowledge`/`event_defs`/`dashboard` 的独立存储表（v1 落各节点自己的 `linked_data` JSON，仅记录）
- 模板 `tags` 向实例化产物传播（v1 不创建 tag 绑定）
- 跨节点告警引用（`key` 仅作可读标识；alarm_rules 的 `property_ref` 只引用本节点属性）

## 附录 A · category 词表与 thing_type 映射（对齐行业本体）

空间类 category 命名对齐 RealEstateCore / Brick 的通行词表，为未来 DTDL 映射留口：

| category | thing_type | RealEstateCore 对应 | 说明 |
|---|---|---|---|
| `campus` | `space` | Campus | 园区 |
| `building` | `building` | Building | 单体建筑 |
| `floor` | `space` | Level/Storey | 楼层 |
| `room` | `space` | Room | 房间 |
| `zone` | `space` | Zone | 跨房间的逻辑分区（备用） |

设备类节点 `thing_type` 恒为 `device`；category 维持现有约定（sensors/cameras/controllers/robots），不受此表约束。节点可在模板中显式声明 `thing_type` 覆盖缺省映射。新增空间类 category 前先查本表。

## Reviewer Concerns（对抗性审查遗留，用户裁定保留）

第 1 轮独立审查（质量分 6/10 → 修正后复审）提出的范围异议，经 CEO 裁定仪式由用户明确采纳，予以保留：

1. `scene_ref` 跨模板引用在 v1 无实际使用场景（首批模板内联）——保留理由：用户裁定 E6 采纳，展开器能力先行。
2. 反向导出（E5）超出最小可用范围——保留理由：用户裁定采纳，定位为模板生态供给侧起点。
3. `resources` 块 v1 仅记录 uri、无真实托管——保留理由：楼层平面图占位对演示体验有价值，写入成本低。
