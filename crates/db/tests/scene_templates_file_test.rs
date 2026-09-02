//! 内置场景包模板文件校验：3 个 JSON 源文件可解析、children 非空，
//! smart_floor（无 template_ref）默认参数可完整展开。

#[test]
fn builtin_scene_templates_parse_and_validate() {
    for file in ["smart_campus", "smart_building", "smart_floor"] {
        let path = format!("../../templates/builtin/scenes/{}.json", file);
        let content = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} 读取失败: {}", path, e));
        let t = tinyiothub_storage::scene_template::SceneTemplateFile::from_json(&content)
            .unwrap_or_else(|e| panic!("{} 解析失败: {}", file, e));
        assert!(!t.children.is_empty(), "{} 必须有 children", file);
        // 展开冒烟：默认参数能展开且不超限
        let r = tinyiothub_storage::scene_template::expand(
            &t,
            "测试",
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        // smart_campus/smart_building 引用 temperature_humidity_sensor —— 无设备模板 map 会 RefNotFound；
        // 这里只验证结构解析；完整展开在集成测试覆盖
        assert!(r.is_ok() || file != "smart_floor", "{} 展开失败", file);

        // 高温告警必须挂在 th_sensor（template_ref 叶节点）上——实例化器只解析本节点属性
        if file != "smart_floor" {
            fn find_node<'a>(
                nodes: &'a [tinyiothub_storage::scene_template::ThingNodeDef],
                key: &str,
            ) -> Option<&'a tinyiothub_storage::scene_template::ThingNodeDef> {
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
