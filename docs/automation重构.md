# TinyIoTHub 自动化规则系统重构方案

> 日期：2026-03-15
> 目标：整合 Jobs 和规则引擎为统一的自动化系统

---

## 一、重构目标

### 1.1 现有模块

| 模块 | 触发方式 | 功能 |
|------|----------|------|
| Jobs | Cron 定时 | 定时执行任务 |
| 规则引擎（待开发） | 事件触发 | 条件触发自动化 |

### 1.2 重构后

统一为 **Automation**（自动化规则）：

| 触发类型 | 说明 | 示例 |
|----------|------|------|
| `event` | 事件触发 | 温度>50℃→告警 |
| `cron` | 定时触发 | 每天8点→开设备 |
| `manual` | 手动触发 | 立即执行 |

---

## 二、数据模型设计

### 2.1 数据库表

```sql
-- 自动化规则表
CREATE TABLE automations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    
    -- 触发类型: event | cron | manual
    trigger_type TEXT NOT NULL DEFAULT 'event',
    
    -- 事件触发配置
    event_source_type TEXT,  -- device | alarm | system
    event_device_id TEXT,
    event_property TEXT,
    event_condition TEXT,    -- JSON: {operator, value}
    
    -- 定时触发配置
    cron_expression TEXT,
    
    -- 条件配置（事件触发时）
    conditions TEXT,          -- JSON: 条件链
    
    -- 动作配置
    actions TEXT NOT NULL,   -- JSON: [{type, config}]
    
    -- 执行配置
    timeout_seconds INTEGER DEFAULT 30,
    retry_count INTEGER DEFAULT 0,
    retry_delay_seconds INTEGER DEFAULT 5,
    cooldown_seconds INTEGER DEFAULT 0,
    
    -- 元数据
    priority INTEGER DEFAULT 100,
    enabled INTEGER DEFAULT 1,
    
    -- 统计
    run_count INTEGER DEFAULT 0,
    success_count INTEGER DEFAULT 0,
    fail_count INTEGER DEFAULT 0,
    last_run_at TEXT,
    last_run_status TEXT,
    last_run_error TEXT,
    
    -- 标签
    tags TEXT,
    
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    created_by TEXT
);

-- 执行历史表
CREATE TABLE automation_executions (
    id TEXT PRIMARY KEY,
    automation_id TEXT NOT NULL,
    trigger_type TEXT NOT NULL,
    trigger_data TEXT,        -- 触发时的上下文数据
    
    conditions_met INTEGER,    -- 条件是否满足
    conditions_result TEXT,     -- 条件评估详情
    
    actions_executed TEXT,      -- 执行的动作
    success INTEGER NOT NULL,
    error_message TEXT,
    
    execution_time_ms INTEGER,
    triggered_by TEXT,
    triggered_at TEXT NOT NULL,
    
    FOREIGN KEY (automation_id) REFERENCES automations(id)
);

-- 索引
CREATE INDEX idx_automations_trigger ON automations(trigger_type, enabled);
CREATE INDEX idx_executions_automation ON automation_executions(automation_id);
CREATE INDEX idx_executions_time ON automation_executions(triggered_at);
```

### 2.2 Rust 结构体

