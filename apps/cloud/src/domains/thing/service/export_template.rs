//! 反向导出：本体子树 → 场景包模板 JSON。
//!
//!   子树遍历（BFS）──▶ ThingNodeDef 递归构建
//!        │ 命名泛化：同父兄弟去数字后相等 + 数字从 1 连续 → "{index}X" + count
//!        │ （多段数字/中间数字不泛化，保留原名 + warning）
//!        ▼
//!   SceneTemplateFile JSON 下载（不入库；注册走 import 流程）

use std::collections::{HashMap, VecDeque};

use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_core::models::thing_property::ThingProperty;
use tinyiothub_storage::alarm_rule::AlarmRule;
use tinyiothub_storage::scene_template::{
    MAX_NODES, SceneAlarmRule, SceneNodeInfo, SceneResource, SceneTemplateFile, ThingNodeDef,
};
use tinyiothub_storage::thing::{ThingResource, ThingRow};
use tinyiothub_storage::thing_template::{CommandTemplate, PropertyTemplate};
use tinyiothub_storage::{Db, DbError};

#[derive(Debug)]
pub struct ExportOutcome {
    pub file: SceneTemplateFile,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("本体不存在或不属于当前 workspace: {0}")]
    NotFound(String),
    #[error("子树节点数 {0} 超过上限 {MAX_NODES}，请缩小导出范围")]
    TooLarge(usize),
    #[error("数据库错误: {0}")]
    Database(#[from] sqlx::Error),
    #[error("存储错误: {0}")]
    Storage(#[from] DbError),
}

/// 导出指定本体及其整棵子树为场景包模板。
/// 根节点 workspace 严格校验（防 IDOR）；子树 > MAX_NODES 拒绝。
pub async fn export_subtree_as_template(
    db: &Db,
    workspace_id: &str,
    root_id: &str,
) -> Result<ExportOutcome, ExportError> {
    let root = db
        .find_thing_by_id_scoped(root_id, workspace_id)
        .await?
        .ok_or_else(|| ExportError::NotFound(root_id.to_string()))?;

    // BFS 收集子树；发现总数超上限即拒绝
    let mut rows: Vec<ThingRow> = Vec::new();
    let mut queue: VecDeque<ThingRow> = VecDeque::from([root.clone()]);
    while let Some(node) = queue.pop_front() {
        let children = db.find_thing_child_rows(&node.id).await?;
        queue.extend(children);
        rows.push(node);
        if rows.len() + queue.len() > MAX_NODES {
            return Err(ExportError::TooLarge(rows.len() + queue.len()));
        }
    }

    // 每节点属性/命令定义（设备节点不逆向 template_ref，仅导出原文）
    // 同时收集 resources / alarm_rules（spec §3.4）；prop_names 供 property_id 反查属性名
    let mut properties: HashMap<String, Vec<PropertyTemplate>> = HashMap::new();
    let mut commands: HashMap<String, Vec<CommandTemplate>> = HashMap::new();
    let mut prop_names: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut resources: HashMap<String, Vec<SceneResource>> = HashMap::new();
    let mut alarm_rules: HashMap<String, Vec<SceneAlarmRule>> = HashMap::new();
    let mut warnings = Vec::new();
    for r in &rows {
        let props = db.find_thing_properties_by_thing_id(&r.id).await?;
        prop_names.insert(
            r.id.clone(),
            props.iter().map(|p| (p.id.clone(), p.name.clone())).collect(),
        );
        properties.insert(r.id.clone(), props.iter().map(property_template).collect());
        let cmds = db.find_thing_commands_by_thing_id(&r.id).await?;
        commands.insert(r.id.clone(), cmds.iter().map(command_template).collect());

        // 排序保证导出确定性（兄弟节点结构等价比较对顺序敏感）
        let mut res: Vec<SceneResource> = db
            .list_thing_resources(&r.id)
            .await?
            .iter()
            .map(resource_template)
            .collect();
        res.sort_by(|a, b| a.name.cmp(&b.name));
        resources.insert(r.id.clone(), res);

        let rules = db.find_alarm_rules_by_thing(&r.id, Some(workspace_id)).await?;
        let names = &prop_names[&r.id];
        let mut ar: Vec<SceneAlarmRule> = rules
            .iter()
            .map(|rule| alarm_rule_template(rule, names, &r.name, &mut warnings))
            .collect();
        ar.sort_by(|a, b| a.name.cmp(&b.name));
        alarm_rules.insert(r.id.clone(), ar);
    }

    let mut by_parent: HashMap<String, Vec<&ThingRow>> = HashMap::new();
    for r in &rows {
        if let Some(pid) = &r.parent_id {
            by_parent.entry(pid.clone()).or_default().push(r);
        }
    }
    let ctx = SubtreeContext {
        by_parent,
        properties,
        commands,
        resources,
        alarm_rules,
    };

    // SceneTemplateFile 无根 thing_type 字段，偏离缺省映射时导出后会按缺省还原
    let root_default = default_mapped_type(root.category.as_deref());
    if root.thing_type != root_default {
        warnings.push(format!(
            "根节点 thing_type「{}」与缺省映射「{}」不一致，重新实例化时将按缺省还原",
            root.thing_type, root_default
        ));
    }

    let linked = parse_linked(&root);
    let file = SceneTemplateFile {
        name: root.name.clone(),
        display_name: HashMap::from([(
            "zh".to_string(),
            root.display_name.clone().unwrap_or_else(|| root.name.clone()),
        )]),
        description: None,
        version: "1.0.0".to_string(),
        category: "scenes".to_string(),
        thing_category: root.category.clone(),
        parameters: vec![],
        // 根命名缺省 {scene_name}，由实例化时的场景名决定
        device_info: SceneNodeInfo::default(),
        properties: ctx.properties.get(&root.id).cloned().unwrap_or_default(),
        commands: ctx.commands.get(&root.id).cloned().unwrap_or_default(),
        events: linked_array(&linked, "event_defs"),
        default_knowledge: linked_str(&linked, "knowledge"),
        resources: ctx.resources.get(&root.id).cloned().unwrap_or_default(),
        dashboard: linked.get("dashboard").cloned(),
        alarm_rules: ctx.alarm_rules.get(&root.id).cloned().unwrap_or_default(),
        children: build_children(&root, &ctx, &mut warnings),
    };
    Ok(ExportOutcome { file, warnings })
}

struct SubtreeContext<'a> {
    by_parent: HashMap<String, Vec<&'a ThingRow>>,
    properties: HashMap<String, Vec<PropertyTemplate>>,
    commands: HashMap<String, Vec<CommandTemplate>>,
    resources: HashMap<String, Vec<SceneResource>>,
    alarm_rules: HashMap<String, Vec<SceneAlarmRule>>,
}

