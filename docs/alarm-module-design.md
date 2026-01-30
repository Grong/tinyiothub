# 报警模块设计文档 (Alarm Module Design)

## 1. 概述 (Overview)

### 1.1 模块定位
报警模块是 IoT 边缘网关系统的核心业务模块，负责设备和属性的报警规则管理、报警触发判断、报警记录管理和报警通知协调。

### 1.2 设计原则
- **领域驱动设计 (DDD)**: 遵循 DDD 分层架构，业务逻辑集中在领域层
- **事件驱动**: 通过事件总线与其他模块解耦
- **可扩展性**: 支持多种报警规则类型和通知渠道
- **高性能**: 实时报警判断，批量处理优化

### 1.3 核心职责
1. **报警规则管理**: 创建、更新、删除、查询报警规则
2. **报警触发判断**: 根据事件和规则判断是否触发报警
3. **报警记录管理**: 记录报警实例，支持确认和解决
4. **报警通知协调**: 与通知服务集成，触发多渠道通知
5. **报警统计分析**: 提供报警统计和趋势分析

## 2. 架构设计 (Architecture)

### 2.1 模块结构

```
src/domain/alarm/
├── entities/              # 实体
│   ├── alarm.rs          # 报警实例实体
│   └── alarm_rule.rs     # 报警规则实体
├── value_objects/         # 值对象
│   ├── alarm_level.rs    # 报警级别
│   ├── alarm_type.rs     # 报警类型
│   ├── threshold.rs      # 阈值配置
│   └── alarm_status.rs   # 报警状态
├── aggregates/            # 聚合根
│   └── alarm_aggregate.rs # 报警聚合
├── services/              # 领域服务
│   ├── alarm_service.rs  # 报警业务服务
│   └── rule_engine.rs    # 规则引擎
├── specifications/        # 规约
│   └── alarm_specs.rs    # 报警规约
├── repositories/          # 仓储接口
│   ├── alarm_repository.rs      # 报警仓储
│   └── alarm_rule_repository.rs # 规则仓储
├── handlers/              # 事件处理器
│   └── alarm_event_handler.rs   # 报警事件处理器
└── errors.rs              # 错误定义
```


### 2.2 分层架构

```
┌─────────────────────────────────────────────────────────────┐
│                      API Layer (Axum)                        │
│  /api/v1/alarms/*  /api/v1/alarm-rules/*                   │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                   Application Layer                          │
│  AlarmApplicationService (协调多个领域服务)                  │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                     Domain Layer                             │
│  ┌─────────────────┐  ┌──────────────────┐                 │
│  │ AlarmService    │  │ RuleEngine       │                 │
│  │ (业务逻辑)      │  │ (规则判断)       │                 │
│  └─────────────────┘  └──────────────────┘                 │
│  ┌─────────────────┐  ┌──────────────────┐                 │
│  │ Alarm Entity    │  │ AlarmRule Entity │                 │
│  └─────────────────┘  └──────────────────┘                 │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                 Infrastructure Layer                         │
│  ┌──────────────────────┐  ┌─────────────────────┐         │
│  │ AlarmRepositoryImpl  │  │ AlarmEventHandler   │         │
│  │ (SQLite)             │  │ (EventBus)          │         │
│  └──────────────────────┘  └─────────────────────┘         │
└─────────────────────────────────────────────────────────────┘
```

### 2.3 与其他模块的集成

```
┌──────────────┐      事件发布      ┌──────────────┐
│ Device       │ ─────────────────→ │ Event Bus    │
│ Driver       │                     └──────────────┘
└──────────────┘                            │
                                            ↓
                                    ┌──────────────┐
                                    │ Alarm Event  │
                                    │ Handler      │
                                    └──────────────┘
                                            │
                                            ↓
                                    ┌──────────────┐
                                    │ Alarm        │
                                    │ Service      │
                                    └──────────────┘
                                            │
                                    ┌───────┴───────┐
                                    ↓               ↓
                            ┌──────────────┐ ┌──────────────┐
                            │ Notification │ │ Alarm        │
                            │ Service      │ │ Repository   │
                            └──────────────┘ └──────────────┘
```

## 3. 核心领域模型 (Domain Model)

### 3.1 实体 (Entities)

#### 3.1.1 Alarm (报警实例)
```rust
pub struct Alarm {
    id: AlarmId,                    // 报警ID
    device_id: String,              // 设备ID
    property_id: Option<String>,    // 属性ID (可选，设备级报警无此字段)
    rule_id: Option<String>,        // 触发的规则ID
    alarm_type: AlarmType,          // 报警类型
    alarm_level: AlarmLevel,        // 报警级别
    message: String,                // 报警消息
    alarm_value: Option<String>,    // 触发报警的值
    threshold_value: Option<String>,// 阈值
    alarm_time: DateTime<Utc>,      // 报警时间
    status: AlarmStatus,            // 报警状态
    acknowledgement: Option<Acknowledgement>, // 确认信息
    resolution: Option<Resolution>,           // 解决信息
    created_at: DateTime<Utc>,
}
```

#### 3.1.2 AlarmRule (报警规则)
```rust
pub struct AlarmRule {
    id: String,                     // 规则ID
    name: String,                   // 规则名称
    description: Option<String>,    // 规则描述
    device_id: Option<String>,      // 设备ID (可选，全局规则无此字段)
    property_id: Option<String>,    // 属性ID (可选)
    rule_type: RuleType,            // 规则类型
    condition: AlarmCondition,      // 触发条件
    alarm_level: AlarmLevel,        // 报警级别
    is_enabled: bool,               // 是否启用
    notification_config: NotificationConfig, // 通知配置
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```


### 3.2 值对象 (Value Objects)

#### 3.2.1 AlarmLevel (报警级别)
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlarmLevel {
    Info,       // 信息
    Warning,    // 警告
    Error,      // 错误
    Critical,   // 严重
}
```

#### 3.2.2 AlarmType (报警类型)
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlarmType {
    DeviceOffline,      // 设备离线
    DeviceError,        // 设备错误
    PropertyThreshold,  // 属性阈值
    PropertyAnomaly,    // 属性异常
    CommandFailed,      // 命令失败
    Custom(String),     // 自定义类型
}
```

#### 3.2.3 AlarmStatus (报警状态)
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlarmStatus {
    Active,         // 活跃（未确认）
    Acknowledged,   // 已确认（未解决）
    Resolved,       // 已解决
    Suppressed,     // 已抑制
}
```

#### 3.2.4 RuleType (规则类型)
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleType {
    Threshold,      // 阈值规则
    Range,          // 范围规则
    Change,         // 变化规则
    Duration,       // 持续时间规则
    Composite,      // 组合规则
}
```

