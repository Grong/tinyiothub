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