/// 递归构建子节点定义；同父兄弟可泛化时合并为单节点（count + name pattern）。
fn build_children(parent: &ThingRow, ctx: &SubtreeContext, warnings: &mut Vec<String>) -> Vec<ThingNodeDef> {
    let Some(kids) = ctx.by_parent.get(&parent.id) else {
        return vec![];
    };
    let defs: Vec<ThingNodeDef> = kids.iter().map(|k| build_node(k, ctx, warnings)).collect();
    if defs.len() <= 1 {
        return defs;
    }
    let names: Vec<String> = kids.iter().map(|k| k.name.clone()).collect();
    let Some((pattern, count)) = generalize_names(&names) else {
        warnings.push(format!(
            "父节点「{}」的子节点命名无法泛化（{}），已保留原名",
            parent.name,
            names.join("、")
        ));
        return defs;
    };
    // 命名可泛化还不够：结构（category/属性/命令/子树等）必须一致，否则合并丢数据
    let first = normalized(&defs[0]);
    if !defs.iter().all(|d| normalized(d) == first) {
        warnings.push(format!(
            "父节点「{}」的子节点（{}）命名可泛化但结构不一致，已保留原名",
            parent.name,
            names.join("、")
        ));
        return defs;
    }
    let mut merged = defs.into_iter().next().expect("non-empty defs");
    merged.count = Some(count as u32);
    merged.device_info.default_name_pattern = Some(pattern);
    vec![merged]
}