#### 3.2.5 AlarmCondition (报警条件)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlarmCondition {
    // 阈值条件
    Threshold {
        operator: ComparisonOperator,  // >, <, >=, <=, ==, !=
        value: f64,
    },
    // 范围条件
    Range {
        min: Option<f64>,
        max: Option<f64>,
        inclusive: bool,
    },
    // 变化条件
    Change {
        change_type: ChangeType,  // Increase, Decrease, Any
        threshold: f64,           // 变化幅度
        time_window: Duration,    // 时间窗口
    },
    // 持续时间条件
    Duration {
        condition: Box<AlarmCondition>,
        duration: Duration,
    },
    // 组合条件
    Composite {
        operator: LogicalOperator,  // And, Or, Not
        conditions: Vec<AlarmCondition>,
    },
}
```

#### 3.2.6 Acknowledgement (确认信息)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Acknowledgement {
    acknowledged_by: String,        // 确认人
    acknowledged_at: DateTime<Utc>, // 确认时间
    note: Option<String>,           // 确认备注
}
```

#### 3.2.7 Resolution (解决信息)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    resolved_by: String,            // 解决人
    resolved_at: DateTime<Utc>,     // 解决时间
    note: Option<String>,           // 解决备注
    resolution_type: ResolutionType,// 解决方式
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResolutionType {
    Fixed,          // 已修复
    FalseAlarm,     // 误报
    Ignored,        // 忽略
    AutoResolved,   // 自动解决
}
```

#### 3.2.8 NotificationConfig (通知配置)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    enabled: bool,
    channels: Vec<NotificationChannelType>,
    recipients: Vec<String>,
    suppress_duration: Option<Duration>, // 抑制时间
    repeat_interval: Option<Duration>,   // 重复通知间隔
}
```


### 3.3 聚合根 (Aggregates)

#### AlarmAggregate
```rust
pub struct AlarmAggregate {
    alarm: Alarm,
    related_events: Vec<EventId>,
    notification_records: Vec<NotificationRecord>,
}

impl AlarmAggregate {
    // 创建新报警
    pub fn create(/* ... */) -> Result<Self, AlarmError>;
    
    // 确认报警
    pub fn acknowledge(&mut self, user_id: String, note: Option<String>) -> Result<(), AlarmError>;
    
    // 解决报警
    pub fn resolve(&mut self, user_id: String, resolution_type: ResolutionType, note: Option<String>) -> Result<(), AlarmError>;
    
    // 抑制报警
    pub fn suppress(&mut self, duration: Duration) -> Result<(), AlarmError>;
    
    // 检查是否可以自动解决
    pub fn can_auto_resolve(&self) -> bool;
    
    // 自动解决
    pub fn auto_resolve(&mut self) -> Result<(), AlarmError>;
}
```

### 3.4 领域服务 (Domain Services)

#### 3.4.1 AlarmService (报警业务服务)
```rust
pub struct AlarmService {
    alarm_repository: Arc<dyn AlarmRepository>,
    rule_repository: Arc<dyn AlarmRuleRepository>,
    rule_engine: Arc<RuleEngine>,
    event_bus: Arc<EventBus>,
}

impl AlarmService {
    // 创建报警
    pub async fn create_alarm(&self, alarm: Alarm) -> Result<Alarm, AlarmError>;
    
    // 确认报警
    pub async fn acknowledge_alarm(&self, alarm_id: &str, user_id: String, note: Option<String>) -> Result<(), AlarmError>;
    
    // 解决报警
    pub async fn resolve_alarm(&self, alarm_id: &str, user_id: String, resolution: Resolution) -> Result<(), AlarmError>;
    
    // 批量确认
    pub async fn batch_acknowledge(&self, alarm_ids: Vec<String>, user_id: String) -> Result<usize, AlarmError>;
    
    // 批量解决
    pub async fn batch_resolve(&self, alarm_ids: Vec<String>, user_id: String, resolution_type: ResolutionType) -> Result<usize, AlarmError>;
    
    // 查询活跃报警
    pub async fn get_active_alarms(&self, device_id: Option<String>) -> Result<Vec<Alarm>, AlarmError>;
    
    // 查询报警历史
    pub async fn get_alarm_history(&self, criteria: AlarmQueryCriteria) -> Result<Vec<Alarm>, AlarmError>;
    
    // 获取报警统计
    pub async fn get_alarm_statistics(&self, time_range: TimeRange) -> Result<AlarmStatistics, AlarmError>;
    
    // 检查自动解决
    pub async fn check_auto_resolution(&self) -> Result<usize, AlarmError>;
}
```

#### 3.4.2 RuleEngine (规则引擎)
```rust
pub struct RuleEngine {
    rule_repository: Arc<dyn AlarmRuleRepository>,
}

impl RuleEngine {
    // 评估事件是否触发报警
    pub async fn evaluate_event(&self, event: &Event) -> Result<Vec<AlarmTrigger>, AlarmError>;
    
    // 评估单个规则
    pub fn evaluate_rule(&self, rule: &AlarmRule, event: &Event) -> Result<Option<AlarmTrigger>, AlarmError>;
    
    // 检查条件是否满足
    fn check_condition(&self, condition: &AlarmCondition, value: &PropertyValue, context: &EvaluationContext) -> bool;
    
    // 检查阈值条件
    fn check_threshold(&self, operator: &ComparisonOperator, threshold: f64, value: f64) -> bool;
    
    // 检查范围条件
    fn check_range(&self, min: Option<f64>, max: Option<f64>, value: f64, inclusive: bool) -> bool;
    
    // 检查变化条件
    fn check_change(&self, change_type: &ChangeType, threshold: f64, old_value: f64, new_value: f64) -> bool;
    
    // 检查持续时间条件
    async fn check_duration(&self, condition: &AlarmCondition, duration: Duration, context: &EvaluationContext) -> bool;
    
    // 检查组合条件
    fn check_composite(&self, operator: &LogicalOperator, conditions: &[AlarmCondition], context: &EvaluationContext) -> bool;
}

#[derive(Debug, Clone)]
pub struct AlarmTrigger {
    pub rule_id: String,
    pub rule_name: String,
    pub alarm_level: AlarmLevel,
    pub alarm_type: AlarmType,
    pub message: String,
    pub triggered_value: Option<String>,
    pub threshold_value: Option<String>,
}
```


### 3.5 仓储接口 (Repository Interfaces)

