//! Alarm trigger — defines when an alarm should fire.
//!
//! TODO: Migrate logic from `cloud/src/domain/alarm/`.

/// Trigger condition for an alarm.
#[derive(Debug, Clone)]
pub struct AlarmTrigger {
    pub alarm_id: String,
    pub rule_expression: String,
    pub severity: AlarmSeverity,
}

/// Alarm severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlarmSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}

impl Default for AlarmSeverity {
    fn default() -> Self {
        Self::Warning
    }
}
