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

// ──────────────────────────────────────────────
// 展开器（纯函数，无 IO）
// ──────────────────────────────────────────────

use thiserror::Error;

use crate::thing_template::ThingTemplate;

pub const MAX_NODES: usize = 500;
pub const MAX_SCENE_REF_DEPTH: usize = 5;
/// 允许的告警规则类型（展开器校验 + 测试断言共用同一来源）。
/// event 规则存 EventAlarmCondition JSON（AlarmCondition 无 Event 变体），
/// 实例化时必失败，故不支持，校验期即拒绝。
pub const ALLOWED_RULE_TYPES: [&str; 3] = ["threshold", "range", "change"];

#[derive(Debug, Error)]
pub enum ExpandError {
    #[error("参数不存在: {name}")]
    InvalidParameter { name: String },
    #[error("参数 {name} 值 {value} 超出范围 [{min}, {max}]")]
    ParamOutOfRange {
        name: String,
        value: i64,
        min: i64,
        max: i64,
    },
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
    #[error("不支持的告警规则类型: {rule_type}（允许: threshold/range/change；event 暂不支持）")]
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
            return Err(ExpandError::TooLarge {
                count: self.nodes.len() + 1,
            });
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
        self.preview_lines
            .push(format!("{}{} ({})", "  ".repeat(depth), label, category));
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
                .ok_or_else(|| ExpandError::RefNotFound {
                    name: template_ref.clone(),
                })?;
            let category = tpl.category.clone();
            let props: Vec<PropertyTemplate> = serde_json::from_str(&tpl.properties).unwrap_or_default();
            let cmds: Vec<CommandTemplate> = serde_json::from_str(&tpl.actions).unwrap_or_default();
            let events: Vec<serde_json::Value> =
                serde_json::from_str(&tpl.events_json_for_inline()).unwrap_or_default();
            for i in 1..=copies {
                let name = self.node_name(node, i);
                let mut inlined = node.clone();
                inlined.properties = props.clone();
                inlined.commands = cmds.clone();
                inlined.events = events.clone();
                let display = self.node_display_name(&inlined, i, &name);
                let id = self.push_node(
                    parent_temp_id,
                    depth,
                    name,
                    display,
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
            return Err(ExpandError::RefCycle {
                chain: chain.join(" → "),
            });
        }
        if self.ref_stack.len() >= MAX_SCENE_REF_DEPTH {
            return Err(ExpandError::TooDeep);
        }
        let target = self
            .scene_templates
            .get(scene_ref)
            .ok_or_else(|| ExpandError::RefNotFound {
                name: scene_ref.to_string(),
            })?
            .clone();

        // 参数映射：目标参数名 ← 本模板参数值（按目标模板定义校验 min/max）
        let saved_params = self.params.clone();
        if let Some(mapping) = &node.param_mapping {
            for (target_param, source_param) in mapping {
                let value = *self
                    .params
                    .get(source_param)
                    .ok_or_else(|| ExpandError::InvalidParameter {
                        name: source_param.clone(),
                    })?;
                if let Some(def) = target.parameters.iter().find(|p| &p.name == target_param) {
                    check_param_range(scene_ref, def, value)?;
                }
                self.params.insert(target_param.clone(), value);
            }
        }
        // 未被映射覆盖的参数：用被引用模板默认值补齐（同样校验 min/max）
        for p in &target.parameters {
            let overridden = node.param_mapping.as_ref().is_some_and(|m| m.contains_key(&p.name));
            if overridden {
                continue;
            }
            check_param_range(scene_ref, p, p.default)?;
            self.params.insert(p.name.clone(), p.default);
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
        let pattern = node.device_info.default_name_pattern.as_deref().unwrap_or("{index}");
        let mut vars = HashMap::new();
        vars.insert("scene_name", self.scene_name.clone());
        vars.insert("index", index.to_string());
        render_name_pattern(pattern, &vars)
    }

    fn node_display_name(&self, node: &ThingNodeDef, index: usize, fallback: &str) -> Option<String> {
        node.device_info
            .default_display_name_pattern
            .as_ref()
            .and_then(localized)
            .map(|pattern| {
                let mut vars = HashMap::new();
                vars.insert("scene_name", self.scene_name.clone());
                vars.insert("index", index.to_string());
                render_name_pattern(&pattern, &vars)
            })
            .or_else(|| Some(fallback.to_string()))
    }
}

/// scene_ref 参数值按被引用模板定义校验 min/max；错误名带目标模板名前缀。
fn check_param_range(scene_ref: &str, def: &SceneParameter, value: i64) -> Result<(), ExpandError> {
    if value < def.min || value > def.max {
        return Err(ExpandError::ParamOutOfRange {
            name: format!("{}.{}", scene_ref, def.name),
            value,
            min: def.min,
            max: def.max,
        });
    }
    Ok(())
}

/// tree_preview 标签清洗：剔除换行/首尾空格，括号转全角。
/// pub：场景实例化器在名称解析后重建 tree_preview 时复用同一规则。
pub fn sanitize_label(s: &str) -> String {
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
    // 未知参数键拒绝：请求 map 中每个键必须在本模板 parameters 中声明，
    // 防笔误（如 building_counts）被静默忽略而用默认值创建出不同的树
    for key in parameter_values.keys() {
        if !template.parameters.iter().any(|p| &p.name == key) {
            return Err(ExpandError::InvalidParameter { name: key.clone() });
        }
    }
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

    fn campus_template() -> SceneTemplateFile {
        SceneTemplateFile::from_json(
            r#"{
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
        }"#,
        )
        .unwrap()
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
    fn expand_rejects_unknown_param_key() {
        let t = campus_template();
        // 笔误 building_counts（已声明的是 building_count）必须报错且指出冒犯键名
        let params = HashMap::from([("building_counts".to_string(), 2i64)]);
        let e = expand(&t, "园区", &params, &HashMap::new(), &HashMap::new()).unwrap_err();
        match e {
            ExpandError::InvalidParameter { name } => assert_eq!(name, "building_counts"),
            other => panic!("expected InvalidParameter, got {other:?}"),
        }
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
        let _t = campus_template();
        let _params = HashMap::from([
            ("building_count".to_string(), 10i64),
            ("floor_count".to_string(), 15i64),
        ]);
        // 1+10+150=161 OK；floor max 15 下不会超 500，造一个超限模板：
        let big = SceneTemplateFile::from_json(
            r#"{
            "name": "big", "display_name": {"zh":"大"}, "category": "scenes",
            "parameters": [{"name":"n","type":"int","default":600,"min":1,"max":1000,"display_name":{}}],
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"category":"room","count_param":"n","device_info":{"default_name_pattern":"{index}"}}]
        }"#,
        )
        .unwrap();
        let e = expand(&big, "x", &HashMap::new(), &HashMap::new(), &HashMap::new()).unwrap_err();
        assert!(matches!(e, ExpandError::TooLarge { .. }));
    }

    #[test]
    fn expand_rejects_both_count_fields() {
        let t = SceneTemplateFile::from_json(
            r#"{
            "name": "bad", "display_name": {"zh":"坏"}, "category": "scenes",
            "parameters": [{"name":"n","type":"int","default":2,"min":1,"max":9,"display_name":{}}],
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"key":"x","category":"room","count":2,"count_param":"n",
                          "device_info":{"default_name_pattern":"{index}"}}]
        }"#,
        )
        .unwrap();
        let e = expand(&t, "x", &HashMap::new(), &HashMap::new(), &HashMap::new()).unwrap_err();
        assert!(matches!(e, ExpandError::BothCountFields { .. }));
    }

    #[test]
    fn expand_scene_ref_inlines_subtree_and_maps_params() {
        let floor_pack = SceneTemplateFile::from_json(
            r#"{
            "name": "smart_floor", "display_name": {"zh":"楼层"}, "category": "scenes",
            "thing_category": "floor",
            "parameters": [{"name":"rooms","type":"int","default":3,"min":1,"max":50,"display_name":{}}],
            "device_info": {"default_name_pattern": "{scene_name}楼层"},
            "children": [{"category":"room","count_param":"rooms","device_info":{"default_name_pattern":"{index}室"}}]
        }"#,
        )
        .unwrap();
        let campus = SceneTemplateFile::from_json(
            r#"{
            "name": "c", "display_name": {"zh":"园"}, "category": "scenes",
            "parameters": [{"name":"n_rooms","type":"int","default":4,"min":1,"max":50,"display_name":{}}],
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"category":"building","device_info":{"default_name_pattern":"主楼"},
                          "children":[{"scene_ref":"smart_floor","param_mapping":{"rooms":"n_rooms"}}]}]
        }"#,
        )
        .unwrap();
        let scenes = HashMap::from([("smart_floor".to_string(), floor_pack)]);
        let r = expand(&campus, "园区", &HashMap::new(), &HashMap::new(), &scenes).unwrap();
        // 1 园 + 1 楼 + 1 楼层 + 4 室 = 7
        assert_eq!(r.node_count, 7);
        assert!(r.nodes.iter().any(|n| n.name == "4室"));
    }

    #[test]
    fn expand_detects_scene_ref_cycle() {
        let a = SceneTemplateFile::from_json(
            r#"{
            "name": "a", "display_name": {"zh":"a"}, "category": "scenes",
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"scene_ref": "b"}]
        }"#,
        )
        .unwrap();
        let b = SceneTemplateFile::from_json(
            r#"{
            "name": "b", "display_name": {"zh":"b"}, "category": "scenes",
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"scene_ref": "a"}]
        }"#,
        )
        .unwrap();
        let scenes = HashMap::from([("a".to_string(), a.clone()), ("b".to_string(), b)]);
        let e = expand(&a, "x", &HashMap::new(), &HashMap::new(), &scenes).unwrap_err();
        assert!(matches!(e, ExpandError::RefCycle { .. }));
    }

    #[test]
    fn expand_rejects_bad_rule_type() {
        let t = SceneTemplateFile::from_json(
            r#"{
            "name": "s", "display_name": {"zh":"s"}, "category": "scenes",
            "device_info": {"default_name_pattern": "{scene_name}"},
            "alarm_rules": [{"name":"r","rule_type":"duration","condition":{}}],
            "children": []
        }"#,
        )
        .unwrap();
        let e = expand(&t, "x", &HashMap::new(), &HashMap::new(), &HashMap::new()).unwrap_err();
        assert!(matches!(e, ExpandError::RuleTypeNotAllowed { .. }));
    }

    #[test]
    fn expand_scene_ref_fills_unmapped_defaults() {
        let floor_pack = SceneTemplateFile::from_json(
            r#"{
            "name": "smart_floor", "display_name": {"zh":"楼层"}, "category": "scenes",
            "thing_category": "floor",
            "parameters": [{"name":"rooms","type":"int","default":3,"min":1,"max":50,"display_name":{}}],
            "device_info": {"default_name_pattern": "{scene_name}楼层"},
            "children": [{"category":"room","count_param":"rooms","device_info":{"default_name_pattern":"{index}室"}}]
        }"#,
        )
        .unwrap();
        // 无 param_mapping：被引用模板的 rooms 必须用其默认值 3
        let campus = SceneTemplateFile::from_json(
            r#"{
            "name": "c", "display_name": {"zh":"园"}, "category": "scenes",
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"scene_ref":"smart_floor"}]
        }"#,
        )
        .unwrap();
        let scenes = HashMap::from([("smart_floor".to_string(), floor_pack)]);
        let r = expand(&campus, "园区", &HashMap::new(), &HashMap::new(), &scenes).unwrap();
        // 1 园 + 1 楼层 + 3 室 = 5
        assert_eq!(r.node_count, 5);
        assert!(r.nodes.iter().any(|n| n.name == "3室"));
    }

    #[test]
    fn expand_scene_ref_rejects_mapped_value_over_target_max() {
        // 目标 floors max 8；源 floor_count max 15 映射 15 → 目标护栏必须拦截
        let building_pack = SceneTemplateFile::from_json(
            r#"{
            "name": "smart_building", "display_name": {"zh":"楼"}, "category": "scenes",
            "thing_category": "building",
            "parameters": [{"name":"floors","type":"int","default":3,"min":1,"max":8,"display_name":{}}],
            "device_info": {"default_name_pattern": "{scene_name}楼"},
            "children": [{"category":"floor","count_param":"floors","device_info":{"default_name_pattern":"{index}F"}}]
        }"#,
        )
        .unwrap();
        let campus = SceneTemplateFile::from_json(
            r#"{
            "name": "c", "display_name": {"zh":"园"}, "category": "scenes",
            "parameters": [{"name":"floor_count","type":"int","default":5,"min":1,"max":15,"display_name":{}}],
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"scene_ref":"smart_building","param_mapping":{"floors":"floor_count"}}]
        }"#,
        )
        .unwrap();
        let scenes = HashMap::from([("smart_building".to_string(), building_pack)]);
        let params = HashMap::from([("floor_count".to_string(), 15i64)]);
        let e = expand(&campus, "园区", &params, &HashMap::new(), &scenes).unwrap_err();
        match e {
            ExpandError::ParamOutOfRange { name, value, max, .. } => {
                assert!(name.contains("smart_building"), "错误须含目标模板名: {name}");
                assert!(name.contains("floors"), "错误须含目标参数名: {name}");
                assert_eq!(value, 15);
                assert_eq!(max, 8);
            }
            other => panic!("expected ParamOutOfRange, got: {other:?}"),
        }
    }

    #[test]
    fn expand_rejects_event_rule_type() {
        let t = SceneTemplateFile::from_json(
            r#"{
            "name": "s", "display_name": {"zh":"s"}, "category": "scenes",
            "device_info": {"default_name_pattern": "{scene_name}"},
            "alarm_rules": [{"name":"r","rule_type":"event","condition":{"type":"event","event_name":"x"}}],
            "children": []
        }"#,
        )
        .unwrap();
        let e = expand(&t, "x", &HashMap::new(), &HashMap::new(), &HashMap::new()).unwrap_err();
        match e {
            ExpandError::RuleTypeNotAllowed { rule_type } => assert_eq!(rule_type, "event"),
            other => panic!("expected RuleTypeNotAllowed, got: {other:?}"),
        }
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

    #[test]
    fn expand_template_ref_inlines_properties_commands_events() {
        let t = SceneTemplateFile::from_json(
            r#"{
            "name": "s", "display_name": {"zh":"s"}, "category": "scenes",
            "device_info": {"default_name_pattern": "{scene_name}"},
            "children": [{"template_ref": "th_sensor", "device_info": {"default_name_pattern": "传感器{index}",
                "default_display_name_pattern": {"zh": "温湿度传感器 {index}", "en": "TH Sensor {index}"}}}]
        }"#,
        )
        .unwrap();
        let tpl = crate::thing_template::ThingTemplate {
            category: "sensors".to_string(),
            properties: r#"[{"name":"temp","display_name":{"zh":"温度"},"data_type":"float","is_read_only":true,"is_required":false}]"#.to_string(),
            actions: r#"[{"name":"reboot","display_name":{"zh":"重启"},"is_required":false}]"#.to_string(),
            events: r#"[{"name":"overheat","level":"warning"}]"#.to_string(),
            ..Default::default()
        };
        let device_templates = HashMap::from([("th_sensor".to_string(), tpl)]);
        let r = expand(&t, "站点", &HashMap::new(), &device_templates, &HashMap::new()).unwrap();
        // 根 + 1 内联节点
        assert_eq!(r.node_count, 2);
        let inlined = &r.nodes[1];
        assert_eq!(inlined.name, "传感器1");
        // template_ref 分支必须应用 default_display_name_pattern，而非回退机器名
        assert_eq!(inlined.display_name.as_deref(), Some("温湿度传感器 1"));
        assert_eq!(inlined.category, "sensors");
        assert_eq!(inlined.properties.len(), 1);
        assert_eq!(inlined.properties[0].name, "temp");
        assert_eq!(inlined.commands.len(), 1);
        assert_eq!(inlined.commands[0].name, "reboot");
        // events_json_for_inline 桩改真后防回归：events 必须来自模板 events 列
        assert_eq!(inlined.event_defs.len(), 1);
        assert_eq!(inlined.event_defs[0]["name"], "overheat");
    }
}