#### 3.5.1 AlarmRepository
```rust
#[async_trait]
pub trait AlarmRepository: Send + Sync {
    // 创建报警
    async fn create(&self, alarm: &Alarm) -> Result<(), AlarmError>;
    
    // 更新报警
    async fn update(&self, alarm: &Alarm) -> Result<(), AlarmError>;
    
    // 根据ID查询
    async fn find_by_id(&self, id: &str) -> Result<Option<Alarm>, AlarmError>;
    
    // 根据条件查询
    async fn find_by_criteria(&self, criteria: AlarmQueryCriteria) -> Result<Vec<Alarm>, AlarmError>;
    
    // 查询活跃报警
    async fn find_active(&self, device_id: Option<String>) -> Result<Vec<Alarm>, AlarmError>;
    
    // 查询未确认报警
    async fn find_unacknowledged(&self, device_id: Option<String>) -> Result<Vec<Alarm>, AlarmError>;
    
    // 统计报警数量
    async fn count_by_criteria(&self, criteria: AlarmQueryCriteria) -> Result<u64, AlarmError>;
    
    // 批量更新状态
    async fn batch_update_status(&self, alarm_ids: Vec<String>, status: AlarmStatus) -> Result<usize, AlarmError>;
    
    // 删除历史报警
    async fn delete_old_alarms(&self, before: DateTime<Utc>) -> Result<usize, AlarmError>;
}

#[derive(Debug, Clone)]
pub struct AlarmQueryCriteria {
    pub device_ids: Option<Vec<String>>,
    pub property_ids: Option<Vec<String>>,
    pub alarm_levels: Option<Vec<AlarmLevel>>,
    pub alarm_types: Option<Vec<AlarmType>>,
    pub statuses: Option<Vec<AlarmStatus>>,
    pub time_range: Option<TimeRange>,
    pub sort_by: Option<String>,
    pub sort_order: Option<SortOrder>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
```

#### 3.5.2 AlarmRuleRepository
```rust
#[async_trait]
pub trait AlarmRuleRepository: Send + Sync {
    // 创建规则
    async fn create(&self, rule: &AlarmRule) -> Result<(), AlarmError>;
    
    // 更新规则
    async fn update(&self, rule: &AlarmRule) -> Result<(), AlarmError>;
    
    // 删除规则
    async fn delete(&self, id: &str) -> Result<(), AlarmError>;
    
    // 根据ID查询
    async fn find_by_id(&self, id: &str) -> Result<Option<AlarmRule>, AlarmError>;
    
    // 查询所有启用的规则
    async fn find_enabled(&self) -> Result<Vec<AlarmRule>, AlarmError>;
    
    // 根据设备查询规则
    async fn find_by_device(&self, device_id: &str) -> Result<Vec<AlarmRule>, AlarmError>;
    
    // 根据属性查询规则
    async fn find_by_property(&self, device_id: &str, property_id: &str) -> Result<Vec<AlarmRule>, AlarmError>;
    
    // 查询全局规则
    async fn find_global_rules(&self) -> Result<Vec<AlarmRule>, AlarmError>;
    
    // 启用/禁用规则
    async fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), AlarmError>;
}
```

### 3.6 规约 (Specifications)

```rust
pub struct AlarmSpecifications;

impl AlarmSpecifications {
    // 检查报警是否可以确认
    pub fn can_acknowledge(alarm: &Alarm) -> bool {
        matches!(alarm.status, AlarmStatus::Active)
    }
    
    // 检查报警是否可以解决
    pub fn can_resolve(alarm: &Alarm) -> bool {
        matches!(alarm.status, AlarmStatus::Active | AlarmStatus::Acknowledged)
    }
    
    // 检查报警是否需要通知
    pub fn should_notify(alarm: &Alarm, rule: &AlarmRule) -> bool {
        rule.notification_config.enabled && 
        !matches!(alarm.status, AlarmStatus::Suppressed)
    }
    
    // 检查报警是否应该被抑制
    pub fn should_suppress(alarm: &Alarm, last_alarm_time: Option<DateTime<Utc>>, suppress_duration: Duration) -> bool {
        if let Some(last_time) = last_alarm_time {
            Utc::now().signed_duration_since(last_time) < suppress_duration
        } else {
            false
        }
    }
    
    // 检查规则是否有效
    pub fn is_valid_rule(rule: &AlarmRule) -> Result<(), String> {
        if rule.name.is_empty() {
            return Err("规则名称不能为空".to_string());
        }
        
        if !rule.notification_config.enabled && rule.notification_config.channels.is_empty() {
            return Err("至少需要配置一个通知渠道".to_string());
        }
        
        Ok(())
    }
}
```


## 4. 事件处理流程 (Event Processing Flow)

### 4.1 报警触发流程

```
1. 设备驱动发布事件
   ↓
2. EventBus 分发事件
   ↓
3. AlarmEventHandler 接收事件 (优先级 50)
   ↓
4. RuleEngine 评估规则
   ├─ 加载相关规则（设备规则 + 全局规则）
   ├─ 逐个评估规则条件
   └─ 返回触发的报警列表
   ↓
5. AlarmService 处理报警
   ├─ 检查报警抑制
   ├─ 创建报警记录
   ├─ 更新设备报警状态
   └─ 发布报警通知事件
   ↓
6. NotificationService 发送通知
   └─ 通过配置的渠道发送通知
```

### 4.2 AlarmEventHandler 实现

```rust
pub struct AlarmEventHandler {
    alarm_service: Arc<AlarmService>,
    rule_engine: Arc<RuleEngine>,
    notification_manager: Arc<NotificationManager>,
}

#[async_trait]
impl EventHandler for AlarmEventHandler {
    async fn handle(&self, event: &Event) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 1. 评估规则，获取触发的报警
        let triggers = self.rule_engine.evaluate_event(event).await?;
        
        if triggers.is_empty() {
            return Ok(());
        }
        
        // 2. 处理每个触发的报警
        for trigger in triggers {
            // 创建报警实例
            let alarm = Alarm::from_trigger(
                event.source().source_id().to_string(),
                event.content().metadata().get("property_id").and_then(|v| v.as_str()).map(String::from),
                trigger,
                event.timestamp().clone(),
            );
            
            // 保存报警
            self.alarm_service.create_alarm(alarm.clone()).await?;
            
            // 触发通知
            if let Some(rule) = self.rule_engine.get_rule(&trigger.rule_id).await? {
                if AlarmSpecifications::should_notify(&alarm, &rule) {
                    self.send_alarm_notification(&alarm, &rule).await?;
                }
            }
        }
        
        Ok(())
    }
    
    fn should_handle(&self, event: &Event) -> bool {
        // 处理设备相关事件
        matches!(event.event_type(), EventType::Device(_))
    }
    
    fn priority(&self) -> u8 {
        50 // 在实时状态更新之后，持久化之前
    }
    
    fn name(&self) -> &str {
        "AlarmEventHandler"
    }
}

impl AlarmEventHandler {
    async fn send_alarm_notification(&self, alarm: &Alarm, rule: &AlarmRule) -> Result<(), AlarmError> {
        let message = NotificationMessage::new(
            format!("报警: {}", alarm.message),
            self.format_alarm_content(alarm),
            alarm.alarm_level.to_notification_level(),
            rule.notification_config.channels.clone(),
            rule.notification_config.recipients.clone(),
        );
        
        for channel in &rule.notification_config.channels {
            if let Err(e) = self.notification_manager.send_notification(&message).await {
                tracing::error!("Failed to send alarm notification via {:?}: {}", channel, e);
            }
        }
        
        Ok(())
    }
    
    fn format_alarm_content(&self, alarm: &Alarm) -> String {
        format!(
            "设备: {}\n级别: {:?}\n消息: {}\n时间: {}",
            alarm.device_id,
            alarm.alarm_level,
            alarm.message,
            alarm.alarm_time.format("%Y-%m-%d %H:%M:%S")
        )
    }
}
```

