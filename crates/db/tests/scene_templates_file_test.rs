//! 内置场景包模板文件校验：3 个 JSON 源文件可解析、children 非空，
//! smart_floor（无 template_ref）默认参数可完整展开；
//! 所有 alarm_rules 的 condition 必须可反序列化为 AlarmCondition（实例化器落库
//! 时做同样解析，此处提前拦截非法内置数据），rule_type 必须在允许集合内
//! （与展开器共用 ALLOWED_RULE_TYPES 单一事实来源）。

use tinyiothub_storage::alarm_rule::AlarmCondition;
use tinyiothub_storage::scene_template::{
    ALLOWED_RULE_TYPES, ExpandError, SceneAlarmRule, SceneTemplateFile, ThingNodeDef,
};

/// 递归收集根节点与全部 children 上声明的告警规则。
fn collect_alarm_rules(t: &SceneTemplateFile) -> Vec<&SceneAlarmRule> {
    fn walk<'a>(nodes: &'a [ThingNodeDef], out: &mut Vec<&'a SceneAlarmRule>) {
        for n in nodes {
            out.extend(n.alarm_rules.iter());
            walk(&n.children, out);
        }
    }
    let mut out: Vec<&SceneAlarmRule> = t.alarm_rules.iter().collect();
    walk(&t.children, &mut out);
    out
}

#[test]
fn builtin_scene_templates_parse_and_validate() {
    for file in ["smart_campus", "smart_building", "smart_floor"] {
        let path = format!("../../templates/builtin/scenes/{}.json", file);
        let content = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} 读取失败: {}", path, e));
        let t = SceneTemplateFile::from_json(&content)
            .unwrap_or_else(|e| panic!("{} 解析失败: {}", file, e));
        assert!(!t.children.is_empty(), "{} 必须有 children", file);

        // 告警规则：condition 必须可解析为 AlarmCondition，rule_type 必须在允许集合内
        for rule in collect_alarm_rules(&t) {
            serde_json::from_value::<AlarmCondition>(rule.condition.clone()).unwrap_or_else(|e| {
                panic!("{} 规则「{}」condition 非法: {}（{:?}）", file, rule.name, e, rule.condition)
            });
            assert!(
                ALLOWED_RULE_TYPES.contains(&rule.rule_type.as_str()),
                "{} 规则「{}」rule_type 非法: {}",
                file,
                rule.name,
                rule.rule_type
            );
        }

        // 展开冒烟：默认参数能展开且不超限
        let r = tinyiothub_storage::scene_template::expand(
            &t,
            "测试",
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        if file == "smart_floor" {
            // 无 template_ref，必须完整展开成功
            r.unwrap_or_else(|e| panic!("{} 展开失败: {}", file, e));
        } else {
            // smart_campus/smart_building 引用 temperature_humidity_sensor ——
            // 无设备模板 map 时必须精确报 RefNotFound（完整展开由集成测试覆盖）
            let e = r.unwrap_err();
            assert!(
                matches!(&e, ExpandError::RefNotFound { name } if name == "temperature_humidity_sensor"),
                "{} 展开应报 temperature_humidity_sensor RefNotFound，实际: {:?}",
                file,
                e
            );
        }

        // 高温告警必须挂在 th_sensor（template_ref 叶节点）上——实例化器只解析本节点属性
        if file != "smart_floor" {
            fn find_node<'a>(nodes: &'a [ThingNodeDef], key: &str) -> Option<&'a ThingNodeDef> {
                nodes.iter().find_map(|n| {
                    if n.key.as_deref() == Some(key) {
                        Some(n)
                    } else {
                        find_node(&n.children, key)
                    }
                })
            }
            let th = find_node(&t.children, "th_sensor").unwrap_or_else(|| panic!("{} 缺少 th_sensor 节点", file));
            assert!(
                th.alarm_rules.iter().any(|r| r.name == "高温告警"),
                "{} 的 th_sensor 节点必须声明高温告警（property_ref 仅本节点属性）",
                file
            );
        }
    }
}
