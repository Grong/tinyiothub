use std::sync::RwLock;

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

use crate::infrastructure::persistence::database as db_util;

lazy_static! {
    static ref RULE: RwLock<Vec<AlarmRule>> = RwLock::new(vec![]);
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuleMatchResult {
    Normal,        // Normal state
    Alarm,         // Alarm state
    ExceedMaximum, // Exceeds maximum value
    BelowMinimum,  // Below minimum value
}

impl RuleMatchResult {
    pub fn to_code(&self) -> i32 {
        match self {
            RuleMatchResult::Normal => 0,
            RuleMatchResult::Alarm => 1,
            RuleMatchResult::ExceedMaximum => 2,
            RuleMatchResult::BelowMinimum => 3,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AlarmRule {
    pub property_id: String,
    pub data_type: String,
    pub level: u32,
    pub rule_name: String,
    pub min: Option<f64>,
    pub min_restore: Option<f64>,
    pub min_msg: Option<String>,
    pub max: Option<f64>,
    pub max_restore: Option<f64>,
    pub max_msg: Option<String>,
}

impl AlarmRule {
    pub fn load_all() -> Vec<Self> {
        // TODO: Convert to SQLx query
        // let sql = r#"SELECT PropertyId,DeviceAlarmRules.Name,Level,LowerLimit,LowerRestore,LowerLimitMessage,UpperLimit,UpperRestore,UpperLimitMessage,DataType from DeviceAlarms LEFT JOIN DeviceAlarmRules  ON  DeviceAlarms.RuleId=DeviceAlarmRules.Id LEFT JOIN DeviceProperties ON DeviceAlarms.PropertyId=DeviceProperties.Id"#;
        vec![]
    }

    pub fn if_match(&self, value: &str) -> RuleMatchResult {
        match &self.data_type as &str {
            "int" | "double" | "float" => {
                let val = match value.parse::<f64>() {
                    Ok(x) => x,
                    Err(_) => return RuleMatchResult::Normal,
                };
                if let Some(min) = self.min.clone() {
                    if min >= val {
                        return RuleMatchResult::BelowMinimum;
                    }
                }
                if let Some(max) = self.max.clone() {
                    if max <= val {
                        return RuleMatchResult::ExceedMaximum;
                    }
                }
                RuleMatchResult::Normal
            }
            "enum" => {
                let val = match value.parse::<f64>() {
                    Ok(x) => x,
                    Err(_) => return RuleMatchResult::Normal,
                };
                if let Some(min) = self.min.clone() {
                    if min == val {
                        return RuleMatchResult::Alarm;
                    }
                }
                RuleMatchResult::Normal
            }
            _ => RuleMatchResult::Normal,
        }
    }

    pub fn if_restore(&self, value: &str) -> bool {
        match &self.data_type as &str {
            "int" | "double" | "float" => {
                let val = match value.parse::<f64>() {
                    Ok(x) => x,
                    Err(_) => return false,
                };
                let mut min_rst = true;
                if let Some(min) = self.min_restore.clone() {
                    min_rst = min < val;
                }
                let mut max_rst = true;
                if let Some(max) = self.max_restore.clone() {
                    max_rst = max > val;
                }
                min_rst && max_rst
            }
            "enum" => {
                let val = match value.parse::<f64>() {
                    Ok(x) => x,
                    Err(_) => return false,
                };
                let mut min_rst = true;
                if let Some(min) = self.min.clone() {
                    min_rst = min != val;
                }
                min_rst
            }
            _ => true,
        }
    }
}

pub fn reload() {
    let all = AlarmRule::load_all();
    match RULE.write() {
        Ok(mut rules) => {
            *rules = all;
        }
        Err(e) => {
            tracing::error!("Failed to acquire write lock for alarm rules: {}", e);
        }
    }
}

pub fn get_rules(prop_id: &str) -> Vec<AlarmRule> {
    let rules = match RULE.read() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to acquire read lock for alarm rules: {}", e);
            return Vec::new();
        }
    };
    let mut rst = rules
        .clone()
        .into_iter()
        .filter(|r| r.property_id == prop_id.to_string())
        .collect::<Vec<AlarmRule>>();
    rst.sort_by_key(|r| r.level);
    rst
}