### 4.3 自动解决流程

```rust
// 在 AlarmService 中实现定期检查
impl AlarmService {
    pub async fn check_auto_resolution(&self) -> Result<usize, AlarmError> {
        // 1. 查询所有活跃报警
        let active_alarms = self.alarm_repository.find_active(None).await?;
        
        let mut resolved_count = 0;
        
        for alarm in active_alarms {
            // 2. 检查对应的属性当前值
            if let Some(property_id) = &alarm.property_id {
                // 获取当前属性值
                let current_value = self.get_current_property_value(&alarm.device_id, property_id).await?;
                
                // 3. 获取触发规则
                if let Some(rule_id) = &alarm.rule_id {
                    if let Some(rule) = self.rule_repository.find_by_id(rule_id).await? {
                        // 4. 检查当前值是否仍然满足报警条件
                        let still_alarming = self.rule_engine.check_condition(
                            &rule.condition,
                            &current_value,
                            &EvaluationContext::default(),
                        );
                        
                        // 5. 如果不再满足条件，自动解决
                        if !still_alarming {
                            self.resolve_alarm(
                                &alarm.id,
                                "system".to_string(),
                                Resolution {
                                    resolved_by: "system".to_string(),
                                    resolved_at: Utc::now(),
                                    note: Some("属性值已恢复正常".to_string()),
                                    resolution_type: ResolutionType::AutoResolved,
                                },
                            ).await?;
                            
                            resolved_count += 1;
                        }
                    }
                }
            }
        }
        
        Ok(resolved_count)
    }
}
```


## 5. API 设计 (API Design)

### 5.1 报警管理 API

#### 5.1.1 查询报警列表
```rust
// GET /api/v1/alarms
async fn list_alarms(
    Query(params): Query<AlarmQueryParams>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<PaginatedResponse<AlarmDto>>> {
    let criteria = AlarmQueryCriteria {
        device_ids: params.device_ids,
        alarm_levels: params.levels,
        statuses: params.statuses,
        time_range: params.time_range,
        limit: params.page_size,
        offset: params.page.map(|p| (p - 1) * params.page_size.unwrap_or(20)),
        ..Default::default()
    };
    
    let alarms = state.alarm_service.get_alarm_history(criteria).await?;
    let total = state.alarm_service.count_alarms(criteria).await?;
    
    ApiResponseBuilder::success(PaginatedResponse {
        data: alarms.into_iter().map(AlarmDto::from).collect(),
        pagination: PaginationInfo::new(params.page.unwrap_or(1), params.page_size.unwrap_or(20), total),
    })
}

#[derive(Deserialize)]
pub struct AlarmQueryParams {
    pub device_ids: Option<Vec<String>>,
    pub levels: Option<Vec<AlarmLevel>>,
    pub statuses: Option<Vec<AlarmStatus>>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}
```

#### 5.1.2 获取报警详情
```rust
// GET /api/v1/alarms/:id
async fn get_alarm(
    Path(id): Path<String>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<AlarmDetailDto>> {
    let alarm = state.alarm_service.get_alarm_by_id(&id).await?
        .ok_or_else(|| AlarmError::NotFound(id.clone()))?;
    
    ApiResponseBuilder::success(AlarmDetailDto::from(alarm))
}
```

#### 5.1.3 确认报警
```rust
// POST /api/v1/alarms/:id/acknowledge
async fn acknowledge_alarm(
    Path(id): Path<String>,
    Json(req): Json<AcknowledgeRequest>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<()>> {
    state.alarm_service.acknowledge_alarm(&id, claims.user_id, req.note).await?;
    ApiResponseBuilder::success(())
}

#[derive(Deserialize)]
pub struct AcknowledgeRequest {
    pub note: Option<String>,
}
```

#### 5.1.4 解决报警
```rust
// POST /api/v1/alarms/:id/resolve
async fn resolve_alarm(
    Path(id): Path<String>,
    Json(req): Json<ResolveRequest>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<()>> {
    let resolution = Resolution {
        resolved_by: claims.user_id,
        resolved_at: Utc::now(),
        note: req.note,
        resolution_type: req.resolution_type,
    };
    
    state.alarm_service.resolve_alarm(&id, claims.user_id, resolution).await?;
    ApiResponseBuilder::success(())
}

#[derive(Deserialize)]
pub struct ResolveRequest {
    pub resolution_type: ResolutionType,
    pub note: Option<String>,
}
```

#### 5.1.5 批量操作
```rust
// POST /api/v1/alarms/batch-acknowledge
async fn batch_acknowledge(
    Json(req): Json<BatchAcknowledgeRequest>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<BatchOperationResult>> {
    let count = state.alarm_service.batch_acknowledge(req.alarm_ids, claims.user_id).await?;
    
    ApiResponseBuilder::success(BatchOperationResult {
        success_count: count,
        total_count: req.alarm_ids.len(),
    })
}

#[derive(Deserialize)]
pub struct BatchAcknowledgeRequest {
    pub alarm_ids: Vec<String>,
}
```

