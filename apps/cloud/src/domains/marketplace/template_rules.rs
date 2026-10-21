//! E3 模板=场景包超集（spec 2026-10-21-e3 §5）：
//! `watch_rules` + `composites` 的引用解析与校验报告。
//!
//! 纯函数：输入模板 JSON + `{slug → thing_id}` 映射 + 既有 composite 名，
//! 输出可落库的 `WatchRule` / `CompositeDef` 或逐条问题。任何引用失败 →
//! `ok=false`，调用方整包拒绝（不静默部分成功）。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tinyiothub_agent::runtime::thing_agent::seams::composite::{COMPOSITE_V, CompositeDef, CompositeOp, CompositeStep};
use tinyiothub_core::watch::{CompareOp, HealthLevel, WatchCondition, WatchRule};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceIssue {
    pub path: String,
    pub ref_id: Option<String>,
    /// missing_device | unresolved | invalid
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub kind: String,
    pub ref_id: String,
    pub message: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCount {
    pub watch_rules: usize,
    pub composites: usize,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub ok: bool,
    pub references: Vec<ReferenceIssue>,
    pub conflicts: Vec<Conflict>,
    pub resolved: ResolvedCount,
}

#[derive(Debug, Default)]
pub struct ResolvedRules {
    pub watch_rules: Vec<WatchRule>,
    pub composites: Vec<CompositeDef>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct RuleJson {
    #[serde(default, rename = "ref")]
    ref_id: Option<String>,
    /// 设备引用（slug）。
    thing: String,
    kind: String,
    #[serde(default)]
    property: Option<String>,
    #[serde(default)]
    op: Option<String>,
    #[serde(default)]
    value: Option<Value>,
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    online: Option<bool>,
    #[serde(default)]
    persistent: bool,
    #[serde(default = "default_timeout")]
    timeout_secs: u32,
}

fn default_timeout() -> u32 {
    600
}

#[derive(Debug, Deserialize)]
struct CompositeJson {
    name: String,
    #[serde(default)]
    description: String,
    steps: Vec<StepJson>,
}

#[derive(Debug, Deserialize)]
struct StepJson {
    #[serde(flatten)]
    op: OpJson,
    #[serde(default)]
    rollback: Option<OpJson>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum OpJson {
    Call {
        thing: String,
        action: String,
        #[serde(default)]
        params: Value,
    },
    Set {
        thing: String,
        properties: Value,
    },
}

impl OpJson {
    fn thing(&self) -> &str {
        match self {
            Self::Call { thing, .. } | Self::Set { thing, .. } => thing,
        }
    }
}

/// 解析模板中的 watch_rules/composites。
pub fn resolve_template_rules(
    template: &Value,
    workspace_id: &str,
    mapping: &HashMap<String, String>,
    existing_composite_names: &[String],
) -> (ResolvedRules, ValidationReport) {
    let mut resolved = ResolvedRules::default();
    let mut report = ValidationReport::default();
    let mut seen_composites: Vec<String> = Vec::new();

    if let Some(rules) = template.get("watch_rules").and_then(|v| v.as_array()) {
        for (i, raw) in rules.iter().enumerate() {
            let path = format!("watch_rules[{i}]");
            let rule: RuleJson = match serde_json::from_value(raw.clone()) {
                Ok(r) => r,
                Err(e) => {
                    report.references.push(ReferenceIssue {
                        path,
                        ref_id: None,
                        status: "invalid".into(),
                        message: format!("规则解析失败: {e}"),
                    });
                    continue;
                }
            };
            let Some(thing_id) = mapping.get(&rule.thing).cloned() else {
                report.references.push(ReferenceIssue {
                    path,
                    ref_id: rule.ref_id.clone(),
                    status: "missing_device".into(),
                    message: format!("物引用 `{}` 未在映射中提供", rule.thing),
                });
                continue;
            };
            let condition = match build_condition(&rule, &thing_id) {
                Ok(c) => c,
                Err(e) => {
                    report.references.push(ReferenceIssue {
                        path,
                        ref_id: rule.ref_id.clone(),
                        status: "invalid".into(),
                        message: e,
                    });
                    continue;
                }
            };
            let watch = WatchRule {
                id: format!("wr_{}", uuid::Uuid::new_v4().simple()),
                workspace_id: workspace_id.to_string(),
                condition,
                persistent: rule.persistent,
                timeout_secs: rule.timeout_secs,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            if let Err(e) = watch.validate() {
                report.references.push(ReferenceIssue {
                    path,
                    ref_id: rule.ref_id.clone(),
                    status: "invalid".into(),
                    message: e,
                });
                continue;
            }
            resolved.watch_rules.push(watch);
        }
    }

    if let Some(composites) = template.get("composites").and_then(|v| v.as_array()) {
        for (i, raw) in composites.iter().enumerate() {
            let path = format!("composites[{i}]");
            let def: CompositeJson = match serde_json::from_value(raw.clone()) {
                Ok(c) => c,
                Err(e) => {
                    report.references.push(ReferenceIssue {
                        path,
                        ref_id: None,
                        status: "invalid".into(),
                        message: format!("composite 解析失败: {e}"),
                    });
                    continue;
                }
            };
            if existing_composite_names.iter().any(|n| n == &def.name) || seen_composites.iter().any(|n| n == &def.name)
            {
                report.conflicts.push(Conflict {
                    kind: "duplicate_composite_name".into(),
                    ref_id: def.name.clone(),
                    message: format!("composite 名称 `{}` 已存在", def.name),
                });
                continue;
            }
            let mut steps = Vec::new();
            let mut composite_ok = true;
            for (j, step) in def.steps.iter().enumerate() {
                let (op, ok) = resolve_op(&step.op, mapping, &format!("{path}.steps[{j}]"), &mut report);
                composite_ok &= ok;
                let rollback = match &step.rollback {
                    Some(rb) => {
                        let (op, ok) = resolve_op(rb, mapping, &format!("{path}.steps[{j}].rollback"), &mut report);
                        composite_ok &= ok;
                        op
                    }
                    None => None,
                };
                if let Some(op) = op {
                    steps.push(CompositeStep { op, rollback });
                }
            }
            if !composite_ok || steps.is_empty() {
                continue;
            }
            seen_composites.push(def.name.clone());
            resolved.composites.push(CompositeDef {
                id: format!("cx_{}", uuid::Uuid::new_v4().simple()),
                workspace_id: workspace_id.to_string(),
                name: def.name,
                description: def.description,
                v: COMPOSITE_V,
                steps,
            });
        }
    }

    report.resolved = ResolvedCount {
        watch_rules: resolved.watch_rules.len(),
        composites: resolved.composites.len(),
    };
    report.ok = report.references.is_empty() && report.conflicts.is_empty();
    (resolved, report)
}

fn resolve_op(
    op: &OpJson,
    mapping: &HashMap<String, String>,
    path: &str,
    report: &mut ValidationReport,
) -> (Option<CompositeOp>, bool) {
    let Some(thing_id) = mapping.get(op.thing()).cloned() else {
        report.references.push(ReferenceIssue {
            path: path.to_string(),
            ref_id: Some(op.thing().to_string()),
            status: "missing_device".into(),
            message: format!("composite 引用的物 `{}` 未在映射中提供", op.thing()),
        });
        return (None, false);
    };
    let out = match op {
        OpJson::Call { action, params, .. } => CompositeOp::Call {
            thing_id,
            action: action.clone(),
            params: params.clone(),
        },
        OpJson::Set { properties, .. } => CompositeOp::Set {
            thing_id,
            properties: properties.clone(),
        },
    };
    (Some(out), true)
}

fn build_condition(rule: &RuleJson, thing_id: &str) -> Result<WatchCondition, String> {
    match rule.kind.as_str() {
        "property" => {
            let property = rule.property.clone().ok_or("property 条件缺少 property")?;
            let op = match rule.op.as_deref() {
                Some("lt") => CompareOp::Lt,
                Some("gt") => CompareOp::Gt,
                Some("eq") => CompareOp::Eq,
                Some("ne") => CompareOp::Ne,
                other => return Err(format!("property 条件 op 非法: {other:?}")),
            };
            let value = rule.value.clone().ok_or("property 条件缺少 value")?;
            Ok(WatchCondition::Property {
                thing_id: thing_id.to_string(),
                property,
                op,
                value,
            })
        }
        "health" => {
            let level = match rule.level.as_deref() {
                Some("attention") => HealthLevel::Attention,
                Some("bad") => HealthLevel::Bad,
                other => return Err(format!("health 条件 level 非法: {other:?}")),
            };
            Ok(WatchCondition::Health {
                thing_id: thing_id.to_string(),
                level,
            })
        }
        "availability" => Ok(WatchCondition::Availability {
            thing_id: thing_id.to_string(),
            online: rule.online.ok_or("availability 条件缺少 online")?,
        }),
        other => Err(format!("未知 watch kind: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mapping() -> HashMap<String, String> {
        HashMap::from([
            ("wind".to_string(), "t-wind".to_string()),
            ("window".to_string(), "t-window".to_string()),
        ])
    }

    #[test]
    fn resolves_watch_and_composite_with_refs() {
        let template = json!({
            "watch_rules": [
                {"ref":"wind_high","thing":"wind","kind":"property","property":"风速","op":"gt","value":20,"persistent":true,"timeout_secs":3600}
            ],
            "composites": [
                {"name":"storm_guard","steps":[
                    {"op":"call","thing":"window","action":"close","params":{}},
                    {"op":"call","thing":"window","action":"open","params":{},
                     "rollback":{"op":"call","thing":"window","action":"close","params":{}}}
                ]}
            ]
        });
        let (resolved, report) = resolve_template_rules(&template, "ws", &mapping(), &[]);
        assert!(report.ok, "{report:?}");
        assert_eq!(report.resolved.watch_rules, 1);
        assert_eq!(report.resolved.composites, 1);
        assert_eq!(resolved.watch_rules[0].condition.index_key().0, "t-wind");
        assert_eq!(resolved.composites[0].steps.len(), 2);
        let CompositeOp::Call { thing_id, .. } = &resolved.composites[0].steps[0].op else {
            panic!("call expected");
        };
        assert_eq!(thing_id, "t-window");
    }

    #[test]
    fn missing_ref_rejects_whole_pack() {
        let template = json!({
            "watch_rules": [{"thing":"ghost","kind":"health","level":"bad"}]
        });
        let (resolved, report) = resolve_template_rules(&template, "ws", &mapping(), &[]);
        assert!(!report.ok);
        assert!(resolved.watch_rules.is_empty());
        assert_eq!(report.references[0].status, "missing_device");
    }

    #[test]
    fn duplicate_composite_name_is_a_conflict() {
        let template = json!({
            "composites": [{"name":"guard","steps":[{"op":"call","thing":"window","action":"close"}]}]
        });
        let (_resolved, report) = resolve_template_rules(&template, "ws", &mapping(), &["guard".to_string()]);
        assert!(!report.ok);
        assert_eq!(report.conflicts[0].kind, "duplicate_composite_name");
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let template = json!({
            "watch_rules": [{"thing":"wind","kind":"availability","online":false,"future_field":1}],
            "unknown_top": true
        });
        let (_resolved, report) = resolve_template_rules(&template, "ws", &mapping(), &[]);
        assert!(report.ok, "{report:?}");
    }
}