```rust
// 自动化规则
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Automation {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    
    // 触发类型
    pub trigger_type: TriggerType,
    
    // 事件触发配置
    pub event_source_type: Option<EventSourceType>,
    pub event_device_id: Option<String>,
    pub event_property: Option<String>,
    pub event_condition: Option<String>,  // JSON
    
    // 定时触发配置
    pub cron_expression: Option<String>,
    
    // 条件配置
    pub conditions: Option<String>,  // JSON
    
    // 动作配置
    pub actions: String,  // JSON
    
    // 执行配置
    pub timeout_seconds: i32,
    pub retry_count: i32,
    pub retry_delay_seconds: i32,
    pub cooldown_seconds: i32,
    
    // 元数据
    pub priority: i32,
    pub enabled: bool,
    
    // 统计
    pub run_count: i64,
    pub success_count: i64,
    pub fail_count: i64,
    pub last_run_at: Option<String>,
    pub last_run_status: Option<String>,
    pub last_run_error: Option<String>,
    
    // 标签
    pub tags: Option<String>,
    
    pub created_at: String,
    pub updated_at: String,
    pub created_by: Option<String>,
}

// 触发类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    Event,   // 事件触发
    Cron,    // 定时触发
    Manual,  // 手动触发
}

// 事件源类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSourceType {
    Device,    // 设备数据变化
    Alarm,     // 告警事件
    System,    // 系统事件
}

// 条件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum Condition {
    Threshold {
        property: String,
        operator: Operator,
        value: f64,
    },
    DeviceState {
        state: DeviceState,
    },
    Comparison {
        property: String,
        operator: Operator,
        value: serde_json::Value,
    },
    And {
        left: Box<Condition>,
        right: Box<Condition>,
    },
    Or {
        left: Box<Condition>,
        right: Box<Condition>,
    },
}

// 操作符
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Eq,      // ==
    Neq,     // !=
    Gt,      // >
    Gte,     // >=
    Lt,      // <
    Lte,     // <=
    Contains,// 包含
    StartsWith,
    EndsWith,
}

// 设备状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    Online,
    Offline,
    Warning,
    Error,
}

// 动作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum Action {
    // 发送告警
    Alarm {
        level: AlarmLevel,
        message: String,
    },
    // 控制设备
    ControlDevice {
        device_id: String,
        command: String,
        parameters: Option<HashMap<String, String>>,
    },
    // 设置属性
    SetProperty {
        device_id: String,
        property: String,
        value: String,
    },
    // 发送通知
    Notify {
        channel: NotifyChannel,
        template: String,
    },
    // HTTP 请求
    HttpRequest {
        method: String,
        url: String,
        headers: Option<HashMap<String, String>>,
        body: Option<String>,
    },
    // 延迟
    Delay {
        duration_ms: u64,
    },
    // 执行脚本
    Script {
        interpreter: String,
        script: String,
    },
    // 关闭设备
    PowerOff {
        device_id: String,
    },
    // 打开设备
    PowerOn {
        device_id: String,
    },
}

// 告警级别
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlarmLevel {
    Info,
    Warning,
    Error,
    Critical,
}

// 通知渠道
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyChannel {
    Email,
    Sms,
    Webhook,
    Mqtt,
}
```

---

## 三、API 设计

### 3.1 API 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | /api/v1/automations | 列表 |
| POST | /api/v1/automations | 创建 |
| GET | /api/v1/automations/{id} | 详情 |
| PUT | /api/v1/automations/{id} | 更新 |
| DELETE | /api/v1/automations/{id} | 删除 |
| POST | /api/v1/automations/{id}/enable | 启用 |
| POST | /api/v1/automations/{id}/disable | 禁用 |
| POST | /api/v1/automations/{id}/run | 手动执行 |
| GET | /api/v1/automations/{id}/executions | 执行历史 |
| POST | /api/v1/automations/{id}/test | 测试条件 |
| GET | /api/v1/automations/statistics | 统计 |

### 3.2 创建示例

```bash
POST /api/v1/automations
Content-Type: application/json

{
  "name": "温度过高告警",
  "description": "温度超过50℃时发送告警",
  "trigger_type": "event",
  "event_source_type": "device",
  "event_device_id": "temp_sensor_001",
  "event_property": "temperature",
  "conditions": {
    "type": "threshold",
    "property": "temperature",
    "operator": ">",
    "value": 50
  },
  "actions": [
    {
      "type": "alarm",
      "level": "warning",
      "message": "温度超过阈值，当前温度 {{temperature}}℃"
    }
  ],
  "priority": 10,
  "cooldown_seconds": 300
}
```

### 3.3 定时任务示例

```bash
POST /api/v1/automations
Content-Type: application/json

{
  "name": "每天8点开启空调",
  "trigger_type": "cron",
  "cron_expression": "0 8 * * *",
  "actions": [
    {
      "type": "control_device",
      "device_id": "ac_001",
      "command": "power_on"
    }
  ]
}
```

---

## 四、执行引擎设计

### 4.1 事件处理流程

