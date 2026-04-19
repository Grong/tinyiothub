//! Alarm manager — tracks active alarms and handles acknowledgment.
//!
//! TODO: Migrate logic from `cloud/src/domain/alarm/`.

use std::collections::HashMap;

use crate::alarm::trigger::{AlarmSeverity, AlarmTrigger};

/// Active alarm instance.
#[derive(Debug, Clone)]
pub struct ActiveAlarm {
    pub id: String,
    pub trigger: AlarmTrigger,
    pub triggered_at: String,
    pub acknowledged: bool,
}

/// Manages the lifecycle of alarms.
#[derive(Debug, Default)]
pub struct AlarmManager {
    active: HashMap<String, ActiveAlarm>,
}

impl AlarmManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trigger(&mut self, alarm: ActiveAlarm) {
        self.active.insert(alarm.id.clone(), alarm);
    }

    pub fn acknowledge(&mut self, alarm_id: &str) -> bool {
        self.active
            .get_mut(alarm_id)
            .map(|a| {
                a.acknowledged = true;
                true
            })
            .unwrap_or(false)
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    pub fn active_by_severity(&self, severity: AlarmSeverity) -> Vec<&ActiveAlarm> {
        self.active
            .values()
            .filter(|a| a.trigger.severity == severity)
            .collect()
    }
}