#### 5.1.6 获取报警统计
```rust
// GET /api/v1/alarms/statistics
async fn get_alarm_statistics(
    Query(params): Query<StatisticsQueryParams>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<AlarmStatistics>> {
    let time_range = TimeRange {
        start: params.start_time.unwrap_or_else(|| Utc::now() - Duration::days(7)),
        end: params.end_time.unwrap_or_else(|| Utc::now()),
    };
    
    let stats = state.alarm_service.get_alarm_statistics(time_range).await?;
    ApiResponseBuilder::success(stats)
}

#[derive(Serialize)]
pub struct AlarmStatistics {
    pub total_count: u64,
    pub active_count: u64,
    pub acknowledged_count: u64,
    pub resolved_count: u64,
    pub by_level: HashMap<AlarmLevel, u64>,
    pub by_device: Vec<DeviceAlarmCount>,
    pub trend: Vec<AlarmTrendPoint>,
}
```

### 5.2 报警规则 API

#### 5.2.1 查询规则列表
```rust
// GET /api/v1/alarm-rules
async fn list_alarm_rules(
    Query(params): Query<RuleQueryParams>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<AlarmRuleDto>>> {
    let rules = if let Some(device_id) = params.device_id {
        state.alarm_service.get_rules_by_device(&device_id).await?
    } else {
        state.alarm_service.get_all_rules().await?
    };
    
    ApiResponseBuilder::success(rules.into_iter().map(AlarmRuleDto::from).collect())
}
```

#### 5.2.2 创建规则
```rust
// POST /api/v1/alarm-rules
async fn create_alarm_rule(
    Json(req): Json<CreateAlarmRuleRequest>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<AlarmRuleDto>> {
    let rule = AlarmRule {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        description: req.description,
        device_id: req.device_id,
        property_id: req.property_id,
        rule_type: req.rule_type,
        condition: req.condition,
        alarm_level: req.alarm_level,
        is_enabled: true,
        notification_config: req.notification_config,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    
    state.alarm_service.create_rule(rule.clone()).await?;
    ApiResponseBuilder::success(AlarmRuleDto::from(rule))
}

#[derive(Deserialize)]
pub struct CreateAlarmRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub device_id: Option<String>,
    pub property_id: Option<String>,
    pub rule_type: RuleType,
    pub condition: AlarmCondition,
    pub alarm_level: AlarmLevel,
    pub notification_config: NotificationConfig,
}
```

#### 5.2.3 更新规则
```rust
// PUT /api/v1/alarm-rules/:id
async fn update_alarm_rule(
    Path(id): Path<String>,
    Json(req): Json<UpdateAlarmRuleRequest>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<AlarmRuleDto>> {
    let mut rule = state.alarm_service.get_rule_by_id(&id).await?
        .ok_or_else(|| AlarmError::RuleNotFound(id.clone()))?;
    
    if let Some(name) = req.name {
        rule.name = name;
    }
    if let Some(condition) = req.condition {
        rule.condition = condition;
    }
    if let Some(alarm_level) = req.alarm_level {
        rule.alarm_level = alarm_level;
    }
    
    rule.updated_at = Utc::now();
    
    state.alarm_service.update_rule(rule.clone()).await?;
    ApiResponseBuilder::success(AlarmRuleDto::from(rule))
}
```

#### 5.2.4 删除规则
```rust
// DELETE /api/v1/alarm-rules/:id
async fn delete_alarm_rule(
    Path(id): Path<String>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<()>> {
    state.alarm_service.delete_rule(&id).await?;
    ApiResponseBuilder::success(())
}
```

#### 5.2.5 启用/禁用规则
```rust
// POST /api/v1/alarm-rules/:id/toggle
async fn toggle_alarm_rule(
    Path(id): Path<String>,
    Json(req): Json<ToggleRuleRequest>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<()>> {
    state.alarm_service.set_rule_enabled(&id, req.enabled).await?;
    ApiResponseBuilder::success(())
}

#[derive(Deserialize)]
pub struct ToggleRuleRequest {
    pub enabled: bool,
}
```


## 6. 数据传输对象 (DTOs)

### 6.1 AlarmDto
```rust
#[derive(Serialize, Deserialize)]
pub struct AlarmDto {
    pub id: String,
    pub device_id: String,
    pub device_name: Option<String>,
    pub property_id: Option<String>,
    pub property_name: Option<String>,
    pub rule_id: Option<String>,
    pub rule_name: Option<String>,
    pub alarm_type: String,
    pub alarm_level: String,
    pub message: String,
    pub alarm_value: Option<String>,
    pub threshold_value: Option<String>,
    pub alarm_time: String,
    pub status: String,
    pub is_acknowledged: bool,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<String>,
    pub acknowledged_note: Option<String>,
    pub is_resolved: bool,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<String>,
    pub resolved_note: Option<String>,
    pub created_at: String,
}

impl From<Alarm> for AlarmDto {
    fn from(alarm: Alarm) -> Self {
        Self {
            id: alarm.id.to_string(),
            device_id: alarm.device_id,
            device_name: None, // 需要从设备服务获取
            property_id: alarm.property_id,
            property_name: None, // 需要从设备服务获取
            rule_id: alarm.rule_id,
            rule_name: None, // 需要从规则仓储获取
            alarm_type: format!("{:?}", alarm.alarm_type),
            alarm_level: format!("{:?}", alarm.alarm_level),
            message: alarm.message,
            alarm_value: alarm.alarm_value,
            threshold_value: alarm.threshold_value,
            alarm_time: alarm.alarm_time.to_rfc3339(),
            status: format!("{:?}", alarm.status),
            is_acknowledged: alarm.acknowledgement.is_some(),
            acknowledged_by: alarm.acknowledgement.as_ref().map(|a| a.acknowledged_by.clone()),
            acknowledged_at: alarm.acknowledgement.as_ref().map(|a| a.acknowledged_at.to_rfc3339()),
            acknowledged_note: alarm.acknowledgement.as_ref().and_then(|a| a.note.clone()),
            is_resolved: alarm.resolution.is_some(),
            resolved_by: alarm.resolution.as_ref().map(|r| r.resolved_by.clone()),
            resolved_at: alarm.resolution.as_ref().map(|r| r.resolved_at.to_rfc3339()),
            resolved_note: alarm.resolution.as_ref().and_then(|r| r.note.clone()),
            created_at: alarm.created_at.to_rfc3339(),
        }
    }
}
```

### 6.2 AlarmRuleDto
```rust
#[derive(Serialize, Deserialize)]
pub struct AlarmRuleDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub property_id: Option<String>,
    pub property_name: Option<String>,
    pub rule_type: String,
    pub condition: AlarmConditionDto,
    pub alarm_level: String,
    pub is_enabled: bool,
    pub notification_config: NotificationConfigDto,
    pub created_at: String,
    pub updated_at: String,
}
```

## 7. 基础设施层实现 (Infrastructure)