fn build_node(row: &ThingRow, ctx: &SubtreeContext, warnings: &mut Vec<String>) -> ThingNodeDef {
    let linked = parse_linked(row);
    ThingNodeDef {
        key: None,
        category: row.category.clone(),
        thing_type: explicit_thing_type(row),
        count: None,
        count_param: None,
        template_ref: None,
        scene_ref: None,
        param_mapping: None,
        device_info: SceneNodeInfo {
            default_name_pattern: Some(row.name.clone()),
            default_display_name_pattern: display_pattern(row),
        },
        properties: ctx.properties.get(&row.id).cloned().unwrap_or_default(),
        commands: ctx.commands.get(&row.id).cloned().unwrap_or_default(),
        events: linked_array(&linked, "event_defs"),
        default_knowledge: linked_str(&linked, "knowledge"),
        resources: ctx.resources.get(&row.id).cloned().unwrap_or_default(),
        dashboard: linked.get("dashboard").cloned(),
        alarm_rules: ctx.alarm_rules.get(&row.id).cloned().unwrap_or_default(),
        children: build_children(row, ctx, warnings),
    }
}

/// 结构等价比较：剔除命名模式后按 JSON 值比较。
/// resources/alarm_rules 随 ThingNodeDef 整体序列化自动纳入比较——
/// 资源或告警规则不同的同名兄弟不会被泛化合并（否则会丢数据）。
fn normalized(def: &ThingNodeDef) -> serde_json::Value {
    let mut d = def.clone();
    d.device_info.default_name_pattern = None;
    d.device_info.default_display_name_pattern = None;
    serde_json::to_value(&d).expect("ThingNodeDef serializes")
}

/// 附录 A 缺省映射（非设备节点）：building→building，其余空间→space。
fn default_mapped_type(category: Option<&str>) -> &'static str {
    match category {
        Some("building") => "building",
        _ => "space",
    }
}

/// thing_type 仅当与缺省映射不一致时显式写入（设备节点恒为 Some("device")）。
fn explicit_thing_type(row: &ThingRow) -> Option<String> {
    let default = default_mapped_type(row.category.as_deref());
    if row.thing_type == default {
        None
    } else {
        Some(row.thing_type.clone())
    }
}

/// display_name 与 name 不一致时导出为单语言 display 模式（zh）。
fn display_pattern(row: &ThingRow) -> Option<HashMap<String, String>> {
    let display = row.display_name.as_deref()?;
    if display == row.name {
        return None;
    }
    Some(HashMap::from([("zh".to_string(), display.to_string())]))
}

fn parse_linked(row: &ThingRow) -> serde_json::Value {
    row.linked_data
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null)
}

/// linked_data 顶层键还原：knowledge（字符串）。
fn linked_str(linked: &serde_json::Value, key: &str) -> Option<String> {
    linked.get(key).and_then(|v| v.as_str()).map(String::from)
}

/// linked_data 顶层键还原：event_defs（数组）。
fn linked_array(linked: &serde_json::Value, key: &str) -> Vec<serde_json::Value> {
    linked.get(key).and_then(|v| v.as_array()).cloned().unwrap_or_default()
}

fn property_template(p: &ThingProperty) -> PropertyTemplate {
    PropertyTemplate {
        name: p.name.clone(),
        display_name: p
            .display_name
            .as_deref()
            .map(|s| HashMap::from([("zh".to_string(), s.to_string())]))
            .unwrap_or_default(),
        description: p
            .description
            .as_deref()
            .map(|s| HashMap::from([("zh".to_string(), s.to_string())])),
        data_type: p.data_type.clone().unwrap_or_else(|| "string".to_string()),
        unit: p.unit.clone(),
        min_value: p.min_value,
        max_value: p.max_value,
        default_value: p.default_value.clone(),
        is_read_only: p.is_read_only != 0,
        is_required: false,
        validation_rules: None,
    }
}

