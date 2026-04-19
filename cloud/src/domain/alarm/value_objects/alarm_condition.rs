use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 报警条件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlarmCondition {
    /// 阈值条件
    Threshold { operator: ComparisonOperator, value: f64 },
    /// 范围条件
    Range { min: Option<f64>, max: Option<f64>, inclusive: bool },
    /// 变化条件
    Change {
        change_type: ChangeType,
        threshold: f64,
        #[serde(with = "duration_serde")]
        time_window: Duration,
    },
    /// 持续时间条件
    Duration {
        condition: Box<AlarmCondition>,
        #[serde(with = "duration_serde")]
        duration: Duration,
    },
    /// 组合条件
    Composite { operator: LogicalOperator, conditions: Vec<AlarmCondition> },
}

/// 比较运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOperator {
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    Equal,
    NotEqual,
}

impl ComparisonOperator {
    pub fn evaluate(&self, left: f64, right: f64) -> bool {
        match self {
            ComparisonOperator::GreaterThan => left > right,
            ComparisonOperator::LessThan => left < right,
            ComparisonOperator::GreaterThanOrEqual => left >= right,
            ComparisonOperator::LessThanOrEqual => left <= right,
            ComparisonOperator::Equal => (left - right).abs() < f64::EPSILON,
            ComparisonOperator::NotEqual => (left - right).abs() >= f64::EPSILON,
        }
    }
}

/// 变化类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Increase,
    Decrease,
    Any,
}

/// 逻辑运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogicalOperator {
    And,
    Or,
    Not,
}

// Duration 序列化辅助模块
mod duration_serde {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}