### 7.1 AlarmRepositoryImpl
```rust
pub struct AlarmRepositoryImpl {
    database: Arc<Database>,
}

#[async_trait]
impl AlarmRepository for AlarmRepositoryImpl {
    async fn create(&self, alarm: &Alarm) -> Result<(), AlarmError> {
        let query = r#"
            INSERT INTO device_alarms (
                id, device_id, property_id, rule_id, alarm_level, 
                alarm_message, alarm_value, threshold_value, alarm_time,
                is_acknowledged, acknowledged_by, acknowledged_at, acknowledged_note,
                is_resolved, resolved_by, resolved_at, resolved_note, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;
        
        sqlx::query(query)
            .bind(&alarm.id.to_string())
            .bind(&alarm.device_id)
            .bind(&alarm.property_id)
            .bind(&alarm.rule_id)
            .bind(format!("{:?}", alarm.alarm_level).to_lowercase())
            .bind(&alarm.message)
            .bind(&alarm.alarm_value)
            .bind(&alarm.threshold_value)
            .bind(alarm.alarm_time.to_rfc3339())
            .bind(alarm.acknowledgement.is_some())
            .bind(alarm.acknowledgement.as_ref().map(|a| &a.acknowledged_by))
            .bind(alarm.acknowledgement.as_ref().map(|a| a.acknowledged_at.to_rfc3339()))
            .bind(alarm.acknowledgement.as_ref().and_then(|a| a.note.as_ref()))
            .bind(alarm.resolution.is_some())
            .bind(alarm.resolution.as_ref().map(|r| &r.resolved_by))
            .bind(alarm.resolution.as_ref().map(|r| r.resolved_at.to_rfc3339()))
            .bind(alarm.resolution.as_ref().and_then(|r| r.note.as_ref()))
            .bind(alarm.created_at.to_rfc3339())
            .execute(self.database.pool())
            .await
            .map_err(|e| AlarmError::DatabaseError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn find_by_criteria(&self, criteria: AlarmQueryCriteria) -> Result<Vec<Alarm>, AlarmError> {
        let mut query = String::from("SELECT * FROM device_alarms WHERE 1=1");
        let mut params: Vec<Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send>> = Vec::new();
        
        if let Some(device_ids) = &criteria.device_ids {
            let placeholders = device_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            query.push_str(&format!(" AND device_id IN ({})", placeholders));
            for id in device_ids {
                params.push(Box::new(id.clone()));
            }
        }
        
        if let Some(levels) = &criteria.alarm_levels {
            let placeholders = levels.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            query.push_str(&format!(" AND alarm_level IN ({})", placeholders));
            for level in levels {
                params.push(Box::new(format!("{:?}", level).to_lowercase()));
            }
        }
        
        // 添加其他过滤条件...
        
        query.push_str(" ORDER BY alarm_time DESC");
        
        if let Some(limit) = criteria.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }
        
        // 执行查询并转换结果
        // ...
        
        Ok(vec![])
    }
    
    async fn find_active(&self, device_id: Option<String>) -> Result<Vec<Alarm>, AlarmError> {
        let criteria = AlarmQueryCriteria {
            device_ids: device_id.map(|id| vec![id]),
            statuses: Some(vec![AlarmStatus::Active, AlarmStatus::Acknowledged]),
            ..Default::default()
        };
        
        self.find_by_criteria(criteria).await
    }
}
```

### 7.2 AlarmRuleRepositoryImpl
```rust
pub struct AlarmRuleRepositoryImpl {
    database: Arc<Database>,
}

#[async_trait]
impl AlarmRuleRepository for AlarmRuleRepositoryImpl {
    async fn create(&self, rule: &AlarmRule) -> Result<(), AlarmError> {
        let query = r#"
            INSERT INTO device_alarm_rules (
                id, name, description, device_id, property_id,
                rule_type, condition, alarm_level, is_enabled,
                notification_config, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;
        
        sqlx::query(query)
            .bind(&rule.id)
            .bind(&rule.name)
            .bind(&rule.description)
            .bind(&rule.device_id)
            .bind(&rule.property_id)
            .bind(format!("{:?}", rule.rule_type))
            .bind(serde_json::to_string(&rule.condition).unwrap())
            .bind(format!("{:?}", rule.alarm_level).to_lowercase())
            .bind(rule.is_enabled)
            .bind(serde_json::to_string(&rule.notification_config).unwrap())
            .bind(rule.created_at.to_rfc3339())
            .bind(rule.updated_at.to_rfc3339())
            .execute(self.database.pool())
            .await
            .map_err(|e| AlarmError::DatabaseError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn find_enabled(&self) -> Result<Vec<AlarmRule>, AlarmError> {
        let query = "SELECT * FROM device_alarm_rules WHERE is_enabled = true";
        
        // 执行查询并转换结果
        // ...
        
        Ok(vec![])
    }
    
    async fn find_by_device(&self, device_id: &str) -> Result<Vec<AlarmRule>, AlarmError> {
        let query = r#"
            SELECT * FROM device_alarm_rules 
            WHERE (device_id = ? OR device_id IS NULL) AND is_enabled = true
        "#;
        
        // 执行查询并转换结果
        // ...
        
        Ok(vec![])
    }
}
```


## 8. 前端集成 (Frontend Integration)

### 8.1 TypeScript 类型定义

```typescript
// web/types/alarm.ts

export interface Alarm {
  id: string
  deviceId: string
  deviceName?: string
  propertyId?: string
  propertyName?: string
  ruleId?: string
  ruleName?: string
  alarmType: string
  alarmLevel: 'info' | 'warning' | 'error' | 'critical'
  message: string
  alarmValue?: string
  thresholdValue?: string
  alarmTime: string
  status: 'active' | 'acknowledged' | 'resolved' | 'suppressed'
  isAcknowledged: boolean
  acknowledgedBy?: string
  acknowledgedAt?: string
  acknowledgedNote?: string
  isResolved: boolean
  resolvedBy?: string
  resolvedAt?: string
  resolvedNote?: string
  createdAt: string
}

export interface AlarmRule {
  id: string
  name: string
  description?: string
  deviceId?: string
  deviceName?: string
  propertyId?: string
  propertyName?: string
  ruleType: 'threshold' | 'range' | 'change' | 'duration' | 'composite'
  condition: AlarmCondition
  alarmLevel: 'info' | 'warning' | 'error' | 'critical'
  isEnabled: boolean
  notificationConfig: NotificationConfig
  createdAt: string
  updatedAt: string
}

export interface AlarmCondition {
  type: 'threshold' | 'range' | 'change' | 'duration' | 'composite'
  // 根据类型不同，包含不同的字段
  operator?: string
  value?: number
  min?: number
  max?: number
  // ...
}

export interface AlarmStatistics {
  totalCount: number
  activeCount: number
  acknowledgedCount: number
  resolvedCount: number
  byLevel: Record<string, number>
  byDevice: Array<{ deviceId: string; deviceName: string; count: number }>
  trend: Array<{ time: string; count: number }>
}
```

