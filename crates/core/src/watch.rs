//! E1 watch 条件词表 v1（spec 2026-10-20-e1 §2.3）：property 比较 / health 级别 / availability。
//! 词表版本化（P5）：序列化 payload 带 v=1。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompareOp { Lt, Gt, Eq, Ne }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthLevel { Attention, Bad }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WatchCondition {
    Property { thing_id: String, property: String, op: CompareOp, value: serde_json::Value },
    Health { thing_id: String, level: HealthLevel },
    Availability { thing_id: String, online: bool },
}

impl WatchCondition {
    /// 倒排索引键：条件挂在哪个物的哪个字段上。
    pub fn index_key(&self) -> (String, String) {
        match self {
            Self::Property { thing_id, property, .. } => (thing_id.clone(), property.clone()),
            Self::Health { thing_id, .. } => (thing_id.clone(), "__health".into()),
            Self::Availability { thing_id, .. } => (thing_id.clone(), "__availability".into()),
        }
    }

    /// 纯函数求值：字段新值是否使命题成立。类型不符/缺失一律不命中。
    pub fn matches(&self, thing_id: &str, field: &str, value: &serde_json::Value) -> bool {
        match self {
            Self::Property { thing_id: t, property, op, value: target } => {
                if t != thing_id || property != field { return false; }
                match (value.as_f64(), target.as_f64()) {
                    (Some(a), Some(b)) => match op {
                        CompareOp::Lt => a < b,
                        CompareOp::Gt => a > b,
                        CompareOp::Eq => a == b,
                        CompareOp::Ne => a != b,
                    },
                    _ => value == target && matches!(op, CompareOp::Eq)
                        || value != target && matches!(op, CompareOp::Ne),
                }
            }
            Self::Health { thing_id: t, level } => {
                t == thing_id && field == "__health"
                    && value.as_str() == Some(match level {
                        HealthLevel::Attention => "attention",
                        HealthLevel::Bad => "bad",
                    })
            }
            Self::Availability { thing_id: t, online } => {
                t == thing_id && field == "__availability" && value.as_bool() == Some(*online)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct WatchRule {
    pub id: String,
    pub workspace_id: String,
    pub condition: WatchCondition,
    pub persistent: bool,
    pub timeout_secs: u32,
    pub created_at: String,
}

impl WatchRule {
    /// spec 限额：超时默认 600s、上限 3600s。
    pub const MAX_TIMEOUT_SECS: u32 = 3600;

    pub fn validate(&self) -> Result<(), String> {
        if self.timeout_secs == 0 || self.timeout_secs > Self::MAX_TIMEOUT_SECS {
            return Err(format!("timeout_secs must be 1..={}", Self::MAX_TIMEOUT_SECS));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_condition_compares_numbers_and_rejects_type_mismatch() {
        let c = WatchCondition::Property {
            thing_id: "t1".into(), property: "温度".into(),
            op: CompareOp::Gt, value: serde_json::json!(30.0),
        };
        assert!(c.matches("t1", "温度", &serde_json::json!(31.5)));
        assert!(!c.matches("t1", "温度", &serde_json::json!(28.0)));
        assert!(!c.matches("t1", "温度", &serde_json::json!("热"))); // 类型不符不命中
        assert!(!c.matches("t2", "温度", &serde_json::json!(31.5))); // 他物不命中
    }

    #[test]
    fn property_condition_missing_field_and_ne_semantics() {
        let eq = WatchCondition::Property {
            thing_id: "t1".into(), property: "状态".into(),
            op: CompareOp::Eq, value: serde_json::json!("on"),
        };
        assert!(eq.matches("t1", "状态", &serde_json::json!("on")));
        assert!(!eq.matches("t1", "状态", &serde_json::json!("off")));
        // 缺失字段（字段名不同）一律不命中，即使 Ne
        let ne = WatchCondition::Property {
            thing_id: "t1".into(), property: "状态".into(),
            op: CompareOp::Ne, value: serde_json::json!("on"),
        };
        assert!(ne.matches("t1", "状态", &serde_json::json!("off")));
        assert!(!ne.matches("t1", "状态", &serde_json::json!("on")));
        assert!(!ne.matches("t1", "其他字段", &serde_json::json!("off")));
        // 数值 Ne：不等才命中
        let ne_num = WatchCondition::Property {
            thing_id: "t1".into(), property: "p".into(),
            op: CompareOp::Ne, value: serde_json::json!(1.0),
        };
        assert!(ne_num.matches("t1", "p", &serde_json::json!(2.0)));
        assert!(!ne_num.matches("t1", "p", &serde_json::json!(1.0)));
        // 类型不符时 Eq/Ne 走 JSON 相等语义
        assert!(ne.matches("t1", "状态", &serde_json::json!(42)));
        assert!(!eq.matches("t1", "状态", &serde_json::json!(42)));
    }

    #[test]
    fn property_condition_nan_never_numeric_compares() {
        // serde_json 无法表示 NaN，json!(NaN) 落为 Null → as_f64 为 None → 不进入数值分支
        let gt = WatchCondition::Property {
            thing_id: "t1".into(), property: "p".into(),
            op: CompareOp::Gt, value: serde_json::json!(30.0),
        };
        let nan = serde_json::json!(f64::NAN);
        assert_eq!(nan, serde_json::Value::Null);
        assert!(!gt.matches("t1", "p", &nan)); // Lt/Gt 对非数值不命中
        let lt = WatchCondition::Property {
            thing_id: "t1".into(), property: "p".into(),
            op: CompareOp::Lt, value: serde_json::json!(30.0),
        };
        assert!(!lt.matches("t1", "p", &nan));
    }

    #[test]
    fn health_and_availability_conditions_match_sentinel_fields() {
        let h = WatchCondition::Health { thing_id: "t1".into(), level: HealthLevel::Bad };
        assert!(h.matches("t1", "__health", &serde_json::json!("bad")));
        assert!(!h.matches("t1", "__health", &serde_json::json!("attention")));
        assert!(!h.matches("t1", "__health", &serde_json::json!(1)));
        assert!(!h.matches("t1", "温度", &serde_json::json!("bad"))); // 字段不符
        let a = WatchCondition::Availability { thing_id: "t1".into(), online: false };
        assert!(a.matches("t1", "__availability", &serde_json::json!(false)));
        assert!(!a.matches("t1", "__availability", &serde_json::json!(true)));
        assert!(!a.matches("t1", "__availability", &serde_json::json!("false")));
        assert!(!a.matches("t2", "__availability", &serde_json::json!(false)));
    }

    #[test]
    fn condition_serializes_with_type_tag_and_roundtrips() {
        let c = WatchCondition::Property {
            thing_id: "t1".into(), property: "温度".into(),
            op: CompareOp::Gt, value: serde_json::json!(30.0),
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["type"], "property");
        assert_eq!(v["op"], "gt");
        assert_eq!(serde_json::from_value::<WatchCondition>(v).unwrap(), c);
        let h = WatchCondition::Health { thing_id: "t1".into(), level: HealthLevel::Attention };
        let hv = serde_json::to_value(&h).unwrap();
        assert_eq!(hv["type"], "health");
        assert_eq!(hv["level"], "attention");
        assert_eq!(serde_json::from_value::<WatchCondition>(hv).unwrap(), h);
        let a = WatchCondition::Availability { thing_id: "t1".into(), online: true };
        let av = serde_json::to_value(&a).unwrap();
        assert_eq!(av["type"], "availability");
        assert_eq!(serde_json::from_value::<WatchCondition>(av).unwrap(), a);
    }

    #[test]
    fn index_key_maps_health_and_availability_to_sentinels() {
        let p = WatchCondition::Property {
            thing_id: "t1".into(), property: "温度".into(),
            op: CompareOp::Lt, value: serde_json::json!(0),
        };
        assert_eq!(p.index_key(), ("t1".to_string(), "温度".to_string()));
        let h = WatchCondition::Health { thing_id: "t1".into(), level: HealthLevel::Bad };
        assert_eq!(h.index_key(), ("t1".to_string(), "__health".to_string()));
        let a = WatchCondition::Availability { thing_id: "t1".into(), online: true };
        assert_eq!(a.index_key(), ("t1".to_string(), "__availability".to_string()));
    }

    #[test]
    fn rule_validate_enforces_timeout_range() {
        let base = WatchRule {
            id: "r1".into(), workspace_id: "w1".into(),
            condition: WatchCondition::Availability { thing_id: "t1".into(), online: false },
            persistent: false, timeout_secs: 600, created_at: "2026-10-20 00:00:00".into(),
        };
        assert!(base.validate().is_ok());
        assert!(WatchRule { timeout_secs: 0, ..base.clone() }.validate().is_err());
        assert!(WatchRule { timeout_secs: 1, ..base.clone() }.validate().is_ok());
        assert!(WatchRule { timeout_secs: WatchRule::MAX_TIMEOUT_SECS, ..base.clone() }.validate().is_ok());
        assert!(WatchRule { timeout_secs: WatchRule::MAX_TIMEOUT_SECS + 1, ..base.clone() }.validate().is_err());
    }
}
