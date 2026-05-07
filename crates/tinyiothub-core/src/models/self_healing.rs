// Self-Healing DTO
// DTOs for self-healing API endpoints

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Recovery action DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryActionDto {
    pub action_type: String,
    pub target: String,
    pub max_retries: u32,
    pub cooldown_secs: u64,
}

/// Healing condition DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealingConditionDto {
    pub condition_type: String,
    pub threshold: f64,
    pub count: u32,
}

/// Level policy DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelPolicyDto {
    pub level: String,
    pub actions: Vec<RecoveryActionDto>,
    pub conditions: Vec<HealingConditionDto>,
    pub require_approval: bool,
    pub cooldown_secs: u64,
}

/// Self-healing policy DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfHealingPolicyDto {
    pub enabled: bool,
    pub levels: HashMap<String, LevelPolicyDto>,
}

/// Probe finding DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeFindingDto {
    pub finding_type: String,
    pub severity: String,
    pub message: String,
    pub target: String,
    pub value: Option<String>,
}

/// Probe result DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResultDto {
    pub probe_type: String,
    pub timestamp: DateTime<Utc>,
    pub healthy: bool,
    pub severity: String,
    pub findings: Vec<ProbeFindingDto>,
    pub metadata: HashMap<String, String>,
}

/// Healing execution DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealingExecutionDto {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub action_type: String,
    pub target: String,
    pub result: String,
    pub logs: Vec<String>,
}

/// Probe configuration DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeConfig {
    pub system_probe_enabled: bool,
    pub device_probe_enabled: bool,
    pub task_probe_enabled: bool,
    pub system_probe_interval_secs: u64,
    pub device_probe_interval_secs: u64,
    pub task_probe_interval_secs: u64,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            system_probe_enabled: true,
            device_probe_enabled: true,
            task_probe_enabled: true,
            system_probe_interval_secs: 60,
            device_probe_interval_secs: 300,
            task_probe_interval_secs: 300,
        }
    }
}

/// Request to execute self-healing action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteSelfHealRequest {
    pub level: String,
    pub action_type: String,
    pub target: String,
    pub cooldown_secs: Option<u64>,
}

/// Response after executing self-healing action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteSelfHealResponse {
    pub execution: HealingExecutionDto,
    pub message: String,
}
