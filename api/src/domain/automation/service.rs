//! 自动化规则服务
//! 
//! 自动化规则的核心服务层（简化版）

use std::sync::Arc;
use chrono::Utc;
use serde_json::{json, Value};
use tokio::sync::RwLock;
use uuid::Uuid;

use super::action::{Action, ActionResult};
use super::condition::{Condition, TriggerContext, TriggerType};
use crate::domain::automation::evaluator::ConditionEvaluator;
use crate::domain::automation::executor::ActionExecutor;
use crate::application::data_server::DataServer;
use crate::domain::event::services::notification_service::NotificationManager;

/// 自动化规则服务
pub struct AutomationService {
    evaluator: ConditionEvaluator,
    executor: ActionExecutor,
    automations: Arc<RwLock<Vec<Automation>>>,
}

impl AutomationService {
    pub fn new() -> Self {
        Self {
            evaluator: ConditionEvaluator::new(),
            executor: ActionExecutor::new(),
            automations: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 使用 AppState 依赖创建服务
    pub fn with_dependencies(
        data_server: Option<Arc<DataServer>>,
        notification_manager: Option<Arc<NotificationManager>>,
    ) -> Self {
        let executor = ActionExecutor::new();
        let executor = match (data_server, notification_manager) {
            (Some(ds), Some(nm)) => executor.with_data_server(ds).with_notification_manager(nm),
            (Some(ds), None) => executor.with_data_server(ds),
            (None, Some(nm)) => executor.with_notification_manager(nm),
            (None, None) => executor,
        };
        Self {
            evaluator: ConditionEvaluator::new(),
            executor,
            automations: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// 触发自动化规则（事件触发）
    pub async fn trigger(&self, context: TriggerContext) -> Vec<AutomationExecution> {
        let automations = self.automations.read().await;
        let mut executions = Vec::new();
        
        for automation in automations.iter() {
            if automation.trigger_type != "event" || !automation.enabled {
                continue;
            }
            
            // 评估条件
            let conditions_met = if let Some(conditions_json) = &automation.conditions {
                if let Ok(conditions) = serde_json::from_str::<Condition>(conditions_json) {
                    self.evaluator.evaluate(&conditions, &context)
                } else {
                    true
                }
            } else {
                true
            };
            
            if !conditions_met {
                continue;
            }
            
            // 执行动作
            let actions: Vec<Action> = serde_json::from_str(&automation.actions).unwrap_or_default();
            let results = self.executor.execute(&actions, &context).await;
            
            let success = results.iter().all(|r| r.success);
            
            let execution = AutomationExecution {
                id: Uuid::new_v4().to_string(),
                automation_id: automation.id.clone(),
                automation_name: automation.name.clone(),
                trigger_type: TriggerType::Event,
                conditions_met,
                actions_results: results,
                success,
                triggered_at: Utc::now().to_rfc3339(),
            };
            
            executions.push(execution);
        }
        
        executions
    }
    
    /// 测试条件
    pub fn test_condition(&self, condition_json: &str, mock_data: Value) -> (bool, Value) {
        let mut context = TriggerContext::new();
        
        if let Some(device_id) = mock_data.get("device_id").and_then(|v| v.as_str()) {
            context.device_id = Some(device_id.to_string());
        }
        
        if let Some(props) = mock_data.get("properties").and_then(|v| v.as_object()) {
            for (k, v) in props {
                context.properties.insert(k.clone(), v.clone());
            }
        }
        
        if let Ok(condition) = serde_json::from_str::<Condition>(condition_json) {
            let result = self.evaluator.evaluate_with_details(&condition, &context);
            (result.0, result.1)
        } else {
            (false, json!({"error": "Invalid condition JSON"}))
        }
    }
    
    /// 获取所有自动化规则
    pub async fn list(&self) -> Vec<Automation> {
        self.automations.read().await.clone()
    }
}

/// 自动化规则
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Automation {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub trigger_type: String,
    pub event_source_type: Option<String>,
    pub event_device_id: Option<String>,
    pub event_property: Option<String>,
    pub event_condition: Option<String>,
    pub cron_expression: Option<String>,
    pub conditions: Option<String>,
    pub actions: String,
    pub timeout_seconds: i32,
    pub retry_count: i32,
    pub retry_delay_seconds: i32,
    pub cooldown_seconds: i32,
    pub priority: i32,
    pub enabled: bool,
    pub run_count: i64,
    pub success_count: i64,
    pub fail_count: i64,
    pub last_run_at: Option<String>,
    pub last_run_status: Option<String>,
    pub last_run_error: Option<String>,
    pub tags: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: Option<String>,
}

/// 自动化执行记录
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AutomationExecution {
    pub id: String,
    pub automation_id: String,
    pub automation_name: String,
    pub trigger_type: TriggerType,
    pub conditions_met: bool,
    pub actions_results: Vec<ActionResult>,
    pub success: bool,
    pub triggered_at: String,
}

impl Default for AutomationService {
    fn default() -> Self {
        Self::new()
    }
}
