use std::sync::Arc;

use super::{
    super::{
        entity::{Alarm, AlarmRule},
        errors::{AlarmError, AlarmResult},
        repository::{AlarmQueryCriteria, AlarmRepository, AlarmRuleRepository, TimeRange},
        specifications::AlarmSpecifications,
        value_objects::{AlarmStatus, ResolutionType},
    },
    rule_engine::RuleEngine,
};

/// 报警业务服务
pub struct AlarmService {
    alarm_repository: Arc<dyn AlarmRepository>,
    rule_repository: Arc<dyn AlarmRuleRepository>,
    rule_engine: Arc<RuleEngine>,
}

impl AlarmService {
    pub fn new(
        alarm_repository: Arc<dyn AlarmRepository>,
        rule_repository: Arc<dyn AlarmRuleRepository>,
    ) -> Self {
        let rule_engine = Arc::new(RuleEngine::new(rule_repository.clone()));

        Self { alarm_repository, rule_repository, rule_engine }
    }

    /// 创建报警
    pub async fn create_alarm(&self, alarm: Alarm) -> AlarmResult<Alarm> {
        self.alarm_repository.create(&alarm).await?;
        Ok(alarm)
    }

    /// 获取报警
    pub async fn get_alarm_by_id(&self, id: &str) -> AlarmResult<Option<Alarm>> {
        self.alarm_repository.find_by_id(id).await
    }

    /// 确认报警
    pub async fn acknowledge_alarm(
        &self,
        alarm_id: &str,
        user_id: String,
        note: Option<String>,
    ) -> AlarmResult<()> {
        let mut alarm = self
            .alarm_repository
            .find_by_id(alarm_id)
            .await?
            .ok_or_else(|| AlarmError::NotFound(alarm_id.to_string()))?;

        if !AlarmSpecifications::can_acknowledge(&alarm) {
            return Err(AlarmError::InvalidStatusTransition {
                from: alarm.status.as_str().to_string(),
                to: AlarmStatus::Acknowledged.as_str().to_string(),
            });
        }

        alarm.acknowledge(user_id, note)?;
        self.alarm_repository.update(&alarm).await?;

        Ok(())
    }

    /// 解决报警
    pub async fn resolve_alarm(
        &self,
        alarm_id: &str,
        user_id: String,
        resolution_type: ResolutionType,
        note: Option<String>,
    ) -> AlarmResult<()> {
        let mut alarm = self
            .alarm_repository
            .find_by_id(alarm_id)
            .await?
            .ok_or_else(|| AlarmError::NotFound(alarm_id.to_string()))?;

        if !AlarmSpecifications::can_resolve(&alarm) {
            return Err(AlarmError::InvalidStatusTransition {
                from: alarm.status.as_str().to_string(),
                to: AlarmStatus::Resolved.as_str().to_string(),
            });
        }

        alarm.resolve(user_id, resolution_type, note)?;
        self.alarm_repository.update(&alarm).await?;

        Ok(())
    }

    /// 批量确认
    pub async fn batch_acknowledge(
        &self,
        alarm_ids: Vec<String>,
        user_id: String,
    ) -> AlarmResult<usize> {
        let mut count = 0;

        for alarm_id in alarm_ids {
            if let Ok(()) = self.acknowledge_alarm(&alarm_id, user_id.clone(), None).await {
                count += 1;
            }
        }

        Ok(count)
    }

    /// 批量解决
    pub async fn batch_resolve(
        &self,
        alarm_ids: Vec<String>,
        user_id: String,
        resolution_type: ResolutionType,
    ) -> AlarmResult<usize> {
        let mut count = 0;

        for alarm_id in alarm_ids {
            if let Ok(()) =
                self.resolve_alarm(&alarm_id, user_id.clone(), resolution_type, None).await
            {
                count += 1;
            }
        }

        Ok(count)
    }

    /// 查询活跃报警
    pub async fn get_active_alarms(&self, device_id: Option<&str>) -> AlarmResult<Vec<Alarm>> {
        self.alarm_repository.find_active(device_id).await
    }

    /// 查询报警历史
    pub async fn get_alarm_history(&self, criteria: AlarmQueryCriteria) -> AlarmResult<Vec<Alarm>> {
        self.alarm_repository.find_by_criteria(&criteria).await
    }

    /// 统计报警数量
    pub async fn count_alarms(&self, criteria: AlarmQueryCriteria) -> AlarmResult<u64> {
        self.alarm_repository.count_by_criteria(&criteria).await
    }

    /// 获取报警统计
    pub async fn get_alarm_statistics(
        &self,
        time_range: TimeRange,
    ) -> AlarmResult<AlarmStatistics> {
        let criteria = AlarmQueryCriteria { time_range: Some(time_range), ..Default::default() };

        let alarms = self.alarm_repository.find_by_criteria(&criteria).await?;

        let total_count = alarms.len() as u64;
        let active_count = alarms.iter().filter(|a| a.status == AlarmStatus::Active).count() as u64;
        let acknowledged_count =
            alarms.iter().filter(|a| a.status == AlarmStatus::Acknowledged).count() as u64;
        let resolved_count =
            alarms.iter().filter(|a| a.status == AlarmStatus::Resolved).count() as u64;

        Ok(AlarmStatistics { total_count, active_count, acknowledged_count, resolved_count })
    }

    /// 检查自动解决
    pub async fn check_auto_resolution(&self) -> AlarmResult<usize> {
        let active_alarms = self.alarm_repository.find_active(None).await?;
        let resolved_count = 0;

        for _alarm in active_alarms {
            // TODO: 实现自动解决逻辑
            // 需要检查当前属性值是否仍然满足报警条件
        }

        Ok(resolved_count)
    }

    // 规则管理方法

    /// 创建规则
    pub async fn create_rule(&self, rule: AlarmRule) -> AlarmResult<AlarmRule> {
        AlarmSpecifications::is_valid_rule(&rule).map_err(AlarmError::InvalidRuleConfig)?;

        self.rule_repository.create(&rule).await?;
        Ok(rule)
    }

    /// 获取规则
    pub async fn get_rule_by_id(&self, id: &str) -> AlarmResult<Option<AlarmRule>> {
        self.rule_repository.find_by_id(id).await
    }

    /// 获取所有规则
    pub async fn get_all_rules(&self) -> AlarmResult<Vec<AlarmRule>> {
        self.rule_repository.find_enabled().await
    }

    /// 根据设备获取规则
    pub async fn get_rules_by_device(&self, device_id: &str) -> AlarmResult<Vec<AlarmRule>> {
        self.rule_repository.find_by_device(device_id).await
    }

    /// 更新规则
    pub async fn update_rule(&self, rule: AlarmRule) -> AlarmResult<()> {
        AlarmSpecifications::is_valid_rule(&rule).map_err(AlarmError::InvalidRuleConfig)?;

        self.rule_repository.update(&rule).await
    }

    /// 删除规则
    pub async fn delete_rule(&self, id: &str) -> AlarmResult<()> {
        self.rule_repository.delete(id).await
    }

    /// 启用/禁用规则
    pub async fn set_rule_enabled(&self, id: &str, enabled: bool) -> AlarmResult<()> {
        self.rule_repository.set_enabled(id, enabled).await
    }

    /// 获取规则引擎
    pub fn rule_engine(&self) -> Arc<RuleEngine> {
        self.rule_engine.clone()
    }
}

/// 报警统计
#[derive(Debug, Clone)]
pub struct AlarmStatistics {
    pub total_count: u64,
    pub active_count: u64,
    pub acknowledged_count: u64,
    pub resolved_count: u64,
}