### 8.2 Service 层实现

```typescript
// web/service/alarms.ts
import { apiGet, apiPost, apiPut, apiDelete } from '@/lib/api-client'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'
import type { Alarm, AlarmRule, AlarmStatistics } from '@/types/alarm'

// API 调用函数
export const alarmApi = {
  // 获取报警列表
  getAlarms: (params?: {
    deviceIds?: string[]
    levels?: string[]
    statuses?: string[]
    startTime?: string
    endTime?: string
    page?: number
    pageSize?: number
  }) => apiGet<{ data: Alarm[]; pagination: any }>('alarms', params),
  
  // 获取报警详情
  getAlarm: (id: string) => apiGet<Alarm>(`alarms/${id}`),
  
  // 确认报警
  acknowledgeAlarm: (id: string, note?: string) => 
    apiPost<void>(`alarms/${id}/acknowledge`, { note }),
  
  // 解决报警
  resolveAlarm: (id: string, data: { resolutionType: string; note?: string }) =>
    apiPost<void>(`alarms/${id}/resolve`, data),
  
  // 批量确认
  batchAcknowledge: (alarmIds: string[]) =>
    apiPost<{ successCount: number; totalCount: number }>('alarms/batch-acknowledge', { alarmIds }),
  
  // 获取统计
  getStatistics: (params?: { startTime?: string; endTime?: string }) =>
    apiGet<AlarmStatistics>('alarms/statistics', params),
  
  // 规则管理
  getRules: (deviceId?: string) => 
    apiGet<AlarmRule[]>('alarm-rules', deviceId ? { deviceId } : undefined),
  
  createRule: (data: Partial<AlarmRule>) => 
    apiPost<AlarmRule>('alarm-rules', data),
  
  updateRule: (id: string, data: Partial<AlarmRule>) =>
    apiPut<AlarmRule>(`alarm-rules/${id}`, data),
  
  deleteRule: (id: string) => 
    apiDelete<void>(`alarm-rules/${id}`),
  
  toggleRule: (id: string, enabled: boolean) =>
    apiPost<void>(`alarm-rules/${id}/toggle`, { enabled }),
}

// React Query Hooks
export const useAlarms = (params?: Parameters<typeof alarmApi.getAlarms>[0]) => {
  return useQuery({
    queryKey: queryKeys.alarms.list(params || {}),
    queryFn: async () => {
      const response = await alarmApi.getAlarms(params)
      return response.result
    },
  })
}

export const useAlarm = (id: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.alarms.detail(id),
    queryFn: async () => {
      const response = await alarmApi.getAlarm(id)
      return response.result
    },
    enabled: enabled && !!id,
  })
}

export const useAcknowledgeAlarm = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ id, note }: { id: string; note?: string }) =>
      alarmApi.acknowledgeAlarm(id, note),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.all })
    },
  })
}

export const useResolveAlarm = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: any }) =>
      alarmApi.resolveAlarm(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.all })
    },
  })
}

export const useAlarmStatistics = (params?: Parameters<typeof alarmApi.getStatistics>[0]) => {
  return useQuery({
    queryKey: queryKeys.alarms.statistics(params || {}),
    queryFn: async () => {
      const response = await alarmApi.getStatistics(params)
      return response.result
    },
  })
}

export const useAlarmRules = (deviceId?: string) => {
  return useQuery({
    queryKey: queryKeys.alarmRules.list(deviceId ? { deviceId } : {}),
    queryFn: async () => {
      const response = await alarmApi.getRules(deviceId)
      return response.result || []
    },
  })
}

export const useCreateAlarmRule = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: alarmApi.createRule,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.alarmRules.all })
    },
  })
}
```

### 8.3 Query Keys 定义

```typescript
// web/lib/query-keys.ts (添加到现有文件)

export const queryKeys = {
  // ... 现有的 keys
  
  alarms: {
    all: ['alarms'] as const,
    lists: () => [...queryKeys.alarms.all, 'list'] as const,
    list: (params: Record<string, any>) => [...queryKeys.alarms.lists(), params] as const,
    details: () => [...queryKeys.alarms.all, 'detail'] as const,
    detail: (id: string) => [...queryKeys.alarms.details(), id] as const,
    statistics: (params: Record<string, any>) => [...queryKeys.alarms.all, 'statistics', params] as const,
  },
  
  alarmRules: {
    all: ['alarm-rules'] as const,
    lists: () => [...queryKeys.alarmRules.all, 'list'] as const,
    list: (params: Record<string, any>) => [...queryKeys.alarmRules.lists(), params] as const,
    details: () => [...queryKeys.alarmRules.all, 'detail'] as const,
    detail: (id: string) => [...queryKeys.alarmRules.details(), id] as const,
  },
}
```


## 9. 实现计划 (Implementation Plan)

### 9.1 第一阶段：核心领域模型 (Phase 1: Core Domain Model)

**目标**: 建立报警模块的核心领域模型和业务逻辑

**任务清单**:
1. 创建值对象
   - [ ] `alarm_level.rs` - 报警级别
   - [ ] `alarm_type.rs` - 报警类型
   - [ ] `alarm_status.rs` - 报警状态
   - [ ] `threshold.rs` - 阈值配置
   - [ ] `alarm_condition.rs` - 报警条件

2. 创建实体
   - [ ] `alarm.rs` - 报警实例实体
   - [ ] `alarm_rule.rs` - 报警规则实体

3. 创建聚合根
   - [ ] `alarm_aggregate.rs` - 报警聚合

4. 定义仓储接口
   - [ ] `alarm_repository.rs` - 报警仓储接口
   - [ ] `alarm_rule_repository.rs` - 规则仓储接口

5. 定义错误类型
   - [ ] `errors.rs` - 报警模块错误定义

### 9.2 第二阶段：领域服务 (Phase 2: Domain Services)

**目标**: 实现核心业务逻辑

**任务清单**:
1. 规则引擎
   - [ ] `rule_engine.rs` - 规则评估引擎
   - [ ] 实现阈值条件检查
   - [ ] 实现范围条件检查
   - [ ] 实现变化条件检查
   - [ ] 实现持续时间条件检查
   - [ ] 实现组合条件检查

2. 报警服务
   - [ ] `alarm_service.rs` - 报警业务服务
   - [ ] 实现报警创建逻辑
   - [ ] 实现报警确认逻辑
   - [ ] 实现报警解决逻辑
   - [ ] 实现批量操作
   - [ ] 实现自动解决检查

