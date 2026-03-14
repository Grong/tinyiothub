# 事件处理器架构设计

## 架构分层原则

事件处理器按照 DDD（领域驱动设计）原则分为两层：

### 1. 基础设施层 Handler（技术关注点）
**位置**: `src/infrastructure/event/handlers/`

这些 Handler 处理技术实现细节，不包含业务逻辑：

- **PersistenceEventHandler** (优先级 90)
  - 职责：将事件持久化到数据库
  - 特性：批量写入（100条/批，5秒刷新）、智能过滤
  - 过滤规则：
    - Debug 级别的属性变化不持久化
    - Info 级别的属性变化只持久化有报警的
    - Warning 及以上级别总是持久化

- **RealTimeStatusHandler** (优先级 10)
  - 职责：更新内存中的实时事件状态
  - 用途：快速查询当前活跃事件

- **SseEventHandler** (优先级 1)
  - 职责：通过 SSE 推送事件到前端
  - 特性：实时性最高，优先级最高

### 2. 领域层 Handler（业务逻辑）
**位置**: `src/domain/*/handlers/`

这些 Handler 包含业务规则和领域逻辑：

- **AlarmEventHandler** (优先级 50)
  - 位置：`src/domain/alarm/handlers/alarm_event_handler.rs`
  - 职责：
    - 根据业务规则判断是否触发报警
    - 创建报警记录
    - 更新设备报警状态
    - 触发报警通知流程
  - 业务规则示例：
    - Warning/Error/Critical 级别触发报警
    - 根据属性配置的报警阈值判断
    - 报警去重和聚合

## 事件处理流程

```
Driver 发布事件
    ↓
EventBus.publish(event)
    ↓
按优先级调用 Handlers：
    ↓
1. SseEventHandler (优先级 1)
   → 实时推送到前端
    ↓
10. RealTimeStatusHandler (优先级 10)
    → 更新内存状态
    ↓
50. AlarmEventHandler (优先级 50)
    → 业务逻辑：报警判断和处理
    ↓
90. PersistenceEventHandler (优先级 90)
    → 持久化到数据库
```

## 如何添加新的业务 Handler

### 步骤 1: 创建 Handler 文件

在对应的领域模块下创建 handler：

```rust
// src/domain/your_domain/handlers/your_handler.rs
use crate::domain::event::entities::Event;
use crate::infrastructure::event::EventHandler;
use std::sync::Arc;

pub struct YourBusinessHandler {
    // 注入需要的服务
    database: Arc<Database>,
}

impl YourBusinessHandler {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

#[async_trait::async_trait]
impl EventHandler for YourBusinessHandler {
    async fn handle(&self, event: &Event) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 实现业务逻辑
        Ok(())
    }
    
    fn name(&self) -> &str {
        "YourBusinessHandler"
    }
    
    fn should_handle(&self, event: &Event) -> bool {
        // 判断是否处理此事件
        true
    }
    
    fn priority(&self) -> u8 {
        // 设置优先级（1-100）
        50
    }
}
```

### 步骤 2: 导出 Handler

```rust
// src/domain/your_domain/handlers/mod.rs
mod your_handler;
pub use your_handler::YourBusinessHandler;

// src/domain/your_domain/mod.rs
pub mod handlers;
pub use handlers::YourBusinessHandler;
```

### 步骤 3: 注册到 EventBus

在 `src/shared/app_state.rs` 中注册：

```rust
// 在 AppState::new() 方法中
tokio::spawn({
    let event_bus = event_bus.clone();
    let database_for_handler = database.clone();
    
    async move {
        // ... 其他 handlers
        
        // 注册你的业务 handler
        event_bus.register_handler(Arc::new(
            YourBusinessHandler::new(database_for_handler)
        )).await;
    }
});
```

## 优先级设计指南

- **1-10**: 实时性要求最高的技术操作（SSE 推送、状态更新）
- **11-50**: 业务逻辑处理（报警、通知、规则引擎）
- **51-90**: 次要业务逻辑（统计、分析）
- **91-100**: 持久化和归档操作

## 示例：属性报警 Handler

当前已实现的 `AlarmEventHandler` 展示了如何处理属性变化事件：

```rust
// 1. 检查业务规则
async fn check_alarm_rules(&self, event: &Event) -> Option<AlarmInfo> {
    // 只处理设备属性事件
    if !matches!(event.event_type(), EventType::Device(DeviceEventType::Property)) {
        return None;
    }
    
    // 根据事件级别判断
    match event.level() {
        EventLevel::Warning | EventLevel::Error | EventLevel::Critical => {
            Some(AlarmInfo { /* ... */ })
        }
        _ => None,
    }
}

// 2. 执行业务操作
async fn process_alarm(&self, alarm: AlarmInfo) -> Result<()> {
    // 创建报警记录
    self.create_alarm_record(&alarm).await?;
    
    // 更新设备状态
    self.update_device_alarm_status(&alarm).await?;
    
    // 触发通知
    self.trigger_alarm_notification(&alarm).await?;
    
    Ok(())
}
```

## 注意事项

1. **基础设施层 Handler 不应包含业务逻辑**
   - ❌ 错误：在 PersistenceEventHandler 中判断报警规则
   - ✅ 正确：在 AlarmEventHandler 中判断报警规则

2. **领域层 Handler 应该独立于技术实现**
   - ❌ 错误：在 AlarmEventHandler 中直接操作 SSE 连接
   - ✅ 正确：通过发布新事件让 SseEventHandler 处理

3. **Handler 应该是无状态的**
   - 所有状态应该存储在数据库或通过事件传递
   - Handler 实例可以被多次创建和销毁

4. **优先级设置要合理**
   - 考虑依赖关系：如果 B 依赖 A 的结果，A 的优先级应该更高
   - 考虑性能：耗时操作应该有较低的优先级

## 测试建议

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_alarm_handler_triggers_on_warning() {
        // 创建测试事件
        let event = create_test_event(EventLevel::Warning);
        
        // 创建 handler
        let handler = AlarmEventHandler::new(test_database());
        
        // 验证应该处理
        assert!(handler.should_handle(&event));
        
        // 执行处理
        let result = handler.handle(&event).await;
        assert!(result.is_ok());
        
        // 验证报警记录已创建
        // ...
    }
}
```

## 扩展建议

可以考虑添加的其他业务 Handler：

- **NotificationEventHandler**: 处理通知规则和发送
- **MetricsEventHandler**: 收集和聚合指标数据
- **AuditEventHandler**: 记录审计日志
- **WorkflowEventHandler**: 触发工作流引擎
- **IntegrationEventHandler**: 与外部系统集成
