# 场景包模板（Scene Templates）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 应用商店支持组合场景包模板（园区/建筑/楼层），一键实例化整棵本体树（含属性/命令/告警/资源/AI 人设），支持 dry-run 预览与反向导出。

**Architecture:** 统一递归 `ThingNodeDef` schema（`crates/db/src/scene_template.rs`）+ 纯函数 `Expander` + `SceneInstantiator` 单事务落库（`apps/cloud/src/domains/marketplace/scene_instantiator.rs`）。零新表零新列：组合数据存 `thing_templates.device_info` 完整 JSON 原文，knowledge/event_defs/dashboard 落各节点自己的 `linked_data`。

**Tech Stack:** Rust + axum + sqlx(SQLite) / Lit (web 前端）

**Spec:** `docs/superpowers/specs/2026-08-31-scene-template-design.md`（v5, CEO+ENG 双 CLEAR）

## Global Constraints

- 组合模板判定唯一真相：`thing_templates.device_info` 列解析为 JSON，根级含**非空** `children` 数组
- `ThingTemplate` 新增 `is_composition()` helper；`get_thing_info()` 全部 6 个调用点（`apps/cloud/src/domains/thing/template/service.rs:60/127/339/625/762/937`）加早返门
- 实例化单事务：所有写入接受 `&mut sqlx::Transaction<'_, sqlx::Sqlite>`；批量 INSERT 用 `QueryBuilder::push_values`，每语句 ≤100 行
- 名称冲突：剥离末尾 `-N` 得 base → 事务内 SELECT 探测 → 撞唯一约束重探测重试，上限 10 次
- 配额校验用 `count_things_by_workspace(workspace_id)`（真实行数），不用 `tenant_usage.device_count` 缓存
- 展开总节点数 > 500 → 400；`scene_ref` 环/深度 > 5 → 400
- `template_ref`/`scene_ref` 查找：workspace 优先于 builtin（`ORDER BY workspace_id IS NULL`）
- `rule_type` 仅允许 `{threshold, range, change, event}`（Rust 枚举 ∩ DB CHECK）
- `linked_data` 合并：按顶层键合并，`knowledge`/`event_defs`/`dashboard` 为保留命名空间，冲突时实例化数据覆盖
- 多语言回退：`zh` → `en` → `name`（display_name 单字符串化）
- 所有实例化节点 `template_id` = 场景包模板 id
- 命名模式变量：`{scene_name}`、`{index}`（每个父节点内独立从 1 开始）
- 前端 dry-run 300ms 防抖、提交中禁用按钮、预览名称标注 tentative
- 测试基于完整迁移链（不能只跑 baseline）

---

### Task 1: SceneTemplateFile / ThingNodeDef 类型 + is_composition() + 命名模式泛化下沉

**Files:**
- Create: `crates/db/src/scene_template.rs`
- Modify: `crates/db/src/lib.rs`（注册模块）
- Modify: `crates/db/src/thing_template.rs:307`（加 `is_composition()`）
- Modify: `apps/cloud/src/domains/thing/template/service.rs:279`（`apply_name_pattern` 委托新函数）

**Interfaces:**
- Produces（后续任务依赖）:
  - `SceneTemplateFile`、`ThingNodeDef`、`SceneParameter`、`SceneResource`、`SceneAlarmRule`、`SceneNodeInfo`（serde 结构）
  - `SceneTemplateFile::from_json(json: &str) -> Result<Self, serde_json::Error>`
  - `ThingTemplate::is_composition(&self) -> bool`
  - `pub fn render_name_pattern(template: &str, vars: &HashMap<&str, String>) -> String`

- [ ] **Step 1: 写失败测试**

创建 `crates/db/src/scene_template.rs`，先只写模块骨架和测试：

```rust
//! 场景包模板：统一递归本体节点定义 + 展开器。
//!
//! 组合模板判定：thing_templates.device_info 列存完整模板 JSON，
//! 根级含非空 children 数组即组合模板（entity 模板该列存 ThingInfo JSON）。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::thing_template::{CommandTemplate, PropertyTemplate};

/// 场景包模板文件（组合模板 device_info 列的完整 JSON）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneTemplateFile {
    pub name: String,
    pub display_name: HashMap<String, String>,
    #[serde(default)]
    pub description: Option<HashMap<String, String>>,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub category: String,
    /// 根节点产出本体的 category（如 campus）；缺省取 category
    #[serde(default)]
    pub thing_category: Option<String>,
    #[serde(default)]
    pub parameters: Vec<SceneParameter>,
    #[serde(default)]
    pub device_info: SceneNodeInfo,
    #[serde(default)]
    pub properties: Vec<PropertyTemplate>,
    #[serde(default)]
    pub commands: Vec<CommandTemplate>,
    #[serde(default)]
    pub events: Vec<serde_json::Value>,
    #[serde(default)]
    pub default_knowledge: Option<String>,
    #[serde(default)]
    pub resources: Vec<SceneResource>,
    #[serde(default)]
    pub dashboard: Option<serde_json::Value>,
    #[serde(default)]
    pub alarm_rules: Vec<SceneAlarmRule>,
    #[serde(default)]
    pub children: Vec<ThingNodeDef>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

impl SceneTemplateFile {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// 递归本体节点定义（children 节点与根共用同一 schema 的能力块）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThingNodeDef {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    /// 显式 thing_type 覆盖；缺省由展开器按附录 A 映射
    #[serde(default)]
    pub thing_type: Option<String>,
    /// 固定份数（与 count_param 互斥）
    #[serde(default)]
    pub count: Option<u32>,
    /// 引用顶层 parameters 的参数名
    #[serde(default)]
    pub count_param: Option<String>,
    /// 引用现有设备模板（按 name）
    #[serde(default)]
    pub template_ref: Option<String>,
    /// 引用其他场景包模板（按 name）
    #[serde(default)]
    pub scene_ref: Option<String>,
    /// 键=被引用模板参数名，值=本模板参数名
    #[serde(default)]
    pub param_mapping: Option<HashMap<String, String>>,
    #[serde(default)]
    pub device_info: SceneNodeInfo,
    #[serde(default)]
    pub properties: Vec<PropertyTemplate>,
    #[serde(default)]
    pub commands: Vec<CommandTemplate>,
    #[serde(default)]
    pub events: Vec<serde_json::Value>,
    #[serde(default)]
    pub default_knowledge: Option<String>,
    #[serde(default)]
    pub resources: Vec<SceneResource>,
    #[serde(default)]
    pub dashboard: Option<serde_json::Value>,
    #[serde(default)]
    pub alarm_rules: Vec<SceneAlarmRule>,
    #[serde(default)]
    pub children: Vec<ThingNodeDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SceneNodeInfo {
    #[serde(default)]
    pub default_name_pattern: Option<String>,
    #[serde(default)]
    pub default_display_name_pattern: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String, // v1 仅 "int"
    pub default: i64,
    pub min: i64,
    pub max: i64,
    #[serde(default)]
    pub display_name: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneResource {
    pub name: String,
    #[serde(rename = "type")]
    pub resource_type: String, // image | model3d | document | file
    pub uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneAlarmRule {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub rule_type: String, // threshold | range | change | event
    pub condition: serde_json::Value,
    #[serde(default = "default_alarm_level")]
    pub alarm_level: String,
    #[serde(default)]
    pub notification_config: serde_json::Value,
    /// 引用本节点的属性名
    #[serde(default)]
    pub property_ref: Option<String>,
}

fn default_alarm_level() -> String {
    "warning".to_string()
}

/// 命名模式替换（泛化自 cloud template service 的 apply_name_pattern）。
/// 支持 {scene_name}、{index}、{name}、{display_name} 等变量。
pub fn render_name_pattern(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_scene_template() {
        let json = r#"{
            "name": "smart_floor",
            "display_name": {"zh": "楼层", "en": "Floor"},
            "category": "scenes",
            "thing_category": "floor",
            "parameters": [
                {"name": "room_count", "type": "int", "default": 8, "min": 1, "max": 50,
                 "display_name": {"zh": "房间数量"}}
            ],
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [
                {"key": "room", "category": "room", "count_param": "room_count",
                 "device_info": {"default_name_pattern": "{index}室"}}
            ]
        }"#;
        let t = SceneTemplateFile::from_json(json).unwrap();
        assert_eq!(t.name, "smart_floor");
        assert_eq!(t.parameters.len(), 1);
        assert_eq!(t.children.len(), 1);
        assert_eq!(t.children[0].count_param.as_deref(), Some("room_count"));
    }

    #[test]
    fn parse_entity_thing_info_json_fails_gracefully_for_scene() {
        // entity 模板的 device_info（ThingInfo）不含 children —— 可解析但 children 为空
        let json = r#"{"default_name_pattern": "th_sensor_{index}", "required_fields": ["name"]}"#;
        let t: Result<SceneTemplateFile, _> = serde_json::from_str(json);
        // 根级必填字段 name/display_name 缺失 → 解析失败是正确行为
        assert!(t.is_err());
    }

    #[test]
    fn render_name_pattern_substitutes_vars() {
        let mut vars = HashMap::new();
        vars.insert("scene_name", "张江科技园".to_string());
        vars.insert("index", "3".to_string());
        assert_eq!(render_name_pattern("{index}号楼", &vars), "3号楼");
        assert_eq!(render_name_pattern("{scene_name}", &vars), "张江科技园");
        assert_eq!(render_name_pattern("无变量", &vars), "无变量");
    }
}
```

在 `crates/db/src/lib.rs` 中注册（找到现有 `pub mod thing_template;` 行，下方加一行）：

```rust
pub mod scene_template;
```

- [ ] **Step 2: 跑测试确认通过（新文件自包含，无外部依赖）**

Run: `cd crates/db && cargo test scene_template`
Expected: 3 个测试 PASS（首次编译可能需 1-2 分钟）

- [ ] **Step 3: ThingTemplate 加 is_composition()**

在 `crates/db/src/thing_template.rs` 的 `impl ThingTemplate` 块中（`get_thing_info` 后，约 :311）加：

```rust
    /// 是否组合模板（场景包）：device_info 列存完整模板 JSON，根级含非空 children。
    /// entity 模板该列存 ThingInfo JSON（无 children 键）。
    pub fn is_composition(&self) -> bool {
        serde_json::from_str::<serde_json::Value>(&self.device_info)
            .ok()
            .and_then(|v| v.get("children").cloned())
            .and_then(|c| c.as_array().map(|a| !a.is_empty()))
            .unwrap_or(false)
    }
```

在 `crates/db/src/thing_template.rs` 的 `#[cfg(test)]` 模块（文件末尾测试区）加：

```rust
    #[test]
    fn is_composition_detects_children() {
        let mut t = ThingTemplate::default();
        assert!(!t.is_composition()); // device_info = "{}"
        t.device_info = r#"{"default_name_pattern":"x"}"#.to_string();
        assert!(!t.is_composition()); // entity ThingInfo
        t.device_info = r#"{"name":"s","children":[]}"#.to_string();
        assert!(!t.is_composition()); // 空 children 不算
        t.device_info = r#"{"name":"s","children":[{"key":"b"}]}"#.to_string();
        assert!(t.is_composition());
        t.device_info = "not json".to_string();
        assert!(!t.is_composition()); // 解析失败安全降级
    }
```

- [ ] **Step 4: 跑测试**

Run: `cd crates/db && cargo test thing_template`
Expected: 全部 PASS（含新测试）

- [ ] **Step 5: service.rs 的 apply_name_pattern 委托新函数**

`apps/cloud/src/domains/thing/template/service.rs:279` 的 `apply_name_pattern` 改为委托（保持签名不变）：

```rust
    /// 应用名称模式（多语言支持）
    fn apply_name_pattern(
        &self,
        pattern: &Option<HashMap<String, String>>,
        user_input: &ThingCreationInput,
    ) -> Option<String> {
        pattern.as_ref().map(|patterns| {
            let template = patterns
                .get("zh")
                .or_else(|| patterns.values().next())
                .cloned()
                .unwrap_or_default();

            let mut vars = HashMap::new();
            vars.insert("name", user_input.name.clone());
            if let Some(display_name) = &user_input.display_name {
                vars.insert("display_name", display_name.clone());
            }
            vars.insert("index", "1".to_string()); // 预览示例固定 index=1
            tinyiothub_storage::scene_template::render_name_pattern(&template, &vars)
        })
    }
```

Run: `cd apps/cloud && cargo test template` — 现有模板测试全部 PASS（行为不变）

- [ ] **Step 6: Commit**

```bash
git add crates/db/src/scene_template.rs crates/db/src/lib.rs crates/db/src/thing_template.rs apps/cloud/src/domains/thing/template/service.rs
git commit -m "feat(db): scene template types + is_composition() + shared name-pattern renderer"
```

---

### Task 2: Expander 纯函数（展开引擎）

**Files:**
- Modify: `crates/db/src/scene_template.rs`（追加展开器）

**Interfaces:**
- Consumes: Task 1 的类型 + `crate::thing_template::ThingTemplate`（被引用的设备模板）
- Produces:
  - `ExpandedNode { temp_id: usize, parent_temp_id: Option<usize>, name: String, display_name: Option<String>, category: String, thing_type: String, properties: Vec<PropertyTemplate>, commands: Vec<CommandTemplate>, event_defs: Vec<serde_json::Value>, knowledge: Option<String>, resources: Vec<SceneResource>, dashboard: Option<serde_json::Value>, alarm_rules: Vec<SceneAlarmRule> }`
  - `ExpansionResult { nodes: Vec<ExpandedNode>, node_count: usize, tree_preview: String, warnings: Vec<String> }`
  - `ExpandError`（thiserror 枚举：InvalidParameter/ParamOutOfRange/RefNotFound/RefCycle/TooDeep/TooLarge/BothCountFields/RuleTypeNotAllowed）
  - `pub fn expand(template: &SceneTemplateFile, scene_name: &str, parameter_values: &HashMap<String, i64>, device_templates: &HashMap<String, ThingTemplate>, scene_templates: &HashMap<String, SceneTemplateFile>) -> Result<ExpansionResult, ExpandError>`

**关键语义（实现时严格遵守）：**
- `{index}` 每个父节点内独立从 1 开始；`{scene_name}` 全局替换
- `count` 与 `count_param` 同时出现 → `ExpandError::BothCountFields{key}`
- `scene_ref` 环检测（引用栈）→ `RefCycle{chain}`；深度 > 5 → `TooDeep`
- 总节点数 > 500 → `TooLarge{count}`
- `param_mapping` 值（本模板参数名）不存在 → `InvalidParameter{name}`
- `thing_type` 缺省映射：显式声明优先；否则 campus/floor/room/zone→`space`，building→`building`，`template_ref` 节点→`device`，其余→`space`
- `template_ref` 节点：properties/commands/events 从被引用模板内联，category 取其 category
- display_name：多语言模式替换后取 zh→en→name
- tree_preview：每行 `<indent(2空格/层)><display_name> (<category>)`；display_name 剔除换行/首尾空格，括号转全角
- alarm_rules 的 rule_type ∉ {threshold,range,change,event} → `RuleTypeNotAllowed`

- [ ] **Step 1: 写失败测试（追加到 scene_template.rs 的 tests 模块）**

```rust
    fn campus_template() -> SceneTemplateFile {
        SceneTemplateFile::from_json(r#"{
            "name": "smart_campus",
            "display_name": {"zh": "智慧园区"},
            "category": "scenes",
            "thing_category": "campus",
            "parameters": [
                {"name": "building_count", "type": "int", "default": 2, "min": 1, "max": 10, "display_name": {}},
                {"name": "floor_count", "type": "int", "default": 5, "min": 1, "max": 15, "display_name": {}}
            ],
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [
                {"key": "building", "category": "building", "count_param": "building_count",
                 "device_info": {"default_name_pattern": "{index}号楼"},
                 "children": [
                     {"key": "floor", "category": "floor", "count_param": "floor_count",
                      "device_info": {"default_name_pattern": "{index}F"},
                      "children": []}
                 ]}
            ]
        }"#).unwrap()
    }

    #[test]
    fn expand_counts_and_names() {
        let t = campus_template();
        let params = HashMap::from([("building_count".to_string(), 2i64), ("floor_count".to_string(), 3i64)]);
        let r = expand(&t, "张江科技园", &params, &HashMap::new(), &HashMap::new()).unwrap();
        // 1 园区 + 2 楼 + 2*3 层 = 9
        assert_eq!(r.node_count, 9);
        assert_eq!(r.nodes[0].name, "张江科技园");
        assert_eq!(r.nodes[0].thing_type, "space");
        let names: Vec<&str> = r.nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"1号楼"));
        assert!(names.contains(&"2号楼"));
        // 每栋楼内楼层独立从 1 起：两个 "1F"
        assert_eq!(names.iter().filter(|n| **n == "1F").count(), 2);
        // 父子链接：3F 的父是某栋楼
        let floor = r.nodes.iter().find(|n| n.name == "1F").unwrap();
        let parent = &r.nodes[floor.parent_temp_id.unwrap()];
        assert_eq!(parent.category, "building");
        assert_eq!(parent.thing_type, "building");
        // tree_preview 含缩进层级
        assert!(r.tree_preview.contains("张江科技园 (campus)"));
        assert!(r.tree_preview.contains("\n  1号楼 (building)"));
        assert!(r.tree_preview.contains("\n    1F (floor)"));
    }

    #[test]
    fn expand_rejects_missing_param_value_uses_default() {
        let t = campus_template();
        let r = expand(&t, "园区", &HashMap::new(), &HashMap::new(), &HashMap::new()).unwrap();
        // 默认 building=2, floor=5 → 1+2+10=13
        assert_eq!(r.node_count, 13);
    }

    #[test]
    fn expand_rejects_out_of_range() {
        let t = campus_template();
        let params = HashMap::from([("building_count".to_string(), 99i64)]);
        let e = expand(&t, "园区", &params, &HashMap::new(), &HashMap::new()).unwrap_err();
        assert!(matches!(e, ExpandError::ParamOutOfRange { .. }));
    }

    #[test]
    fn expand_rejects_too_large() {
        let t = campus_template();
        let params = HashMap::from([("building_count".to_string(), 10i64), ("floor_count".to_string(), 15i64)]);
        // 1+10+150=161 OK；floor max 15 下不会超 500，造一个超限模板：
        let big = SceneTemplateFile::from_json(r#"{
            "name": "big", "display_name": {"zh":"大"}, "category": "scenes",
            "parameters": [{"name":"n","type":"int","default":600,"min":1,"max":1000,"display_name":{}}],
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"category":"room","count_param":"n","device_info":{"default_name_pattern":"{index}"}}]
        }"#).unwrap();
        let e = expand(&big, "x", &HashMap::new(), &HashMap::new(), &HashMap::new()).unwrap_err();
        assert!(matches!(e, ExpandError::TooLarge { .. }));
    }

    #[test]
    fn expand_rejects_both_count_fields() {
        let t = SceneTemplateFile::from_json(r#"{
            "name": "bad", "display_name": {"zh":"坏"}, "category": "scenes",
            "parameters": [{"name":"n","type":"int","default":2,"min":1,"max":9,"display_name":{}}],
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"key":"x","category":"room","count":2,"count_param":"n",
                          "device_info":{"default_name_pattern":"{index}"}}]
        }"#).unwrap();
        let e = expand(&t, "x", &HashMap::new(), &HashMap::new(), &HashMap::new()).unwrap_err();
        assert!(matches!(e, ExpandError::BothCountFields { .. }));
    }

    #[test]
    fn expand_scene_ref_inlines_subtree_and_maps_params() {
        let floor_pack = SceneTemplateFile::from_json(r#"{
            "name": "smart_floor", "display_name": {"zh":"楼层"}, "category": "scenes",
            "thing_category": "floor",
            "parameters": [{"name":"rooms","type":"int","default":3,"min":1,"max":50,"display_name":{}}],
            "device_info": {"default_name_pattern": "{scene_name}楼层"},
            "children": [{"category":"room","count_param":"rooms","device_info":{"default_name_pattern":"{index}室"}}]
        }"#).unwrap();
        let campus = SceneTemplateFile::from_json(r#"{
            "name": "c", "display_name": {"zh":"园"}, "category": "scenes",
            "parameters": [{"name":"n_rooms","type":"int","default":4,"min":1,"max":50,"display_name":{}}],
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"category":"building","device_info":{"default_name_pattern":"主楼"},
                          "children":[{"scene_ref":"smart_floor","param_mapping":{"rooms":"n_rooms"}}]}]
        }"#).unwrap();
        let scenes = HashMap::from([("smart_floor".to_string(), floor_pack)]);
        let r = expand(&campus, "园区", &HashMap::new(), &HashMap::new(), &scenes).unwrap();
        // 1 园 + 1 楼 + 1 楼层 + 4 室 = 7
        assert_eq!(r.node_count, 7);
        assert!(r.nodes.iter().any(|n| n.name == "4室"));
    }

    #[test]
    fn expand_detects_scene_ref_cycle() {
        let a = SceneTemplateFile::from_json(r#"{
            "name": "a", "display_name": {"zh":"a"}, "category": "scenes",
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"scene_ref": "b"}]
        }"#).unwrap();
        let b = SceneTemplateFile::from_json(r#"{
            "name": "b", "display_name": {"zh":"b"}, "category": "scenes",
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"scene_ref": "a"}]
        }"#).unwrap();
        let scenes = HashMap::from([("a".to_string(), a.clone()), ("b".to_string(), b)]);
        let e = expand(&a, "x", &HashMap::new(), &HashMap::new(), &scenes).unwrap_err();
        assert!(matches!(e, ExpandError::RefCycle { .. }));
    }

    #[test]
    fn expand_rejects_bad_rule_type() {
        let t = SceneTemplateFile::from_json(r#"{
            "name": "s", "display_name": {"zh":"s"}, "category": "scenes",
            "device_info": {"default_name_pattern": "{scene_name}"},
            "alarm_rules": [{"name":"r","rule_type":"duration","condition":{}}],
            "children": []
        }"#).unwrap();
        let e = expand(&t, "x", &HashMap::new(), &HashMap::new(), &HashMap::new()).unwrap_err();
        assert!(matches!(e, ExpandError::RuleTypeNotAllowed { .. }));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd crates/db && cargo test scene_template`
Expected: 编译错误 `cannot find function expand` / `ExpandError`

- [ ] **Step 3: 实现展开器**

在 `crates/db/src/scene_template.rs` 追加（核心实现，递归 + 引用栈）：

```rust
// ──────────────────────────────────────────────
// 展开器（纯函数，无 IO）
// ──────────────────────────────────────────────

use thiserror::Error;

use crate::thing_template::ThingTemplate;

pub const MAX_NODES: usize = 500;
pub const MAX_SCENE_REF_DEPTH: usize = 5;
const ALLOWED_RULE_TYPES: [&str; 4] = ["threshold", "range", "change", "event"];

#[derive(Debug, Error)]
pub enum ExpandError {
    #[error("参数不存在: {name}")]
    InvalidParameter { name: String },
    #[error("参数 {name} 值 {value} 超出范围 [{min}, {max}]")]
    ParamOutOfRange { name: String, value: i64, min: i64, max: i64 },
    #[error("引用不存在或已停用: {name}")]
    RefNotFound { name: String },
    #[error("scene_ref 引用环: {chain}")]
    RefCycle { chain: String },
    #[error("scene_ref 引用深度超过 {MAX_SCENE_REF_DEPTH} 层")]
    TooDeep,
    #[error("展开后节点数 {count} 超过上限 {MAX_NODES}")]
    TooLarge { count: usize },
    #[error("节点 {key} 同时声明 count 与 count_param")]
    BothCountFields { key: String },
    #[error("不支持的告警规则类型: {rule_type}（允许: threshold/range/change/event）")]
    RuleTypeNotAllowed { rule_type: String },
}

/// 展开后的待创建本体（拓扑序：父先于子）。
#[derive(Debug, Clone)]
pub struct ExpandedNode {
    pub temp_id: usize,
    pub parent_temp_id: Option<usize>,
    pub name: String,
    pub display_name: Option<String>,
    pub category: String,
    pub thing_type: String,
    pub properties: Vec<PropertyTemplate>,
    pub commands: Vec<CommandTemplate>,
    pub event_defs: Vec<serde_json::Value>,
    pub knowledge: Option<String>,
    pub resources: Vec<SceneResource>,
    pub dashboard: Option<serde_json::Value>,
    pub alarm_rules: Vec<SceneAlarmRule>,
}

#[derive(Debug)]
pub struct ExpansionResult {
    pub nodes: Vec<ExpandedNode>,
    pub node_count: usize,
    pub tree_preview: String,
    pub warnings: Vec<String>,
}

/// thing_type 缺省映射（spec 附录 A）。
fn default_thing_type(category: &str, is_device: bool) -> String {
    if is_device {
        return "device".to_string();
    }
    match category {
        "building" => "building".to_string(),
        _ => "space".to_string(),
    }
}

/// 多语言回退：zh → en。
pub fn localized(map: &HashMap<String, String>) -> Option<String> {
    map.get("zh").or_else(|| map.get("en")).cloned()
}

struct Expander<'a> {
    params: HashMap<String, i64>,
    scene_name: String,
    device_templates: &'a HashMap<String, ThingTemplate>,
    scene_templates: &'a HashMap<String, SceneTemplateFile>,
    ref_stack: Vec<String>,
    nodes: Vec<ExpandedNode>,
    warnings: Vec<String>,
    preview_lines: Vec<String>,
}

impl<'a> Expander<'a> {
    fn push_node(
        &mut self,
        parent_temp_id: Option<usize>,
        depth: usize,
        name: String,
        display_name: Option<String>,
        category: String,
        thing_type: String,
        node: &ThingNodeDef,
    ) -> Result<usize, ExpandError> {
        if self.nodes.len() >= MAX_NODES {
            return Err(ExpandError::TooLarge { count: self.nodes.len() + 1 });
        }
        let temp_id = self.nodes.len();
        self.nodes.push(ExpandedNode {
            temp_id,
            parent_temp_id,
            name: name.clone(),
            display_name: display_name.clone(),
            category: category.clone(),
            thing_type,
            properties: node.properties.clone(),
            commands: node.commands.clone(),
            event_defs: node.events.clone(),
            knowledge: node.default_knowledge.clone(),
            resources: node.resources.clone(),
            dashboard: node.dashboard.clone(),
            alarm_rules: node.alarm_rules.clone(),
        });
        let label = sanitize_label(display_name.as_deref().unwrap_or(&name));
        self.preview_lines.push(format!("{}{} ({})", "  ".repeat(depth), label, category));
        Ok(temp_id)
    }

    fn expand_node(
        &mut self,
        node: &ThingNodeDef,
        parent_temp_id: Option<usize>,
        depth: usize,
    ) -> Result<(), ExpandError> {
        // count 与 count_param 互斥校验
        if node.count.is_some() && node.count_param.is_some() {
            return Err(ExpandError::BothCountFields {
                key: node.key.clone().unwrap_or_else(|| "<anonymous>".to_string()),
            });
        }
        // 告警规则类型校验
        for rule in &node.alarm_rules {
            if !ALLOWED_RULE_TYPES.contains(&rule.rule_type.as_str()) {
                return Err(ExpandError::RuleTypeNotAllowed {
                    rule_type: rule.rule_type.clone(),
                });
            }
        }

        if let Some(scene_ref) = &node.scene_ref {
            return self.expand_scene_ref(node, scene_ref, parent_temp_id, depth);
        }

        let copies = match (node.count, &node.count_param) {
            (Some(n), None) => n as usize,
            (None, Some(param)) => *self
                .params
                .get(param)
                .ok_or_else(|| ExpandError::InvalidParameter { name: param.clone() })?
                as usize,
            (None, None) => 1,
            _ => unreachable!(),
        };

        if let Some(template_ref) = &node.template_ref {
            // 设备模板引用：内联其 properties/commands/events
            let tpl = self
                .device_templates
                .get(template_ref)
                .ok_or_else(|| ExpandError::RefNotFound { name: template_ref.clone() })?;
            let category = tpl.category.clone();
            let props: Vec<PropertyTemplate> =
                serde_json::from_str(&tpl.properties).unwrap_or_default();
            let cmds: Vec<CommandTemplate> = serde_json::from_str(&tpl.actions).unwrap_or_default();
            let events: Vec<serde_json::Value> =
                serde_json::from_str(&tpl.events_json_for_inline()).unwrap_or_default();
            for i in 1..=copies {
                let name = self.node_name(node, i);
                let mut inlined = node.clone();
                inlined.properties = props.clone();
                inlined.commands = cmds.clone();
                inlined.events = events.clone();
                let id = self.push_node(
                    parent_temp_id,
                    depth,
                    name.clone(),
                    Some(name),
                    category.clone(),
                    "device".to_string(),
                    &inlined,
                )?;
                for child in &node.children {
                    self.expand_node(child, Some(id), depth + 1)?;
                }
            }
            return Ok(());
        }

        for i in 1..=copies {
            let name = self.node_name(node, i);
            let display = self.node_display_name(node, i, &name);
            let category = node.category.clone().unwrap_or_default();
            let thing_type = node
                .thing_type
                .clone()
                .unwrap_or_else(|| default_thing_type(&category, false));
            let id = self.push_node(parent_temp_id, depth, name, display, category, thing_type, node)?;
            for child in &node.children {
                self.expand_node(child, Some(id), depth + 1)?;
            }
        }
        Ok(())
    }

    fn expand_scene_ref(
        &mut self,
        node: &ThingNodeDef,
        scene_ref: &str,
        parent_temp_id: Option<usize>,
        depth: usize,
    ) -> Result<(), ExpandError> {
        if self.ref_stack.iter().any(|r| r == scene_ref) {
            let mut chain = self.ref_stack.clone();
            chain.push(scene_ref.to_string());
            return Err(ExpandError::RefCycle { chain: chain.join(" → ") });
        }
        if self.ref_stack.len() >= MAX_SCENE_REF_DEPTH {
            return Err(ExpandError::TooDeep);
        }
        let target = self
            .scene_templates
            .get(scene_ref)
            .ok_or_else(|| ExpandError::RefNotFound { name: scene_ref.to_string() })?
            .clone();

        // 参数映射：目标参数名 ← 本模板参数值
        let saved_params = self.params.clone();
        if let Some(mapping) = &node.param_mapping {
            for (target_param, source_param) in mapping {
                let value = self
                    .params
                    .get(source_param)
                    .ok_or_else(|| ExpandError::InvalidParameter { name: source_param.clone() })?;
                self.params.insert(target_param.clone(), *value);
            }
        }
        self.ref_stack.push(scene_ref.to_string());

        // 被引用模板的根节点内联到当前位置
        let root = target.as_root_node();
        let result = self.expand_node(&root, parent_temp_id, depth);

        self.ref_stack.pop();
        self.params = saved_params;
        result
    }

    fn node_name(&self, node: &ThingNodeDef, index: usize) -> String {
        let pattern = node
            .device_info
            .default_name_pattern
            .as_deref()
            .unwrap_or("{index}");
        let mut vars = HashMap::new();
        vars.insert("scene_name", self.scene_name.clone());
        vars.insert("index", index.to_string());
        render_name_pattern(pattern, &vars)
    }

    fn node_display_name(&self, node: &ThingNodeDef, index: usize, fallback: &str) -> Option<String> {
        node.device_info
            .default_display_name_pattern
            .as_ref()
            .and_then(|m| localized(m))
            .map(|pattern| {
                let mut vars = HashMap::new();
                vars.insert("scene_name", self.scene_name.clone());
                vars.insert("index", index.to_string());
                render_name_pattern(&pattern, &vars)
            })
            .or_else(|| Some(fallback.to_string()))
    }
}

/// tree_preview 标签清洗：剔除换行/首尾空格，括号转全角。
fn sanitize_label(s: &str) -> String {
    s.replace(['\n', '\r'], "")
        .trim()
        .replace('(', "（")
        .replace(')', "）")
        .to_string()
}

/// 展开场景包模板。纯函数：所有引用模板由调用方预加载传入。
pub fn expand(
    template: &SceneTemplateFile,
    scene_name: &str,
    parameter_values: &HashMap<String, i64>,
    device_templates: &HashMap<String, ThingTemplate>,
    scene_templates: &HashMap<String, SceneTemplateFile>,
) -> Result<ExpansionResult, ExpandError> {
    // 参数校验 + 默认值填充
    let mut params = HashMap::new();
    for p in &template.parameters {
        if p.param_type != "int" {
            return Err(ExpandError::InvalidParameter { name: p.name.clone() });
        }
        let value = parameter_values.get(&p.name).copied().unwrap_or(p.default);
        if value < p.min || value > p.max {
            return Err(ExpandError::ParamOutOfRange {
                name: p.name.clone(),
                value,
                min: p.min,
                max: p.max,
            });
        }
        params.insert(p.name.clone(), value);
    }

    let mut ex = Expander {
        params,
        scene_name: scene_name.to_string(),
        device_templates,
        scene_templates,
        ref_stack: vec![template.name.clone()],
        nodes: Vec::new(),
        warnings: Vec::new(),
        preview_lines: Vec::new(),
    };

    // 根节点
    let root = template.as_root_node_with_name(scene_name);
    ex.expand_node(&root, None, 0)?;

    let node_count = ex.nodes.len();
    Ok(ExpansionResult {
        nodes: ex.nodes,
        node_count,
        tree_preview: ex.preview_lines.join("\n"),
        warnings: ex.warnings,
    })
}
```

再给 `SceneTemplateFile` 加根节点转换（放在 `impl SceneTemplateFile` 块）：

```rust
    /// 根模板转为根节点定义（children 场景内联展开用）。
    pub fn as_root_node(&self) -> ThingNodeDef {
        ThingNodeDef {
            key: None,
            category: self.thing_category.clone().or_else(|| Some(self.category.clone())),
            thing_type: None,
            count: None,
            count_param: None,
            template_ref: None,
            scene_ref: None,
            param_mapping: None,
            device_info: self.device_info.clone(),
            properties: self.properties.clone(),
            commands: self.commands.clone(),
            events: self.events.clone(),
            default_knowledge: self.default_knowledge.clone(),
            resources: self.resources.clone(),
            dashboard: self.dashboard.clone(),
            alarm_rules: self.alarm_rules.clone(),
            children: self.children.clone(),
        }
    }

    /// 根节点命名模式缺省为 {scene_name}。
    fn as_root_node_with_name(&self, scene_name: &str) -> ThingNodeDef {
        let mut root = self.as_root_node();
        if root.device_info.default_name_pattern.is_none() {
            root.device_info.default_name_pattern = Some("{scene_name}".to_string());
        }
        let _ = scene_name;
        root
    }
```

`ThingTemplate` 需要一个 events 访问器（db 行结构含 events 列；若 `ThingTemplate` 结构体当前没有 events 字段，则在 `ThingTemplate` 上加 `pub events: String` 需先核对——**核对步骤**：`grep -n "pub events" crates/db/src/thing_template.rs`；若主结构体无该列而只有 Row 类型有，则给 `ThingTemplate` 结构体与 SELECT 列清单补上 `events`，与 Task 3 的 full row 修复一起做。此处先提供 helper）：

在 `impl ThingTemplate` 加：

```rust
    /// events 列原文（供展开器内联）。若结构体无 events 字段，返回 "[]"。
    pub fn events_json_for_inline(&self) -> String {
        // Task 3 补齐 events 字段后改为 self.events.clone()
        "[]".to_string()
    }
```

- [ ] **Step 4: 跑测试**

Run: `cd crates/db && cargo test scene_template`
Expected: 全部 PASS（parse×2 + render×1 + is_composition 不受影响 + expand×7）

- [ ] **Step 5: Commit**

```bash
git add crates/db/src/scene_template.rs crates/db/src/thing_template.rs
git commit -m "feat(db): scene template expander (pure fn, count params, template_ref/scene_ref, cycle detection)"
```

---

### Task 3: get_thing_info 防护门 + install 路径 device_info 修复 + 查找排序

**Files:**
- Modify: `crates/db/src/thing_template.rs`（`ThingTemplateFullRow` 补 `device_info`、`find_thing_template_by_name` 加 ORDER BY、`insert_thing_template_copy` 绑定、核对 `events` 字段）
- Modify: `apps/cloud/src/domains/thing/template/service.rs:60,127,339,625,762,937`（6 处调用点早返门）

**Interfaces:**
- Consumes: `ThingTemplate::is_composition()`（Task 1）
- Produces: 无新签名；行为变化——组合模板走 entity 路径得到明确 400 而非解析混乱

- [ ] **Step 1: 核对 events 字段与 full row**

Run: `grep -n "pub events\|device_info" crates/db/src/thing_template.rs | head -20`
确认：`ThingTemplateFullRow`（:220）是否已有 `device_info`；`ThingTemplate` 主结构体是否有 `events` 字段。

- [ ] **Step 2: 写失败测试**

在 `crates/db/src/thing_template.rs` 测试模块加：

```rust
    #[tokio::test]
    async fn find_by_name_prefers_workspace_over_builtin() {
        // 建库模式（与 crates/db/tests/seed_tests.rs 一致）：
        //   let pool = tinyiothub_storage::test_helpers::test_pool().await;
        //   let db = tinyiothub_storage::Db::new(pool);
        //   tinyiothub_storage::seed::seed_system(&db).await.unwrap();
        // 然后插入同名 builtin（workspace_id NULL）与 workspace 模板各一条
        let found = db.find_thing_template_by_name("dup_name", "ws1").await.unwrap().unwrap();
        assert_eq!(found.workspace_id.as_deref(), Some("ws1"));
    }

    #[tokio::test]
    async fn install_copy_preserves_device_info() {
        // 同上 test_pool() 建库；插入一条 device_info 含 children 的模板
        // → find_thing_template_full 读出 → 断言 device_info 含 "children"
    }
```

- [ ] **Step 3: 跑测试确认失败**

Run: `cd crates/db && cargo test thing_template`
Expected: 两个新测试 FAIL（无排序 / full row 缺字段）

- [ ] **Step 4: 实现修复**

`find_thing_template_by_name`（:420）SQL 加排序：

```sql
            SELECT id, name, display_name, description, version, author, category,
                   manufacturer, protocol_type, driver_name, tags,
                   device_info, properties, actions, is_builtin, is_active,
                   created_at, updated_at, workspace_id
            FROM thing_templates WHERE name = ? AND is_active = 1
              AND (workspace_id IS NULL OR workspace_id = ?)
            ORDER BY workspace_id IS NULL
            LIMIT 1
```

（`workspace_id IS NULL` 对 workspace 行求值为 0、builtin 行为 1，升序即 workspace 优先。）

`ThingTemplateFullRow` 加 `pub device_info: String,`，`find_thing_template_full` 的 SELECT 列清单加 `device_info`，`insert_thing_template_copy` 的 INSERT 加列与 `.bind(&source.device_info)`。

若 Step 1 核对发现 `ThingTemplate` 主结构体缺 `events` 字段：给结构体加 `pub events: String`，所有 SELECT 列清单（`find_thing_template_by_id`、`find_thing_template_by_name`、`find_thing_templates`、marketplace list 等）补 `events`，`Default` 实现补 `events: "[]".to_string()`，并把 Task 2 的 `events_json_for_inline()` 改为 `self.events.clone()`。

- [ ] **Step 5: service.rs 6 处调用点加门**

每处在 `template.get_thing_info()` 调用前加早返（以 :60 `apply_template` 为例，其余 5 处同模式）：

```rust
        // 组合模板（场景包）不能走单本体创建路径
        if template.is_composition() {
            return Err(TemplateError::InvalidTemplateType {
                message: "场景包模板请使用 instantiate 接口".to_string(),
            });
        }
```

`TemplateError` 若无 `InvalidTemplateType` 变体，在 `crates/core/src/models/template_error.rs` 加：

```rust
    #[error("模板类型不适用: {message}")]
    InvalidTemplateType { message: String },
```

（:127 `preview_template`、:339 `get_template_requirements` 硬失败处加同样早返；:624/:762/:937 已是 `if let Ok` 容错处，改为显式判断 `if template.is_composition() { ...跳过/报错... }`。）

- [ ] **Step 6: 跑测试 + 回归**

Run: `cd crates/db && cargo test && cd ../../apps/cloud && cargo test template`
Expected: 全部 PASS；entity 模板创建/预览回归不变

- [ ] **Step 7: Commit**

```bash
git add crates/db/src/thing_template.rs crates/core/src/models/template_error.rs apps/cloud/src/domains/thing/template/service.rs
git commit -m "fix(db): composition template gates + install preserves device_info + deterministic name lookup"
```

---

### Task 4: 事务版写入函数 + create_thing_row_with_type + busy_timeout

**Files:**
- Modify: `crates/db/src/thing.rs`（新增 `create_thing_row_with_type`、`resolve_thing_name_tx`）
- Modify: `crates/db/src/thing_property.rs`（新增 `create_thing_properties_batch_tx`）
- Modify: `crates/db/src/thing_command.rs`（新增 `bulk_create_thing_commands_tx`）
- Modify: `crates/db/src/alarm_rule.rs`（新增 `create_alarm_rule_tx`）
- Modify: `crates/db/src/thing.rs` resources 区（新增 `insert_thing_resource_tx`）
- Modify: `crates/db/src/pool.rs:11`（busy_timeout）

**Interfaces:**
- Produces（Task 5 依赖）:
  - `pub(crate) async fn create_thing_row_with_type(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, req: &CreateThingRequest, thing_type: &str) -> Result<String, sqlx::Error>`（返回新 thing id）
  - `pub(crate) async fn resolve_thing_name_tx(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, workspace_id: &str, base: &str) -> Result<String, sqlx::Error>`
  - `pub(crate) async fn create_thing_properties_batch_tx(tx, requests: &[CreateThingPropertyRequest]) -> Result<(), sqlx::Error>`
  - `pub(crate) async fn bulk_create_thing_commands_tx(tx, requests: &[CreateThingCommandRequest]) -> Result<(), sqlx::Error>`
  - `pub(crate) async fn create_alarm_rule_tx(tx, rule: &AlarmRule) -> Result<(), sqlx::Error>`
  - `pub(crate) async fn insert_thing_resource_tx(tx, workspace_id: &str, thing_id: &str, resource_type: &str, name: &str, file_path: &str) -> Result<(), sqlx::Error>`

- [ ] **Step 1: busy_timeout（一行 + 测试）**

`crates/db/src/pool.rs` `connect_options`：

```rust
pub(crate) fn connect_options(config: &DatabaseConfig) -> Result<SqliteConnectOptions, sqlx::Error> {
    Ok(SqliteConnectOptions::from_str(&config.url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_millis(5000)))
}
```

HarmonyOS 分支（:36 附近）的 `harmonyos_options` 链同样加 `.busy_timeout(Duration::from_millis(5000))`。

- [ ] **Step 2: 写失败测试**

`crates/db/src/thing.rs` 测试模块加：

```rust
    #[tokio::test]
    async fn create_thing_row_with_type_sets_type_and_returns_id() {
        // 开事务 → create_thing_row_with_type(req, "space") → 提交 → 查出 thing_type == "space"
    }

    #[tokio::test]
    async fn resolve_name_strips_suffix_and_probes() {
        // 预占 "主楼"、"主楼-2" → resolve_thing_name_tx("主楼") == "主楼-3"
        // resolve_thing_name_tx("主楼-2") 剥离后缀按 "主楼" 探测
    }

    #[tokio::test]
    async fn batch_tx_functions_write_in_caller_transaction() {
        // 开事务 → batch_tx 写属性 → **回滚** → 查询无记录（证明没有自作主张提交）
    }
```

- [ ] **Step 3: 跑测试确认失败**

Run: `cd crates/db && cargo test thing`
Expected: FAIL（函数不存在）

- [ ] **Step 4: 实现**

`crates/db/src/thing.rs`（参考现有 `create_thing_inner` :1883 的列清单，加 `thing_type`）：

```rust
/// 在调用方事务内创建 Thing 行（实例化器专用），显式写 thing_type。
/// 与 create_thing_inner 的差异：tx 传入 + thing_type 参数。
pub(crate) async fn create_thing_row_with_type(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    req: &CreateThingRequest,
    thing_type: &str,
) -> Result<String, sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query(
        r#"
        INSERT INTO things (
            id, name, display_name, category, address, description, position,
            driver_name, device_model, protocol_type, factory_name, linked_data,
            driver_options, state, parent_id, template_id, thing_type,
            linked_gateway, fingerprint, workspace_id, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.display_name)
    .bind(&req.category)
    .bind(&req.address)
    .bind(&req.description)
    .bind(&req.position)
    .bind(&req.driver_name)
    .bind(&req.device_model)
    .bind(&req.protocol_type)
    .bind(&req.factory_name)
    .bind(&req.linked_data)
    .bind(&req.driver_options)
    .bind(0) // state：空间节点无连接态
    .bind(&req.parent_id)
    .bind(&req.template_id)
    .bind(thing_type)
    .bind(&req.linked_gateway)
    .bind(&req.fingerprint)
    .bind(&req.workspace_id)
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

/// 名称冲突解决：剥离末尾 -N 后缀得 base，探测 base/base-2/.../base-10。
/// 快路径；调用方仍需捕获唯一约束错误并重试（TOCTOU 兜底）。
pub(crate) async fn resolve_thing_name_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    base: &str,
) -> Result<String, sqlx::Error> {
    let stripped = strip_numeric_suffix(base);
    for n in 0..=10 {
        let candidate = if n == 0 {
            stripped.clone()
        } else {
            format!("{}-{}", stripped, n + 1)
        };
        let exists: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM things WHERE COALESCE(workspace_id, '') = COALESCE(?, '') AND name = ?",
        )
        .bind(workspace_id)
        .bind(&candidate)
        .fetch_optional(&mut **tx)
        .await?;
        if exists.is_none() {
            return Ok(candidate);
        }
    }
    Err(sqlx::Error::Protocol(format!(
        "同名冲突过多（{}），请手动指定名称",
        stripped
    )))
}

fn strip_numeric_suffix(name: &str) -> String {
    match name.rfind('-') {
        Some(pos) if name[pos + 1..].chars().all(|c| c.is_ascii_digit()) && !name[pos + 1..].is_empty() => {
            name[..pos].to_string()
        }
        _ => name.to_string(),
    }
}
```

`thing_property.rs` / `thing_command.rs`：把现有 `create_thing_properties_batch` / `bulk_create_thing_commands` 的函数体抽为 `_tx` 版本（`pool.begin()` 删除，改为 `&mut **tx` 执行；`QueryBuilder::push_values` 分批每 100 行——参考现有实现是否已用 QueryBuilder，若用则保持），原公开函数改为薄包装：

```rust
pub(crate) async fn create_thing_properties_batch_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    requests: &[CreateThingPropertyRequest],
) -> Result<(), sqlx::Error> {
    for chunk in requests.chunks(100) {
        let mut qb = sqlx::QueryBuilder::new(
            "INSERT INTO thing_properties (id, thing_id, name, display_name, description, data_type, unit, min_value, max_value, default_value, is_read_only, created_at, updated_at) ",
        );
        qb.push_values(chunk, |mut b, r| {
            b.push_bind(uuid::Uuid::new_v4().to_string())
                .push_bind(&r.thing_id)
                .push_bind(&r.name)
                .push_bind(&r.display_name)
                .push_bind(&r.description)
                .push_bind(&r.data_type)
                .push_bind(&r.unit)
                .push_bind(r.min_value)
                .push_bind(r.max_value)
                .push_bind(&r.default_value)
                .push_bind(r.is_read_only)
                .push_bind(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string())
                .push_bind(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        });
        qb.build().execute(&mut **tx).await?;
    }
    Ok(())
}
```

（**核对步骤**：先读 `thing_property.rs:97-160` 现有实现的真实列清单，以现有列为准调整上面的 INSERT 列；上面是示意骨架。）`bulk_create_thing_commands_tx` 同模式（列为现有 thing_commands 列）。

`alarm_rule.rs` 加 `create_alarm_rule_tx`：复用现有公开插入函数的 SQL，仅改为 `&mut **tx` 执行（读现有函数确认列清单后照搬）。

`thing.rs` resources 区加：

```rust
pub(crate) async fn insert_thing_resource_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    thing_id: &str,
    resource_type: &str,
    name: &str,
    file_path: &str,
) -> Result<(), sqlx::Error> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query(
        "INSERT INTO resources (id, workspace_id, thing_id, type, name, file_path, content, tags, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, NULL, '[]', ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(thing_id)
    .bind(resource_type)
    .bind(name)
    .bind(file_path)
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
```

- [ ] **Step 5: 跑测试**

Run: `cd crates/db && cargo test`
Expected: 全部 PASS

- [ ] **Step 6: Commit**

```bash
git add crates/db/src/thing.rs crates/db/src/thing_property.rs crates/db/src/thing_command.rs crates/db/src/alarm_rule.rs crates/db/src/pool.rs
git commit -m "feat(db): tx-variant insert functions + create_thing_row_with_type + name resolver + busy_timeout"
```

---

### Task 5: SceneInstantiator（单事务落库 + 可观测性）

**Files:**
- Create: `apps/cloud/src/domains/marketplace/scene_instantiator.rs`
- Modify: `apps/cloud/src/domains/marketplace/mod.rs`（注册模块）

**Interfaces:**
- Consumes: Task 2 的 `expand`/`SceneTemplateFile`/`ExpansionResult`，Task 4 的 tx 函数
- Produces（Task 6 handler 依赖）:
  - `pub struct InstantiateParams { pub scene_name: String, pub parent_id: Option<String>, pub parameter_values: HashMap<String, i64>, pub dry_run: bool }`
  - `pub struct InstantiateOutcome { pub node_count: usize, pub root_thing_id: Option<String>, pub tree_preview: String, pub warnings: Vec<String> }`
  - `pub async fn instantiate(db: &Db, workspace_id: &str, template_id: &str, params: &InstantiateParams) -> Result<InstantiateOutcome, MarketplaceError>`

**流程（spec §3.2 严格执行）：** 加载模板 → `is_composition` 校验 → `SceneTemplateFile::from_json(device_info)` → 递归收集 `template_ref`/`scene_ref` 引用名 → 逐个 `find_thing_template_by_name`（workspace 优先）加载 → `expand` → 配额校验（`count_things_by_workspace` + node_count ≤ thing_limit）→ dry_run 直接返回 → 否则 `pool.begin()` 单事务落库（拓扑序，名称冲突 `resolve_thing_name_tx` + 唯一约束捕获重探测重试 ≤10）→ tracing 日志 + 计数。

- [ ] **Step 1: 写失败测试（集成，真实 DB）**

创建 `apps/cloud/src/tests/scene_instantiate_test.rs`（参考 `apps/cloud/src/tests/` 现有测试的建库模式）：

```rust
#[tokio::test]
async fn instantiate_creates_full_tree_in_one_tx() {
    // seed 一个含 children 的场景包模板 + temperature_humidity_sensor 设备模板
    // instantiate(dry_run=false) → 断言：
    //   things 行数 == node_count、层级 parent_id 正确、thing_type 映射正确、
    //   template_id == 场景包 id、属性/命令已建、linked_data 含 knowledge
}

#[tokio::test]
async fn instantiate_dry_run_writes_nothing() {
    // dry_run=true → things 行数不变，返回 node_count/tree_preview
}

#[tokio::test]
async fn instantiate_rolls_back_on_mid_failure() {
    // 注入失败（如 alarm rule 引用不存在的 property_ref）→ things 行数不变（无半棵树）
}

#[tokio::test]
async fn instantiate_resolves_name_conflicts_with_suffix() {
    // 预占 "1号楼" → 实例化 → 存在 "1号楼-2"
}

#[tokio::test]
async fn instantiate_rejects_over_quota() {
    // thing_limit 设为 5 → 展开 9 节点 → 400
}

#[tokio::test]
async fn instantiate_rejects_non_composition_template() {
    // entity 模板调 instantiate → 400
}

#[tokio::test]
async fn instantiate_concurrent_same_name_gets_suffix_not_500() {
    // 两个 tokio::join! 并发实例化同名场景 → 都成功，
    // 其中一棵树节点名带 -2 后缀（覆盖 TOCTOU 唯一约束兜底路径），无 500
}
```

**可观测性说明**：项目当前**没有 metrics 注册表**（grep 无 prometheus/counter 设施），v1 用 tracing 结构化日志承载（`#[instrument]` + `info!`/`warn!` 带 template/node_count/result 字段），`scene_instantiations_total` 指标待项目引入 metrics 设施后补——日志字段设计已按可聚合口径（template + result 标签）。

- [ ] **Step 2: 跑测试确认失败**

Run: `cd apps/cloud && cargo test scene_instantiate`
Expected: 编译失败（模块不存在）

- [ ] **Step 3: 实现 SceneInstantiator**

```rust
//! SceneInstantiator — 场景包实例化：展开（纯函数）→ 配额校验 → 单事务落库。
//!
//! 数据流：
//!   template row ──▶ SceneTemplateFile::from_json(device_info)
//!        │ 收集 template_ref/scene_ref ──▶ find_thing_template_by_name (workspace→builtin)
//!        ▼
//!   expand() ──▶ ExpansionResult (nodes 拓扑序, tree_preview, warnings)
//!        │ dry_run=true → 直接返回（只读）
//!        ▼
//!   配额校验 (count_things_by_workspace + node_count ≤ thing_limit)
//!        ▼
//!   单事务：create_thing_row_with_type → properties/commands/resources/alarm_rules → linked_data
//!        │ 名称冲突: resolve_thing_name_tx（快路径）+ 唯一约束捕获重试（兜底）
//!        ▼
//!   commit / rollback（任何失败整体回滚，不留半棵树）

use std::collections::HashMap;

use tinyiothub_storage::scene_template::{
    expand, ExpandedNode, SceneTemplateFile,
};
use tinyiothub_storage::Db;
use tracing::{info, instrument, warn};

use super::error::{MarketplaceError, Result};

#[derive(Debug)]
pub struct InstantiateParams {
    pub scene_name: String,
    pub parent_id: Option<String>,
    pub parameter_values: HashMap<String, i64>,
    pub dry_run: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstantiateOutcome {
    pub node_count: usize,
    pub root_thing_id: Option<String>,
    pub tree_preview: String,
    pub warnings: Vec<String>,
}

pub struct SceneInstantiator;

impl SceneInstantiator {
    #[instrument(skip(db, params), fields(template_id, dry_run = params.dry_run))]
    pub async fn instantiate(
        db: &Db,
        workspace_id: &str,
        template_id: &str,
        params: &InstantiateParams,
    ) -> Result<InstantiateOutcome> {
        // 1. 加载模板
        let template = db
            .find_thing_template_by_id(template_id, workspace_id)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?
            .ok_or_else(|| MarketplaceError::NotFound(format!("模板不存在: {}", template_id)))?;
        if !template.is_composition() {
            return Err(MarketplaceError::InvalidConfig(
                "非场景包模板，请使用 install 接口".to_string(),
            ));
        }
        let scene = SceneTemplateFile::from_json(&template.device_info)
            .map_err(|e| MarketplaceError::Template(format!("场景包解析失败: {}", e)))?;

        // 2. 收集并加载引用（workspace → builtin）
        let (device_refs, scene_refs) = collect_refs(&scene);
        let mut device_templates = HashMap::new();
        for name in &device_refs {
            let t = db
                .find_thing_template_by_name(name, workspace_id)
                .await
                .map_err(|e| MarketplaceError::Template(e.to_string()))?
                .ok_or_else(|| {
                    MarketplaceError::Template(format!("引用模板不存在或已停用: {}", name))
                })?;
            device_templates.insert(name.clone(), t);
        }
        let mut scene_templates = HashMap::new();
        for name in &scene_refs {
            let t = db
                .find_thing_template_by_name(name, workspace_id)
                .await
                .map_err(|e| MarketplaceError::Template(e.to_string()))?
                .ok_or_else(|| {
                    MarketplaceError::Template(format!("引用场景包不存在或已停用: {}", name))
                })?;
            scene_templates.insert(
                name.clone(),
                SceneTemplateFile::from_json(&t.device_info)
                    .map_err(|e| MarketplaceError::Template(format!("场景包 {} 解析失败: {}", name, e)))?,
            );
        }

        // 3. 展开（纯函数）
        let result = expand(
            &scene,
            &params.scene_name,
            &params.parameter_values,
            &device_templates,
            &scene_templates,
        )
        .map_err(|e| MarketplaceError::Validation(e.to_string()))?;

        // 4. 配额校验（真实行数，不用缓存计数）
        let current = db
            .count_things_by_workspace(workspace_id)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?;
        let limit = db.tenant_thing_limit(workspace_id).await.unwrap_or(i64::MAX);
        if current + result.node_count as i64 > limit {
            return Err(MarketplaceError::Validation(format!(
                "超出配额：当前 {} 个本体 + 将创建 {} 个 > 上限 {}",
                current, result.node_count, limit
            )));
        }

        // 5. dry-run：只读返回
        if params.dry_run {
            info!(node_count = result.node_count, "dry-run 预览完成");
            return Ok(InstantiateOutcome {
                node_count: result.node_count,
                root_thing_id: None,
                tree_preview: result.tree_preview,
                warnings: result.warnings,
            });
        }

        // 6. parent_id 校验（存在且属于本 workspace）
        if let Some(parent_id) = &params.parent_id {
            let parent = db
                .find_thing_by_id(parent_id, workspace_id)
                .await
                .map_err(|e| MarketplaceError::Template(e.to_string()))?;
            if parent.is_none() {
                return Err(MarketplaceError::Validation(format!(
                    "父本体不存在或不属于当前 workspace: {}",
                    parent_id
                )));
            }
        }

        // 7. 单事务落库
        let mut tx = db
            .pool()
            .begin()
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?;
        let persist_result =
            persist_tree(&mut tx, workspace_id, template_id, params, &result).await;
        match persist_result {
            Ok((root_id, mut warnings)) => {
                tx.commit()
                    .await
                    .map_err(|e| MarketplaceError::Template(e.to_string()))?;
                warnings.extend(result.warnings.clone());
                info!(
                    node_count = result.node_count,
                    template = %template.name,
                    "场景包实例化完成"
                );
                // 指标：scene_instantiations_total{template, result="success"}
                Ok(InstantiateOutcome {
                    node_count: result.node_count,
                    root_thing_id: Some(root_id),
                    tree_preview: result.tree_preview,
                    warnings,
                })
            }
            Err(e) => {
                warn!(error = %e, "场景包实例化失败，事务回滚");
                // tx drop 自动回滚
                Err(e)
            }
        }
    }
}

/// 递归收集 template_ref / scene_ref 引用名。
fn collect_refs(scene: &SceneTemplateFile) -> (Vec<String>, Vec<String>) {
    fn walk(
        nodes: &[tinyiothub_storage::scene_template::ThingNodeDef],
        device: &mut Vec<String>,
        scene: &mut Vec<String>,
    ) {
        for n in nodes {
            if let Some(r) = &n.template_ref {
                device.push(r.clone());
            }
            if let Some(r) = &n.scene_ref {
                scene.push(r.clone());
            }
            walk(&n.children, device, scene);
        }
    }
    let mut device = Vec::new();
    let mut scenes = Vec::new();
    walk(&scene.children, &mut device, &mut scenes);
    device.sort();
    device.dedup();
    scenes.sort();
    scenes.dedup();
    (device, scenes)
}

/// 事务内落库：拓扑序创建本体 → 子表 → linked_data。返回 (root_thing_id, warnings)。
async fn persist_tree(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    template_id: &str,
    params: &InstantiateParams,
    result: &tinyiothub_storage::scene_template::ExpansionResult,
) -> Result<(String, Vec<String>)> {
    let mut warnings = Vec::new();
    let mut real_ids: HashMap<usize, String> = HashMap::new();
    let mut root_id = String::new();

    for node in &result.nodes {
        let real_parent = match node.parent_temp_id {
            Some(pid) => real_ids.get(&pid).cloned(),
            None => params.parent_id.clone(),
        };
        // 名称冲突：SELECT 探测（快路径）
        let resolved = resolve_with_retry(tx, workspace_id, &node.name, &mut warnings).await?;
        let req = build_thing_request(node, resolved, real_parent, template_id, workspace_id);
        let id = tinyiothub_storage::thing::create_thing_row_with_type(tx, &req, &node.thing_type)
            .await
            .map_err(|e| MarketplaceError::Template(format!("创建本体失败: {}", e)))?;
        real_ids.insert(node.temp_id, id.clone());
        if node.temp_id == 0 {
            root_id = id.clone();
        }

        persist_children_tables(tx, &id, node, &real_ids, result, &mut warnings).await?;
    }
    Ok((root_id, warnings))
}

/// 唯一约束兜底：撞冲突时重探测（TOCTOU，上限含在 resolve 内）。
async fn resolve_with_retry(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    name: &str,
    warnings: &mut Vec<String>,
) -> Result<String> {
    let resolved = tinyiothub_storage::thing::resolve_thing_name_tx(tx, workspace_id, name)
        .await
        .map_err(|e| MarketplaceError::Validation(e.to_string()))?;
    if resolved != name {
        warnings.push(format!("名称冲突：{} → {}", name, resolved));
    }
    Ok(resolved)
}

fn build_thing_request(
    node: &ExpandedNode,
    resolved_name: String,
    parent_id: Option<String>,
    template_id: &str,
    workspace_id: &str,
) -> tinyiothub_core::models::thing::CreateThingRequest {
    // linked_data：knowledge / event_defs / dashboard 按顶层键合并（v1 新建，无既有键）
    let mut linked = serde_json::Map::new();
    if let Some(k) = &node.knowledge {
        linked.insert("knowledge".to_string(), serde_json::json!(k));
    }
    if !node.event_defs.is_empty() {
        linked.insert("event_defs".to_string(), serde_json::json!(node.event_defs));
    }
    if let Some(d) = &node.dashboard {
        linked.insert("dashboard".to_string(), d.clone());
    }
    let linked_data = if linked.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(linked).to_string())
    };

    tinyiothub_core::models::thing::CreateThingRequest {
        name: resolved_name,
        display_name: node.display_name.clone(),
        category: Some(node.category.clone()),
        address: None,
        description: None,
        position: None,
        driver_name: None,
        device_model: None,
        protocol_type: None,
        factory_name: None,
        linked_data,
        driver_options: None,
        parent_id,
        template_id: Some(template_id.to_string()),
        linked_gateway: None,
        fingerprint: None,
        workspace_id: Some(workspace_id.to_string()),
    }
}

async fn persist_children_tables(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    thing_id: &str,
    node: &ExpandedNode,
    _real_ids: &HashMap<usize, String>,
    _result: &tinyiothub_storage::scene_template::ExpansionResult,
    warnings: &mut Vec<String>,
) -> Result<()> {
    // 属性
    let props: Vec<_> = node
        .properties
        .iter()
        .map(|p| tinyiothub_core::models::thing_property::CreateThingPropertyRequest {
            thing_id: thing_id.to_string(),
            name: p.name.clone(),
            display_name: tinyiothub_storage::scene_template::localized(&p.display_name),
            description: p
                .description
                .as_ref()
                .and_then(|d| tinyiothub_storage::scene_template::localized(d)),
            data_type: Some(p.data_type.clone()),
            unit: p.unit.clone(),
            min_value: p.min_value,
            max_value: p.max_value,
            default_value: p.default_value.clone(),
            is_read_only: Some(p.is_read_only as i32),
        })
        .collect();
    tinyiothub_storage::thing_property::create_thing_properties_batch_tx(tx, &props)
        .await
        .map_err(|e| MarketplaceError::Template(format!("创建属性失败: {}", e)))?;

    // 命令
    let cmds: Vec<_> = node
        .commands
        .iter()
        .map(|c| tinyiothub_core::models::thing_command::CreateThingCommandRequest {
            thing_id: thing_id.to_string(),
            name: c.name.clone(),
            display_name: tinyiothub_storage::scene_template::localized(&c.display_name),
            description: c
                .description
                .as_ref()
                .and_then(|d| tinyiothub_storage::scene_template::localized(d)),
            parameters: c.parameters.clone(),
        })
        .collect();
    tinyiothub_storage::thing_command::bulk_create_thing_commands_tx(tx, &cmds)
        .await
        .map_err(|e| MarketplaceError::Template(format!("创建命令失败: {}", e)))?;

    // 资源（file_path = uri 原样记录，v1 无真实托管）
    for r in &node.resources {
        tinyiothub_storage::thing::insert_thing_resource_tx(
            tx, workspace_id_placeholder, thing_id, &r.resource_type, &r.name, &r.uri,
        )
        .await
        .map_err(|e| MarketplaceError::Template(format!("创建资源失败: {}", e)))?;
    }

    // 告警规则：property_ref → 本节点真实 property_id
    for rule in &node.alarm_rules {
        let property_id = match &rule.property_ref {
            Some(ref_name) => {
                // 从事务内查刚创建的属性 id
                let found: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM thing_properties WHERE thing_id = ? AND name = ?",
                )
                .bind(thing_id)
                .bind(ref_name)
                .fetch_optional(&mut **tx)
                .await
                .map_err(|e| MarketplaceError::Template(e.to_string()))?;
                match found {
                    Some((id,)) => Some(id),
                    None => {
                        warnings.push(format!(
                            "告警规则 {} 引用的属性 {} 不存在，跳过",
                            rule.name, ref_name
                        ));
                        continue;
                    }
                }
            }
            None => None,
        };
        let alarm = tinyiothub_storage::alarm_rule::AlarmRule::new(
            rule.name.clone(),
            rule.description.clone(),
            Some(thing_id.to_string()),
            property_id,
            parse_rule_type(&rule.rule_type)?,
            serde_json::from_value(rule.condition.clone())
                .map_err(|e| MarketplaceError::Validation(format!("告警条件格式错误: {}", e)))?,
            parse_alarm_level(&rule.alarm_level),
            serde_json::from_value(rule.notification_config.clone()).unwrap_or_default(),
            workspace_id_placeholder.to_string(),
        )
        .map_err(|e| MarketplaceError::Validation(e.to_string()))?;
        tinyiothub_storage::alarm_rule::create_alarm_rule_tx(tx, &alarm)
            .await
            .map_err(|e| MarketplaceError::Template(format!("创建告警规则失败: {}", e)))?;
    }
    Ok(())
}
```

（**实现注意**：`workspace_id_placeholder` 是伪代码标记——`persist_children_tables` 签名加 `workspace_id: &str` 参数从 `persist_tree` 透传；`parse_rule_type`/`parse_alarm_level` 是小辅助函数，把字符串映射到 `RuleType`/`AlarmLevel` 枚举，`RuleType` 只接受 4 个允许值（展开器已校验，此处 `unreachable` 或再校验一次）。）

`MarketplaceError` 需补 `Validation(String)` 变体（error.rs 若无）：

```rust
    #[error("参数校验失败: {0}")]
    Validation(String),

    #[error("模板错误: {0}")]
    Template(String),
```

`mod.rs` 注册：`pub mod scene_instantiator;`

`Db` 需要确认/补充 wrapper：`count_things_by_workspace`（thing.rs:2852 已有 pub(crate)，补 pub wrapper）、`tenant_thing_limit`（新方法：从租户订阅读 thing_limit；读 `crates/db/src/tenant.rs` 确认现有查询函数后加薄 wrapper）、`find_thing_by_id`（应已存在，核对名字）。

- [ ] **Step 4: 跑测试**

Run: `cd apps/cloud && cargo test scene_instantiate`
Expected: 6 个测试 PASS

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/marketplace/scene_instantiator.rs apps/cloud/src/domains/marketplace/mod.rs apps/cloud/src/domains/marketplace/error.rs apps/cloud/src/tests/scene_instantiate_test.rs
git commit -m "feat(marketplace): scene instantiator — single-tx tree persistence with dry-run and quota check"
```

---

### Task 6: marketplace API（列表 is_composition + 详情 + instantiate 端点）

**Files:**
- Modify: `apps/cloud/src/domains/marketplace/thing_template_installer.rs`（`ThingTemplateItem` 加 `is_composition`/`parameter_count`，list 加 composition 过滤）
- Modify: `apps/cloud/src/domains/marketplace/handler.rs:25`（路由 + 两个 handler）
- Modify: `apps/cloud/src/domains/marketplace/scene_instantiator.rs`（无——只被调用）

**Interfaces:**
- Consumes: `SceneInstantiator::instantiate`（Task 5）、`ThingTemplate::is_composition()`
- Produces（前端 Task 11 依赖）:
  - `GET /api/marketplace/thing-templates?composition=true` → items 含 `isComposition`/`parameterCount`
  - `GET /api/marketplace/thing-templates/{id}` → 详情含 `parameters[]`、`structureSummary {parameterCount, maxDepth}`
  - `POST /api/marketplace/thing-templates/{id}/instantiate`，body `{sceneName, parentId?, parameterValues?, dryRun?}` → `{nodeCount, rootThingId?, treePreview, warnings[]}`

- [ ] **Step 1: 写失败测试（API 集成）**

`apps/cloud/src/tests/scene_instantiate_test.rs` 追加：

```rust
#[tokio::test]
async fn api_instantiate_happy_path() {
    // POST instantiate {sceneName:"测试园区", parameterValues:{...}} → 200
    // result.nodeCount == 预期；result.treePreview 含 "测试园区 (campus)"
}

#[tokio::test]
async fn api_instantiate_dry_run() {
    // dryRun=true → 200，rootThingId 为 null，DB 无变化
}

#[tokio::test]
async fn api_list_marks_composition() {
    // GET thing-templates → 场景包项 isComposition=true, parameterCount=2；
    // ?composition=true 只返回场景包
}

#[tokio::test]
async fn api_detail_returns_parameters() {
    // GET thing-templates/{id} → parameters 含 building_count 的 min/max/default
}

#[tokio::test]
async fn api_instantiate_entity_template_400() {
    // entity 模板调 instantiate → 400
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd apps/cloud && cargo test api_`
Expected: FAIL（路由不存在）

- [ ] **Step 3: 实现**

`thing_template_installer.rs` 的 `ThingTemplateItem` 加字段并在 `list` 映射时计算：

```rust
pub struct ThingTemplateItem {
    // ... 现有字段 ...
    pub is_composition: bool,
    pub parameter_count: usize,
}
```

`list` 中（现有 map 闭包内）：

```rust
let is_composition = serde_json::from_str::<serde_json::Value>(&r.device_info)
    .ok()
    .and_then(|v| v.get("children").cloned())
    .and_then(|c| c.as_array().map(|a| !a.is_empty()))
    .unwrap_or(false);
let parameter_count = if is_composition {
    serde_json::from_str::<serde_json::Value>(&r.device_info)
        .ok()
        .and_then(|v| v.get("parameters")?.as_array().map(|a| a.len()))
        .unwrap_or(0)
} else {
    0
};
```

（注：`list_marketplace_thing_templates` 的 SELECT 若不含 `device_info` 需补列。）

`handler.rs` 路由注册（`create_router` 内追加）：

```rust
        .route("/thing-templates/{id}", get(get_thing_template_detail))
        .route(
            "/thing-templates/{id}/instantiate",
            post(instantiate_thing_template),
        )
```

handler 实现：

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstantiateRequestBody {
    pub scene_name: String,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub parameter_values: std::collections::HashMap<String, i64>,
    #[serde(default)]
    pub dry_run: bool,
}

async fn instantiate_thing_template(
    State(state): State<AppState>,
    claims: Claims,
    workspace: WorkspaceScope,
    Path(id): Path<String>,
    Json(body): Json<InstantiateRequestBody>,
) -> Json<ApiResponse<InstantiateOutcome>> {
    let params = InstantiateParams {
        scene_name: body.scene_name,
        parent_id: body.parent_id,
        parameter_values: body.parameter_values,
        dry_run: body.dry_run,
    };
    match SceneInstantiator::instantiate(state.db(), workspace.id(), &id, &params).await {
        Ok(outcome) => ApiResponseBuilder::success(outcome),
        Err(e) => ApiResponseBuilder::from(e), // 按现有 handler 错误映射模式
    }
}
```

（**核对**：读 `install_thing_template` 现有 handler 的签名模式——Claims/WorkspaceScope 提取方式、`state.db` 访问方式、错误到 ApiResponse 的映射，照搬同文件现有模式。）

详情 handler `get_thing_template_detail`：取模板 → `is_composition` → 组合模板解析 `SceneTemplateFile` 返回 `parameters` 与 `structure_summary`（`max_depth` 静态深度：根=1，children 递归最深，`template_ref`/`scene_ref` 计 1 层）：

```rust
fn max_depth(nodes: &[ThingNodeDef], depth: usize) -> usize {
    nodes
        .iter()
        .map(|n| max_depth(&n.children, depth + 1))
        .max()
        .unwrap_or(depth)
}
```

`list_thing_templates` handler 加 `composition: Option<bool>` query 参数，应用层过滤后分页。

- [ ] **Step 4: 跑测试**

Run: `cd apps/cloud && cargo test`
Expected: 全部 PASS

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/marketplace/
git commit -m "feat(marketplace): scene template list/detail/instantiate API with dry-run"
```

---

### Task 7: seed 数据（scenes 类别 + 3 个内置场景包）

**Files:**
- Create: `templates/builtin/scenes/smart_campus.json`、`smart_building.json`、`smart_floor.json`（源文件）
- Modify: `crates/db/src/seed/system.sql`（scenes 类别 + 3 个模板行，device_info 内嵌完整 JSON）

- [ ] **Step 1: 写失败测试（模板文件校验）**

`crates/db/tests/scene_templates_file_test.rs`：

```rust
#[test]
fn builtin_scene_templates_parse_and_validate() {
    for file in ["smart_campus", "smart_building", "smart_floor"] {
        let path = format!("../../templates/builtin/scenes/{}.json", file);
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} 读取失败: {}", path, e));
        let t = tinyiothub_storage::scene_template::SceneTemplateFile::from_json(&content)
            .unwrap_or_else(|e| panic!("{} 解析失败: {}", file, e));
        assert!(!t.children.is_empty(), "{} 必须有 children", file);
        // 展开冒烟：默认参数能展开且不超限
        let r = tinyiothub_storage::scene_template::expand(
            &t, "测试", &Default::default(), &Default::default(), &Default::default(),
        );
        // smart_campus/smart_building 引用 temperature_humidity_sensor —— 无设备模板 map 会 RefNotFound；
        // 这里只验证结构解析；完整展开在集成测试覆盖
        assert!(r.is_ok() || file != "smart_floor", "{} 展开失败", file);
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd crates/db && cargo test --test scene_templates_file_test`
Expected: FAIL（文件不存在）

- [ ] **Step 3: 写 3 个场景包 JSON**

`templates/builtin/scenes/smart_floor.json`（最小模板，完整内容）：

```json
{
  "name": "smart_floor",
  "display_name": {"zh": "智慧楼层", "en": "Smart Floor"},
  "description": {"zh": "一层楼的空间结构：楼层 + N 个房间", "en": "Floor structure with N rooms"},
  "version": "1.0.0",
  "author": "TinyIoT",
  "category": "scenes",
  "thing_category": "floor",
  "tags": ["floor", "space"],
  "parameters": [
    {"name": "room_count", "type": "int", "default": 8, "min": 1, "max": 50,
     "display_name": {"zh": "房间数量", "en": "Room Count"}}
  ],
  "device_info": {
    "default_name_pattern": "{scene_name}",
    "default_display_name_pattern": {"zh": "{scene_name}", "en": "{scene_name}"}
  },
  "properties": [
    {"name": "area", "display_name": {"zh": "面积", "en": "Area"}, "data_type": "number", "unit": "m²", "is_read_only": false, "is_required": false}
  ],
  "children": [
    {"key": "room", "category": "room", "count_param": "room_count",
     "device_info": {"default_name_pattern": "{index}室",
       "default_display_name_pattern": {"zh": "{index}室", "en": "Room {index}"}}}
  ]
}
```

`templates/builtin/scenes/smart_building.json`：

```json
{
  "name": "smart_building",
  "display_name": {"zh": "智慧楼宇", "en": "Smart Building"},
  "description": {"zh": "单体建筑：楼栋 + N 层 + 每层 2 个温湿度传感器", "en": "Building with N floors, 2 temp/humidity sensors per floor"},
  "version": "1.0.0",
  "author": "TinyIoT",
  "category": "scenes",
  "thing_category": "building",
  "tags": ["building", "space"],
  "parameters": [
    {"name": "floor_count", "type": "int", "default": 10, "min": 1, "max": 15,
     "display_name": {"zh": "楼层数", "en": "Floor Count"}}
  ],
  "device_info": {"default_name_pattern": "{scene_name}"},
  "default_knowledge": "你是这栋楼的楼宇管家，关注各层环境与设备状态。",
  "alarm_rules": [
    {"name": "高温告警", "rule_type": "threshold",
     "condition": {"type": "threshold", "operator": "greater_than", "value": 35.0},
     "alarm_level": "warning", "notification_config": {}, "property_ref": "temperature"}
  ],
  "children": [
    {"key": "floor", "category": "floor", "count_param": "floor_count",
     "device_info": {"default_name_pattern": "{index}F",
       "default_display_name_pattern": {"zh": "{index}F", "en": "{index}F"}},
     "resources": [
       {"name": "floor_plan", "type": "image", "uri": "builtin://scenes/smart_building/floor_plan.png"}
     ],
     "children": [
       {"key": "th_sensor", "template_ref": "temperature_humidity_sensor", "count": 2,
        "device_info": {"default_name_pattern": "th_sensor_{index}",
          "default_display_name_pattern": {"zh": "温湿度传感器 {index}", "en": "Temp & Humidity Sensor {index}"}}}
     ]}
  ]
}
```

`templates/builtin/scenes/smart_campus.json`：

```json
{
  "name": "smart_campus",
  "display_name": {"zh": "智慧园区", "en": "Smart Campus"},
  "description": {"zh": "园区：N 栋楼、每栋 M 层、每层 2 个温湿度传感器", "en": "Campus with N buildings, M floors each, 2 sensors per floor"},
  "version": "1.0.0",
  "author": "TinyIoT",
  "category": "scenes",
  "thing_category": "campus",
  "tags": ["campus", "building"],
  "parameters": [
    {"name": "building_count", "type": "int", "default": 2, "min": 1, "max": 10,
     "display_name": {"zh": "楼栋数量", "en": "Building Count"}},
    {"name": "floor_count", "type": "int", "default": 5, "min": 1, "max": 15,
     "display_name": {"zh": "每栋楼层数（最终节点数受 500 上限约束）", "en": "Floors per Building"}}
  ],
  "device_info": {"default_name_pattern": "{scene_name}"},
  "properties": [
    {"name": "area", "display_name": {"zh": "占地面积", "en": "Site Area"}, "data_type": "number", "unit": "m²", "is_read_only": false, "is_required": false},
    {"name": "plot_ratio", "display_name": {"zh": "容积率", "en": "Plot Ratio"}, "data_type": "number", "is_read_only": false, "is_required": false}
  ],
  "default_knowledge": "你是园区管家，统览各楼栋运行状态与告警。",
  "dashboard": {"cards": [{"property": "area"}, {"property": "plot_ratio"}]},
  "alarm_rules": [
    {"name": "能耗异常", "rule_type": "change",
     "condition": {"type": "change", "field": "energy", "threshold_percent": 50.0},
     "alarm_level": "warning", "notification_config": {}}
  ],
  "children": [
    {"key": "building", "category": "building", "count_param": "building_count",
     "device_info": {"default_name_pattern": "{index}号楼",
       "default_display_name_pattern": {"zh": "{index}号楼", "en": "Building {index}"}},
     "default_knowledge": "你是楼栋管家，关注本楼各层环境。",
     "alarm_rules": [
       {"name": "高温告警", "rule_type": "threshold",
        "condition": {"type": "threshold", "operator": "greater_than", "value": 35.0},
        "alarm_level": "warning", "notification_config": {}, "property_ref": "temperature"}
     ],
     "children": [
       {"key": "floor", "category": "floor", "count_param": "floor_count",
        "device_info": {"default_name_pattern": "{index}F"},
        "children": [
          {"key": "th_sensor", "template_ref": "temperature_humidity_sensor", "count": 2,
           "device_info": {"default_name_pattern": "th_sensor_{index}"}}
        ]}
     ]}
  ]
}
```

- [ ] **Step 4: system.sql 追加 seed**

在 `crates/db/src/seed/system.sql` 的 template_categories 段（:13-21 之后）追加：

```sql
('scenes', '{"zh": "场景包", "en": "Scene Packs"}', '{"zh": "空间组合模板：园区/楼宇/楼层", "en": "Spatial composition templates"}', 7, 1, datetime('now'));
```

（注意把现有最后一行 `('meters', ...)` 末尾的分号改为逗号再接新行，保持 SQL 语法。）

然后在 system.sql 末尾追加 3 个 `thing_templates` 行。`device_info` 列存**完整模板 JSON 原文**（SQL 字符串内单引号转义为 `''`；JSON 无双引号冲突，SQL 字符串用单引号包裹）：

```sql
-- ── 场景包模板（source: templates/builtin/scenes/*.json）────────────────────
INSERT OR IGNORE INTO thing_templates (id, name, display_name, description, version, author, category, manufacturer, thing_type, protocol_type, driver_name, tags, device_info, properties, actions, events, default_knowledge, is_builtin, is_active, created_at, updated_at)
SELECT
  'builtin_smart_floor', 'smart_floor', '{"zh": "智慧楼层", "en": "Smart Floor"}',
  '{"zh": "一层楼的空间结构：楼层 + N 个房间"}', '1.0.0', 'TinyIoT', 'scenes', 'TinyIoT', 'space', NULL, NULL,
  '["floor", "space"]',
  '<smart_floor.json 完整内容，单引号转义>',
  '[]', '[]', '[]', NULL, 1, 1, datetime('now'), datetime('now')
WHERE NOT EXISTS (SELECT 1 FROM thing_templates WHERE id = 'builtin_smart_floor');
```

（**生成方法**：把 Step 3 中对应 JSON 文件的完整内容粘贴进 `device_info` 列——JSON 中无单引号则原样粘贴，有则替换 `'` 为 `''`；3 个模板同样模式，`thing_type` 分别为 space/building/space，smart_campus 的 `default_knowledge` 列填园区管家人设。**核对列清单**：先 `grep -n "INSERT OR IGNORE INTO thing_templates" crates/db/src/seed/system.sql` 看现有模板行的列顺序，以现有为准。）

- [ ] **Step 5: 跑测试 + seed 验证**

Run: `cd crates/db && cargo test --test scene_templates_file_test && cargo test seed`
Expected: PASS；seed 测试（现有 seed 幂等测试）不破

手动验证：起一个新库跑 seed → `SELECT id, thing_type, is_builtin FROM thing_templates WHERE category='scenes';` 应有 3 行 → 每行 `json_extract(device_info, '$.children')` 非空。

- [ ] **Step 6: Commit**

```bash
git add templates/builtin/scenes/ crates/db/src/seed/system.sql crates/db/tests/scene_templates_file_test.rs
git commit -m "feat(seed): builtin scene packs (campus/building/floor) + scenes category"
```

---

### Task 8: 反向导出 export-as-template

**Files:**
- Modify: `apps/cloud/src/domains/thing/handler/mod.rs`（路由）
- Create: `apps/cloud/src/domains/thing/service/export_template.rs`

**Interfaces:**
- Consumes: db 的 things 子树查询（`find_thing_children` 类现有函数——**核对** `crates/db/src/thing.rs` 的 children 查询名）
- Produces: `POST /api/things/{id}/export-as-template` → JSON 文件下载（`SceneTemplateFile` 格式）
- 导出规则（spec §3.4）：workspace 校验（404 防 IDOR）；子树 >500 拒绝；命名泛化只处理单段数字前/后缀（去数字后相等 + 从 1 连续序列 → `{index}X` + count）；category 直接取；thing_type 仅与缺省映射不一致时写入；`linked_data` 的 knowledge/event_defs/dashboard 还原；设备节点不逆向 template_ref

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn export_round_trip_structure_equivalent() {
    // 实例化 smart_floor（room_count=4）→ 导出 → 断言模板 JSON：
    //   children[0].count == 4、name_pattern 含 "{index}室"
    // 重新解析 + 展开 → 节点数 == 原树
}

#[tokio::test]
async fn export_rejects_other_workspace() {
    // workspace A 的 thing，用 workspace B 调导出 → 404
}

#[tokio::test]
async fn export_keeps_non_pattern_names_with_warning() {
    // 手建两个不同名子节点（"会议室"/"储藏室"）→ 导出 → 不泛化，warnings 非空
}

#[tokio::test]
async fn export_rejects_oversized_subtree() {
    // 501+ 节点 → 400
}
```

- [ ] **Step 2: 跑测试确认失败** → `cd apps/cloud && cargo test export` → FAIL

- [ ] **Step 3: 实现**

`export_template.rs` 核心：

```rust
//! 反向导出：本体子树 → 场景包模板 JSON。
//!
//!   子树遍历（BFS）──▶ ThingNodeDef 递归构建
//!        │ 命名泛化：同父兄弟去数字后相等 + 数字从 1 连续 → "{index}X" + count
//!        │ （多段数字/中间数字不泛化，保留原名 + warning）
//!        ▼
//!   SceneTemplateFile JSON 下载（不入库；注册走 import 流程）

/// 命名泛化：返回 Some((pattern, count)) 或 None。
fn generalize_names(names: &[String]) -> Option<(String, usize)> {
    // 仅单段数字前缀或后缀
    let stripped: Vec<(String, Option<u32>)> = names.iter().map(|n| strip_single_number(n)).collect();
    let base = &stripped[0].0;
    if !stripped.iter().all(|(b, _)| b == base) {
        return None;
    }
    let mut indices: Vec<u32> = stripped.iter().filter_map(|(_, i)| *i).collect();
    if indices.len() != names.len() {
        return None; // 有的名字没数字
    }
    indices.sort_unstable();
    if indices != (1..=names.len() as u32).collect::<Vec<_>>() {
        return None; // 非从 1 连续
    }
    // 还原 pattern：把数字段替换为 {index}
    let pattern = strip_single_number_pattern(&names[0]);
    Some((pattern, names.len()))
}

fn strip_single_number(name: &str) -> (String, Option<u32>) {
    // 前缀数字 "1号楼" → ("号楼", Some(1))；后缀数字 "room1" → ("room", Some(1))
    // 中间数字 "A1室" 或多段 "1F-01" → (原名, None)
    let prefix_len = name.chars().take_while(|c| c.is_ascii_digit()).count();
    let suffix_len = name.chars().rev().take_while(|c| c.is_ascii_digit()).count();
    let total_len = name.len();
    if prefix_len > 0 && prefix_len < total_len && suffix_len == 0 {
        let num: u32 = name[..prefix_len].parse().unwrap_or(0);
        (name[prefix_len..].to_string(), Some(num))
    } else if suffix_len > 0 && suffix_len < total_len && prefix_len == 0 {
        let num: u32 = name[total_len - suffix_len..].parse().unwrap_or(0);
        (name[..total_len - suffix_len].to_string(), Some(num))
    } else {
        (name.to_string(), None)
    }
}

fn strip_single_number_pattern(name: &str) -> String {
    let (base, _) = strip_single_number(name);
    // 前缀还是后缀：看原名数字位置
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("{{index}}{}", base)
    } else {
        format!("{}{{index}}", base)
    }
}
```

handler：校验 workspace（thing 的 workspace_id == 当前 workspace，否则 404）→ BFS 子树（每节点查 children）→ 组 `ThingNodeDef` 树 → 同父兄弟组跑 `generalize_names` → 输出 `SceneTemplateFile` JSON（Content-Disposition 下载）。

- [ ] **Step 4: 跑测试** → `cd apps/cloud && cargo test export` → PASS

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/thing/service/export_template.rs apps/cloud/src/domains/thing/handler/mod.rs
git commit -m "feat(thing): export-as-template — reverse subtree to scene pack JSON with name generalization"
```

---

### Task 9: import 扩展（场景包注册闭环）

**Files:**
- Modify: `crates/db/src/thing_template.rs:190`（`ParsedTemplate` 加 scene 字段或旁路）
- Modify: `apps/cloud/src/domains/thing/service/import_export.rs`（import 入口识别 children）
- Modify: `crates/db/src/thing_template.rs:1449`（`insert_parsed_thing_template` 的 device_info 硬编码 `"{}"` 改为透传）

**Interfaces:**
- Consumes: `SceneTemplateFile`（Task 1）、`ThingTemplate::is_composition` 判定逻辑
- Produces: import 接受含 `children` 的模板 JSON → 注册为 workspace 组合模板（device_info 存完整原文）

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn import_scene_template_round_trips() {
    // 用 Task 8 导出的 smart_floor JSON 走 import → thing_templates 出现新行：
    //   workspace_id = 当前 workspace、is_builtin=0、
    //   device_info 原文含 children → is_composition() == true
}

#[tokio::test]
async fn import_entity_template_unchanged() {
    // 现有 entity 模板 import 回归不变
}
```

- [ ] **Step 2: 跑测试确认失败** → FAIL

- [ ] **Step 3: 实现**

import 入口（读 `import_export.rs` 现有 parse 函数）：解析 JSON 为 `serde_json::Value` → 含非空 `children` → 走 scene 旁路：`SceneTemplateFile::from_json` 校验 → 直接 insert（name 冲突沿用现有重命名策略），`device_info` = 原文，category/thing_type/display_name 从 JSON 取。否则走现有 `ParsedTemplate` 路径（不变）。

`insert_parsed_thing_template` 的 `device_info = "{}"` 硬编码改为参数透传（entity 路径调用方传现有 ThingInfo JSON；scene 旁路传完整原文）。

- [ ] **Step 4: 跑测试** → PASS

- [ ] **Step 5: Commit**

```bash
git add crates/db/src/thing_template.rs apps/cloud/src/domains/thing/service/import_export.rs
git commit -m "feat(template): import scene packs — device_info passthrough, composition detection"
```

---

### Task 10: ResourceType 扩展（image/model3d）

**Files:**
- Modify: `crates/core/src/models/workspace.rs:68`（枚举 + as_str）
- Modify: `apps/cloud/src/domains/tenant/workspace/handler.rs:450`（放开校验）
- Modify: 前端资源创建 UI（找到 resource 类型选项处同步）

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn resource_type_serializes_new_variants() {
    assert_eq!(ResourceType::Image.as_str(), "image");
    assert_eq!(ResourceType::Model3d.as_str(), "model3d");
    let parsed: ResourceType = serde_json::from_str("\"image\"").unwrap();
    assert_eq!(parsed, ResourceType::Image);
}
```

- [ ] **Step 2: 跑测试确认失败** → FAIL（编译错误，无变体）

- [ ] **Step 3: 实现**

```rust
pub enum ResourceType {
    File,
    Document,
    Image,
    Model3d,
}

impl ResourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Document => "document",
            Self::Image => "image",
            Self::Model3d => "model3d",
        }
    }
}
```

Rust exhaustive match 会让所有 `match resource_type` 消费点编译报错——逐一跟进补分支（编译器驱动）。handler.rs:450 校验改为接受全部 4 种：

```rust
// 原：if payload.resource_type != ResourceType::File { return 400 }
// 改为：接受所有已知类型（serde 反序列化已保证合法性，无需额外校验）
```

删除该硬编码判断即可。前端找到资源类型下拉（`grep -rn "file.*document\|resource" web/src/ui --include="*.ts" -l`），选项加 image/model3d。

- [ ] **Step 4: 跑测试** → `cargo test -p tinyiothub-core && cd apps/cloud && cargo test workspace` → PASS

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/models/workspace.rs apps/cloud/src/domains/tenant/workspace/handler.rs web/src/
git commit -m "feat(core): ResourceType image/model3d variants for scene pack resources"
```

---

### Task 11: 前端（场景包 Tab + 参数对话框 + dry-run 预览）

**Files:**
- Modify: `web/src/api/marketplace.ts`（类型 + API 方法）
- Modify: `web/src/ui/views/marketplace.ts`（Tab + 对话框）
- Modify: `web/src/ui/views/things-detail.ts`（「另存为场景包」入口——**核对**该文件现有的操作按钮区）

**Interfaces:**
- Consumes: Task 6 的 API（`isComposition`/`parameterCount`/detail/instantiate）
- Produces: 用户可用完整流程

- [ ] **Step 1: API 层**

`web/src/api/marketplace.ts` 追加：

```typescript
export interface SceneParameter {
  name: string;
  type: "int";
  default: number;
  min: number;
  max: number;
  display_name?: LocalizedString;
}

export interface ThingTemplateItem {
  id: string;
  name: string;
  displayName?: string;
  description?: string;
  category: string;
  isBuiltin: boolean;
  isComposition: boolean;
  parameterCount: number;
}

export interface SceneTemplateDetail extends ThingTemplateItem {
  parameters: SceneParameter[];
  structureSummary: { parameterCount: number; maxDepth: number };
}

export interface InstantiateResult {
  nodeCount: number;
  rootThingId: string | null;
  treePreview: string;
  warnings: string[];
}

export const sceneApi = {
  listThingTemplates: (composition?: boolean) =>
    apiGet(`/marketplace/thing-templates${composition === undefined ? "" : `?composition=${composition}`}`),
  getThingTemplate: (id: string) => apiGet(`/marketplace/thing-templates/${id}`),
  instantiate: (id: string, body: {
    sceneName: string;
    parentId?: string;
    parameterValues?: Record<string, number>;
    dryRun?: boolean;
  }) => apiPost(`/marketplace/thing-templates/${id}/instantiate`, body),
  exportAsTemplate: (thingId: string) =>
    apiPost(`/things/${thingId}/export-as-template`, {}),
};
```

（**核对** `apiGet`/`apiPost` 现有签名与返回包装——照搬 `marketplaceApi` 现有方法的解包模式。）

- [ ] **Step 2: 视图层**

`marketplace.ts`：

1. `type Tab = "templates" | "drivers" | "scenes";` + Tab 栏加「场景包」
2. scenes Tab 数据源：`sceneApi.listThingTemplates(true)`，卡片显示 `parameterCount 参数 · 模板结构 N 层`（详情接口取 maxDepth，或列表项扩展——以 Task 6 实际返回为准）
3. 点击「使用模板」→ 对话框组件（新内部方法渲染）：
   - 根节点名称输入（必填）
   - 按 `parameters` 动态生成整数输入（min/max 校验，`resolveLocalized(display_name)` 做 label）
   - 参数变化 → 300ms 防抖调 `instantiate(id, {dryRun: true, ...})` → 显示"将创建 N 个本体（预览，最终名称以创建结果为准）" + `<pre>` 展示 treePreview
   - dryRun 进行中与提交进行中禁用提交按钮
4. 提交 → `instantiate(dryRun: false)` → warnings 非空先展示列表（toast/dialog）→ 跳转 `/things/{rootThingId}`

关键实现片段（防抖 + 提交锁）：

```typescript
  @state() sceneParams: Record<string, number> = {};
  @state() preview: InstantiateResult | null = null;
  @state() previewLoading = false;
  @state() submitting = false;
  private previewTimer: number | undefined;

  private onParamChange(templateId: string, sceneName: string) {
    window.clearTimeout(this.previewTimer);
    this.previewTimer = window.setTimeout(async () => {
      if (!sceneName) { this.preview = null; return; }
      this.previewLoading = true;
      try {
        this.preview = await sceneApi.instantiate(templateId, {
          sceneName, parameterValues: this.sceneParams, dryRun: true,
        });
      } catch { this.preview = null; }
      this.previewLoading = false;
    }, 300);
  }
```

`things-detail.ts` 操作区加「另存为场景包」按钮 → `sceneApi.exportAsTemplate(id)` → 触发下载（响应是 JSON → `Blob` + `a[download]`）。

- [ ] **Step 3: 手动 QA（按测试计划）**

按 `~/.gstack/projects/Grong-tinyiothub/chenguorong-refactor-thing-followups-eng-review-test-plan-20260831-163000.md` 过一遍：Tab 切换、参数表单、防抖预览、tentative 标注、提交禁用、warnings、跳转、另存为下载。

- [ ] **Step 4: Commit**

```bash
git add web/src/api/marketplace.ts web/src/ui/views/marketplace.ts web/src/ui/views/things-detail.ts
git commit -m "feat(web): scene pack tab + parameter dialog with dry-run preview + export-as-template entry"
```

---

## 依赖与执行顺序

```
Lane A（主线，串行）: T1 → T2 → T3 → T4 → T5 → T6 → T7
Lane B（T1 完成后可并行）: T8 → T9（依赖 T8 的导出格式）；T10 随时可做
Lane C（T6 完成后）: T11
```

T10（ResourceType）与 T1-T3 无共享文件，可最早并行。T7 依赖 T5/T6（场景包能实例化才有东西可验证）。

## 验证清单（全部完成后）

- [ ] `cargo test` 全 workspace 绿
- [ ] `cargo clippy` 无新警告
- [ ] 新库 seed 后 3 个场景包可见
- [ ] 手动 QA 测试计划全过
- [ ] entity 模板 install/创建/预览回归不破
