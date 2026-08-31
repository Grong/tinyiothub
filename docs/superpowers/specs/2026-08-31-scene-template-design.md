# 场景包模板设计（应用商店本体模板扩充）

- 日期：2026-08-31
- 状态：已确认（用户逐节批准）
- 背景：平台概念从"物联网平台"升级为"本体智能平台"，应用商店需扩充本体模板：园区、单体建筑、楼层等空间组合模板。

## 1. 需求摘要

| 决策点 | 结论 |
|---|---|
| 模板粒度 | 完整场景包：本体树 + 告警规则 + AI Agent 配置 + 仪表盘/视图 |
| 模板体系 | 扩展现有 ThingTemplate，一张表管所有模板，不建独立场景包实体；组合模板由 `device_info` JSON 内含 `children` 判定 |
| 实例化产物 | 一次性快照，与模板脱钩，用户可自由修改；模板升级不影响已实例化内容 |
| 参数化 | 简单计数参数（int + min/max/default）+ `{index}` 命名展开 |
| 子树编码 | 递归嵌套 `children`，与"园区→建筑→楼层"心智模型一致；叶节点可 `template_ref` 引用现有设备模板 |
| 分发 | v1 仅内置官方场景包（`templates/builtin/scenes/`），marketplace 前端展示 + 实例化 |

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
  "tags": ["campus", "building"],

  "parameters": [
    {"name": "building_count", "type": "int", "default": 2, "min": 1, "max": 20,
     "display_name": {"zh": "楼栋数量"}},
    {"name": "floor_count", "type": "int", "default": 5, "min": 1, "max": 50,
     "display_name": {"zh": "每栋楼层数"}}
  ],

  "device_info": {"default_name_pattern": "{scene_name}", "required_fields": []},
  "properties": [],
  "commands": [],
  "events": [],
  "default_knowledge": "你是园区管家…",
  "resources": [
    {"name": "campus_map", "type": "image", "uri": "builtin://scenes/smart_campus/map.png"}
  ],
  "dashboard": {},
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

- **同一 schema 递归**：每个 child 节点支持顶层模板的全部能力块——`properties` / `commands` / `events` / `default_knowledge`（AI Agent 人设/知识）/ `resources` / `dashboard` / `alarm_rules` / `children`。Rust 侧为一个递归类型 `ThingNodeDef`，不区分"设备节点"与"空间节点"。
- **`key`**：节点在模板内的局部标识，供告警规则等跨节点引用。
- **`count_param`**：引用顶层 `parameters` 中的参数名，实例化时将该节点展开 N 份。
- **`count`**：叶节点直接写死份数（如每层 2 个传感器）。
- **命名模式**：`device_info.default_name_pattern` 中支持 `{index}`（兄弟节点间序号，从 1 开始）与 `{scene_name}`（实例化时用户输入的根名称）。
- **`template_ref`**：叶节点快捷引用现有设备模板（按 `name`），实例化时等价于把该模板的 properties/commands/events 内联。
- **`parameters`**：仅允许 `type: "int"`，必填 `default`/`min`/`max`。
- **`resources`**：通用资源定义 `{name, type, uri}`，`type` 取值如 `image` / `model3d` / `document`。

### 2.3 存储

- **不占用 `thing_type` 列做模板分类**：`thing_type` 按本体设计（2026-07-22-thing-ontology-design）的既定语义填根节点类型（如 `space`），存量语义不变。
- **组合模板判定**：`device_info` JSON 内含 `children` 即组合模板（schema 层面唯一真相，零新列）；marketplace 列表在应用层解析 `device_info` 得出 `is_composition` 与 `parameter_count`。v1 模板数量少，应用层过滤足够。
- `parameters` / `children` 等组合数据序列化进现有 `device_info` JSON 列（它本就是 JSON 容器）。**不新建表、不新增列**。
- `resources` 实例化后写入**现有 `resources` 表**（`ThingResource`：`thing_id` / `type` / `name` / `file_path` / `content` / `tags`）。模板 resources 块字段对齐该表：`{name, type, uri}` 中 `uri` 存入 `file_path`；v1 不做文件上传，`builtin://` URI 原样记录。

## 3. 实例化流程

### 3.1 入口与 API

`POST /api/marketplace/thing-templates/{id}/instantiate`

```json
{
  "scene_name": "张江科技园",
  "parent_id": null,
  "parameter_values": {"building_count": 2, "floor_count": 10}
}
```

- `scene_name`：必填，根节点名称。
- `parent_id`：可选，把整棵树挂到现有本体下。
- `parameter_values`：可选，缺省用参数默认值。

响应：`{root_thing_id, created_count, warnings[]}`。

### 3.2 后端流程

新增 `SceneInstantiator`，位于 `apps/cloud/src/domains/marketplace/`，与现有 `ThingTemplateInstaller` 并列：