3. 规约
   - [ ] `alarm_specs.rs` - 报警规约

### 9.3 第三阶段：基础设施层 (Phase 3: Infrastructure)

**目标**: 实现数据持久化和事件处理

**任务清单**:
1. 仓储实现
   - [ ] `alarm_repository_impl.rs` - 报警仓储实现
   - [ ] `alarm_rule_repository_impl.rs` - 规则仓储实现
   - [ ] 实现 CRUD 操作
   - [ ] 实现复杂查询
   - [ ] 实现批量操作

2. 事件处理器
   - [ ] 更新 `alarm_event_handler.rs` - 完善报警事件处理器
   - [ ] 集成规则引擎
   - [ ] 集成通知服务
   - [ ] 实现报警抑制逻辑

3. 数据库迁移
   - [ ] 验证现有表结构是否满足需求
   - [ ] 如需要，创建新的迁移文件

### 9.4 第四阶段：API 层 (Phase 4: API Layer)

**目标**: 提供 REST API 接口

**任务清单**:
1. 报警管理 API
   - [ ] `src/api/alarms/mod.rs` - 模块定义
   - [ ] `src/api/alarms/query.rs` - 查询接口
   - [ ] `src/api/alarms/management.rs` - 管理接口
   - [ ] `src/api/alarms/statistics.rs` - 统计接口

2. 规则管理 API
   - [ ] `src/api/alarm_rules/mod.rs` - 模块定义
   - [ ] `src/api/alarm_rules/crud.rs` - CRUD 接口
   - [ ] `src/api/alarm_rules/management.rs` - 管理接口

3. DTOs
   - [ ] `src/dto/entity/alarm.rs` - 报警 DTO
   - [ ] `src/dto/entity/alarm_rule.rs` - 规则 DTO
   - [ ] `src/dto/request/alarm.rs` - 请求 DTO
   - [ ] `src/dto/response/alarm.rs` - 响应 DTO

4. 路由注册
   - [ ] 在 `src/api/mod.rs` 中注册路由

### 9.5 第五阶段：前端集成 (Phase 5: Frontend Integration)

**目标**: 实现前端报警管理界面

**任务清单**:
1. 类型定义
   - [ ] `web/types/alarm.ts` - TypeScript 类型定义

2. Service 层
   - [ ] `web/service/alarms.ts` - 报警服务
   - [ ] 实现 API 调用函数
   - [ ] 实现 React Query hooks

3. 组件开发
   - [ ] `web/app/components/alarm/alarm-list.tsx` - 报警列表
   - [ ] `web/app/components/alarm/alarm-detail.tsx` - 报警详情
   - [ ] `web/app/components/alarm/alarm-statistics.tsx` - 报警统计
   - [ ] `web/app/components/alarm/alarm-rule-list.tsx` - 规则列表
   - [ ] `web/app/components/alarm/alarm-rule-form.tsx` - 规则表单

4. 页面开发
   - [ ] `web/app/(commonLayout)/alarms/page.tsx` - 报警管理页面
   - [ ] `web/app/(commonLayout)/alarm-rules/page.tsx` - 规则管理页面

### 9.6 第六阶段：测试和优化 (Phase 6: Testing & Optimization)

**任务清单**:
1. 单元测试
   - [ ] 领域模型测试
   - [ ] 规则引擎测试
   - [ ] 服务层测试

2. 集成测试
   - [ ] API 端点测试
   - [ ] 事件处理测试
   - [ ] 端到端测试

3. 性能优化
   - [ ] 数据库查询优化
   - [ ] 规则评估性能优化
   - [ ] 批量操作优化

4. 文档完善
   - [ ] API 文档
   - [ ] 使用手册
   - [ ] 开发文档

## 10. 技术考虑 (Technical Considerations)

### 10.1 性能优化

1. **规则缓存**: 将启用的规则缓存在内存中，避免每次事件都查询数据库
2. **批量处理**: 报警创建和通知发送支持批量操作
3. **索引优化**: 在常用查询字段上创建索引
4. **异步处理**: 报警通知异步发送，不阻塞主流程

### 10.2 可扩展性

1. **规则类型扩展**: 通过枚举和 trait 支持新的规则类型
2. **条件组合**: 支持复杂的条件组合逻辑
3. **通知渠道**: 与现有通知服务集成，支持多种通知渠道
4. **自定义报警类型**: 支持用户定义的报警类型

### 10.3 可靠性

1. **事务处理**: 关键操作使用数据库事务
2. **错误处理**: 完善的错误处理和日志记录
3. **重试机制**: 通知发送失败时的重试机制
4. **数据一致性**: 确保报警状态与实际设备状态一致

### 10.4 安全性

1. **权限控制**: 报警操作需要适当的权限
2. **审计日志**: 记录报警确认和解决操作
3. **数据隔离**: 多租户环境下的数据隔离
4. **输入验证**: 严格的输入验证和清理

## 11. 集成点 (Integration Points)

### 11.1 与事件系统集成
- 通过 EventBus 接收设备事件
- 发布报警通知事件

### 11.2 与通知服务集成
- 使用 NotificationManager 发送通知
- 支持多种通知渠道（Email, SMS, SSE, Webhook）

### 11.3 与设备服务集成
- 获取设备和属性信息
- 查询当前属性值用于自动解决

### 11.4 与用户服务集成
- 获取用户信息用于确认和解决记录
- 权限验证

## 12. 配置示例 (Configuration Examples)

### 12.1 报警规则配置示例

```json
{
  "name": "温度过高报警",
  "description": "当温度超过80度时触发警告",
  "device_id": "device-001",
  "property_id": "temperature",
  "rule_type": "Threshold",
  "condition": {
    "type": "threshold",
    "operator": ">",
    "value": 80.0
  },
  "alarm_level": "warning",
  "notification_config": {
    "enabled": true,
    "channels": ["email", "sms"],
    "recipients": ["admin@example.com", "+1234567890"],
    "suppress_duration": "PT5M",
    "repeat_interval": "PT1H"
  }
}
```

### 12.2 组合规则示例

```json
{
  "name": "温度和湿度异常",
  "rule_type": "Composite",
  "condition": {
    "type": "composite",
    "operator": "And",
    "conditions": [
      {
        "type": "threshold",
        "property": "temperature",
        "operator": ">",
        "value": 80.0
      },
      {
        "type": "threshold",
        "property": "humidity",
        "operator": ">",
        "value": 90.0
      }
    ]
  },
  "alarm_level": "critical"
}
```