```
设备数据上报
      │
      ▼
┌──────────────────┐
│  EventListener   │
│  (事件监听器)    │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  查找匹配的自动化  │
│ (按触发类型筛选)  │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  条件评估器       │
│ ConditionEvaluator│
└────────┬─────────┘
         │
    ┌────┴────┐
    │ 满足?   │
    └────┬────┘
      是 │ 否
         ▼
┌──────────────────┐
│  冷却检查         │
└────────┬─────────┘
         │
    ┌────┴────┐
    │ 冷却中? │
    └────┬────┘
      否 │ 是
         ▼
┌──────────────────┐
│  动作执行器       │
│  ActionExecutor  │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  记录执行历史     │
└──────────────────┘
```

### 4.2 核心模块

```rust
// 自动化管理器
pub struct AutomationManager {
    automations: Vec<Automation>,
    executor: ActionExecutor,
    evaluator: ConditionEvaluator,
}

// 条件评估器
pub struct ConditionEvaluator;

impl ConditionEvaluator {
    pub fn evaluate(&self, condition: &Condition, context: &TriggerContext) -> bool {
        match condition {
            Condition::Threshold { property, operator, value } => {
                let current = context.get_property(property);
                self.compare(current, operator, *value)
            }
            Condition::DeviceState { state } => {
                context.device_state == *state
            }
            Condition::And(left, right) => {
                self.evaluate(left, context) && self.evaluate(right, context)
            }
            Condition::Or(left, right) => {
                self.evaluate(left, context) || self.evaluate(right, context)
            }
            _ => false,
        }
    }
}

// 动作执行器
pub struct ActionExecutor {
    alarm_service: Arc<AlarmService>,
    device_service: Arc<DeviceService>,
    http_client: Client,
}

impl ActionExecutor {
    pub async fn execute(&self, actions: &[Action], context: &TriggerContext) -> Result<()> {
        for action in actions {
            match action {
                Action::Alarm { level, message } => {
                    let msg = self.render_template(message, context);
                    self.alarm_service.create_alarm(level, &msg).await?;
                }
                Action::ControlDevice { device_id, command, params } => {
                    self.device_service.execute(device_id, command, params.clone()).await?;
                }
                Action::HttpRequest { method, url, .. } => {
                    self.execute_http(method, url).await?;
                }
                // ... 其他动作
            }
        }
        Ok(())
    }
}
```

---

## 五、实施步骤

### 5.1 阶段一：数据库和 DTO（Day 1）

- [ ] 创建 `automation` 表
- [ ] 创建 `automation_execution` 表
- [ ] 定义 Rust 结构体
- [ ] 实现基础的 CRUD

### 5.2 阶段二：执行引擎（Day 2-3）

- [ ] 实现 ConditionEvaluator
- [ ] 实现 ActionExecutor
- [ ] 实现事件监听集成

### 5.3 阶段三：定时调度（Day 4）

- [ ] 复用现有 Jobs 的 Cron 调度
- [ ] 定时任务触发逻辑

### 5.4 阶段四：API 和测试（Day 5）

- [ ] 完善 API
- [ ] 单元测试
- [ ] 集成测试

---

## 六、向后兼容

### 6.1 Jobs 迁移

现有的 Jobs 可以通过以下方式迁移：

1. **直接迁移**：自动转换旧的 Job 到新的 Automation
2. **保留双轨**：同时支持 Jobs API 和 Automation API
3. **推荐**：保留 Jobs API 作为兼容，新功能使用 Automation

### 6.2 迁移脚本

```sql
-- 将 Jobs 转换为 Automation
INSERT INTO automations (
    id, name, description, trigger_type, cron_expression,
    actions, timeout_seconds, retry_count, enabled,
    run_count, success_count, fail_count,
    created_at, updated_at
)
SELECT 
    id, name, description, 'cron', cron_expression,
    -- 将 config JSON 转换为 actions 格式
    json_object('type', 'http', 'url', json_extract(config, '$.url')),
    timeout_seconds, retry_count, is_enabled,
    run_count, success_count, fail_count,
    created_at, updated_at
FROM jobs;
```

---

## 七、总结

| 特性 | 说明 |
|------|------|
| **统一入口** | `/api/v1/automations` |
| **多种触发** | 事件/Cron/手动 |
| **灵活条件** | 阈值/状态/组合 |
| **丰富动作** | 告警/控制/通知/Http |
| **向后兼容** | 保留 Jobs API |