1. **加载与校验**：取模板 → 校验参数值（类型、min/max）→ 校验所有 `template_ref` 引用的设备模板存在。
2. **展开（纯函数，无 IO）**：递归遍历根节点，按 `count_param`/`count` 展开 children，生成「待创建本体清单」，每项含 name/display_name/category/properties/commands/events/knowledge/resources/dashboard/alarm_rules 及临时父子链接（按展开顺序的序号）。`{index}`、`{scene_name}` 在此替换。
3. **落库（单事务）**：按拓扑序（父先于子）创建 Thing，临时链接映射为真实 `parent_id`；随后批量创建属性/命令/事件/default_knowledge/alarm_rules；resources 写入 `resources` 表（`thing_id` 指向新建本体）。
4. **返回结果**。

### 3.3 关键决策

- **先全量展开、再单事务落库**：任何失败整体回滚，不留半棵树；展开器是纯函数，可直接单元测试。
- **规模护栏**：展开后总节点数 > 500 直接拒绝（400），防止参数组合爆炸。
- **warnings 非阻断**：次级问题（如引用模板已停用）跳过该节点并记入 warnings，主流程继续。

## 4. 首批内置场景包

存放 `templates/builtin/scenes/`，随现有 seed 流程注册（`is_builtin=1`）。首批 3 个，覆盖"由大到小"三个粒度，全部只引用现有 `temperature_humidity_sensor` 设备模板，不为场景包新增设备模板：

1. **智慧园区（smart_campus）**：园区 → 楼栋 ×N → 楼层 ×N → 温湿度传感器 ×2/层。参数 `building_count`（默认 2）、`floor_count`（默认 5）。园区级：属性（占地面积、容积率）、knowledge（园区管家人设）、alarm_rules（能耗异常）；楼栋级：knowledge、alarm_rules（温度超阈值）；楼层级：resources（楼层平面图占位）。
2. **单体建筑（smart_building）**：建筑 → 楼层 ×N → 温湿度传感器 ×2/层。参数 `floor_count`（默认 10）。
3. **楼层（smart_floor）**：楼层 → 房间 ×N（category=room，纯空间节点）。参数 `room_count`（默认 8）。

dashboard 块 v1 只放简单视图配置（属性卡片列表），不接 3D 场景。

## 5. API 与前端

### 5.1 API 变更（均在现有 marketplace 域内）

| 端点 | 变更 |
|---|---|
| `GET /api/marketplace/thing-templates` | 列表项增加 `is_composition`（bool，应用层解析 `device_info` 判定）与 `parameter_count`；支持 `?composition=true` 过滤 |
| `GET /api/marketplace/thing-templates/{id}` | 新增详情端点：组合模板含 `parameters`（供前端渲染表单）与子树文本摘要 |
| `POST /api/marketplace/thing-templates/{id}/instantiate` | 新增；对非组合模板调此端点返回 400（走原有 install 流程） |

### 5.2 前端（`web/src/ui/views/marketplace.ts`、`web/src/api/marketplace.ts`）

- 商店页分 Tab：「设备模板」/「场景包」（按 `is_composition` 过滤；场景包卡片显示结构摘要，如"2 参数 · 4 层结构"）。
- 场景包「使用模板」→ 参数对话框：按 `parameters` 动态生成表单（整数输入 + min/max 校验 + 多语言 display_name）+ 根节点名称输入 + 可选父本体选择。
- 提交调 instantiate API；成功跳转新根本体详情页；有 warnings 先展示警告列表再跳转。

**不做**：场景包可视化树预览（v1 用文本摘要）、用户自制场景包上传 UI。

## 6. 错误处理

复用现有 `MarketplaceError` / `TemplateError`：

| 场景 | 行为 |
|---|---|
| 参数缺失/类型错误/超 min/max | 400，逐字段返回错误信息 |
| `template_ref` 引用不存在 | 400（加载期硬校验，属模板缺陷） |
| 展开后总节点数 > 500 | 400，提示缩小参数 |
| 名称冲突（同 workspace 下重名） | 沿用现有 install 冲突策略：自动追加序号，记 warning |
| 落库中途失败 | 事务回滚，500，不留半棵树 |
| 次级问题（引用模板已停用等） | 跳过该节点，记入 warnings，主流程继续 |

## 7. 测试

1. **展开器单元测试**（纯函数，重点）：计数参数展开、`{index}`/`{scene_name}` 替换、多层嵌套父子链接、`template_ref` 内联、超上限拒绝、缺参数报错。
2. **模板文件校验测试**：`templates/builtin/scenes/*.json` 全部可解析并通过 schema 校验。
3. **集成测试**：调 instantiate API → 断言库中树结构正确（数量、层级、属性、alarm_rules、resources 表记录）；事务回滚用例（注入中途失败）。
4. **前端手测**：参数表单渲染、warnings 展示、跳转。

## 8. 非目标（v1 明确不做）

- 用户自制场景包上传/分享、模板版本升级推送
- 场景包可视化树预览
- 3D 场景视图接入
- 资源文件的真实上传与托管（v1 resources 仅记录 uri）