fn command_template(c: &ThingCommand) -> CommandTemplate {
    CommandTemplate {
        name: c.name.clone(),
        display_name: c
            .display_name
            .as_deref()
            .map(|s| HashMap::from([("zh".to_string(), s.to_string())]))
            .unwrap_or_default(),
        description: c
            .description
            .as_deref()
            .map(|s| HashMap::from([("zh".to_string(), s.to_string())])),
        parameters: c.parameters.clone(),
        parameter_schema: None,
        is_required: false,
    }
}

/// resources 行 → 模板 SceneResource（name/resource_type→type/file_path→uri）。
fn resource_template(r: &ThingResource) -> SceneResource {
    SceneResource {
        name: r.name.clone(),
        resource_type: r.resource_type.clone(),
        uri: r.file_path.clone(),
    }
}

/// AlarmRule → 模板简写 SceneAlarmRule；property_id 反查属性名写入 property_ref。
fn alarm_rule_template(
    rule: &AlarmRule,
    prop_names: &HashMap<String, String>,
    thing_name: &str,
    warnings: &mut Vec<String>,
) -> SceneAlarmRule {
    let rule_type = rule.rule_type.as_str().to_string();
    // 展开器仅允许 threshold/range/change/event；duration/composite 导出后无法重实例化
    if !["threshold", "range", "change", "event"].contains(&rule_type.as_str()) {
        warnings.push(format!(
            "节点「{}」的告警规则「{}」类型 {} 不在模板允许范围（threshold/range/change/event），重新实例化将被拒绝",
            thing_name, rule.name, rule_type
        ));
    }
    let property_ref = match &rule.property_id {
        Some(pid) => match prop_names.get(pid) {
            Some(name) => Some(name.clone()),
            None => {
                warnings.push(format!(
                    "节点「{}」的告警规则「{}」引用的属性 {pid} 已不存在，property_ref 已省略",
                    thing_name, rule.name
                ));
                None
            }
        },
        None => None,
    };
    SceneAlarmRule {
        name: rule.name.clone(),
        description: rule.description.clone(),
        rule_type,
        condition: serde_json::to_value(&rule.condition).unwrap_or_default(),
        alarm_level: rule.alarm_level.as_str().to_string(),
        notification_config: serde_json::to_value(&rule.notification_config).unwrap_or_default(),
        property_ref,
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generalize_prefix_numbers() {
        let names = vec!["1号楼".to_string(), "2号楼".to_string()];
        assert_eq!(generalize_names(&names), Some(("{index}号楼".to_string(), 2)));
    }

    #[test]
    fn generalize_suffix_numbers() {
        let names = vec!["room2".to_string(), "room1".to_string()];
        assert_eq!(generalize_names(&names), Some(("room{index}".to_string(), 2)));
    }

    #[test]
    fn no_generalize_for_middle_or_multi_segment_numbers() {
        // 中间数字
        assert_eq!(generalize_names(&["A1室".to_string(), "A2室".to_string()]), None);
        // 多段数字
        assert_eq!(generalize_names(&["1F-01".to_string(), "1F-02".to_string()]), None);
    }

    #[test]
    fn no_generalize_for_non_consecutive_or_missing_numbers() {
        // 非从 1 连续
        assert_eq!(generalize_names(&["room1".to_string(), "room3".to_string()]), None);
        // 有的名字没数字
        assert_eq!(generalize_names(&["1号楼".to_string(), "号楼".to_string()]), None);
        // 去数字后不相等
        assert_eq!(generalize_names(&["1号楼".to_string(), "2号库".to_string()]), None);
        // 纯数字名（整串都是数字）不泛化
        assert_eq!(generalize_names(&["1".to_string(), "2".to_string()]), None);
    }
}
