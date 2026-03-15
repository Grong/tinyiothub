# TinyIoTHub 规则引擎设计方案

> 日期：2026-03-15
> 目标：为 TinyIoTHub 设计轻量级规则引擎

---

## 一、需求分析

### 1.1 规则引擎价值

| 场景 | 示例 | 价值 |
|------|------|------|
| **自动告警** | 温度>50℃ → 发送告警 | 实时监控 |
| **设备联动** | 门窗开 → 亮灯 | 自动化 |
| **数据转发** | 湿度>70% → 转发到云端 | 数据流转 |
| **定时任务** | 每天8点 → 开启设备 | 定时控制 |
| **阈值控制** | 光照<100lux → 打开窗帘 | 智能控制 |

### 1.2 核心需求

- ✅ 可视化配置（无需编码）
- ✅ 多种触发条件（阈值、时间、设备状态）
- ✅ 多种执行动作（告警、控制、转发）
- ✅ 规则启用/禁用/调试
- ✅ 规则优先级
- ✅ 规则历史记录

---

## 二、架构设计

### 2.1 整体架构

```
┌─────────────────────────────────────────────────────────────────────┐
│                        规则引擎架构                                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐          │
│  │   Web UI    │    │   REST API  │    │   MCP API   │          │
│  │  (前端页面)  │    │  (管理接口)  │    │  (AI调用)   │          │
│  └──────┬───────┘    └──────┬───────┘    └──────┬───────┘          │
│         │                   │                   │                  │
│         └───────────────────┼───────────────────┘                  │
│                             ▼                                      │
│                    ┌──────────────────┐                            │
│                    │   规则管理器      │                            │
│                    │  RuleManager     │                            │
│                    └────────┬─────────┘                            │
│                             │                                       │
│         ┌───────────────────┼───────────────────┐                   │
│         ▼                   ▼                   ▼                   │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐        │
│  │  条件评估器   │    │   定时调度器  │    │  事件监听器   │        │
│  │ Condition    │    │   Scheduler  │    │   Listener   │        │
│  │  Evaluator   │    │              │    │              │        │
│  └──────┬───────┘    └──────┬───────┘    └──────┬───────┘        │
│         │                   │                   │                  │
│         └───────────────────┼───────────────────┘                  │
│                             ▼                                      │
│                    ┌──────────────────┐                            │
│                    │   执行引擎        │                            │
│                    │  ActionExecutor  │                            │
│                    └────────┬─────────┘                            │
│                             │                                       │
│         ┌───────────────────┼───────────────────┐                   │
│         ▼                   ▼                   ▼                   │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐        │
│  │   告警动作    │    │   控制动作    │    │   转发动作    │        │
│  │   Alarm      │    │   Control    │    │   Forward    │        │
│  └──────────────┘    └──────────────┘    └──────────────┘        │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 核心模块

```rust
// 规则管理器
pub struct RuleManager {
    rules: Vec<Rule>,
    scheduler: Scheduler,
    event_bus: Arc<EventBus>,
}

// 规则定义
pub struct Rule {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub priority: i32,              // 优先级，数字越小越高
    pub conditions: Vec<Condition>, // 触发条件
    pub actions: Vec<Action>,       // 执行动作
    pub cooldown: Option<u64>,      // 冷却时间（秒）
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

// 触发条件
pub enum Condition {
    // 阈值条件
    Threshold {
        device_id: String,
        property: String,
        operator: Operator,  // >, <, >=, <=, ==, !=
        value: f64,
    },
    // 设备状态条件
    DeviceState {
        device_id: String,
        state: DeviceState,  // online, offline
    },
    // 时间条件
    Time {
        cron: String,        // Cron 表达式
    },
    // 复合条件
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
}

// 执行动作
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
        parameters: HashMap<String, String>,
    },
    // 设置属性
    SetProperty {
        device_id: String,
        property: String,
        value: String,
    },
    // 转发数据
    Forward {
        endpoint: String,
        format: DataFormat,  // json, csv
    },
    // 发送通知
    Notify {
        channel: NotifyChannel,  // email, webhook, mqtt
        template: String,
    },
    // 延迟执行
    Delay {
        duration_ms: u64,
        actions: Vec<Action>,
    },
}
```

---

## 三、规则类型设计

### 3.1 阈值告警规则

```json
{
  "type": "threshold_alarm",
  "name": "温度过高告警",
  "conditions": {
    "type": "threshold",
    "device_id": "temp_sensor_001",
    "property": "temperature",
    "operator": ">",
    "value": 50
  },
  "actions": [
    {
      "type": "alarm",
      "level": "warning",
      "message": "温度超过50℃，当前温度 {{temperature}}℃"
    },
    {
      "type": "notify",
      "channel": "webhook",
      "url": "https://example.com/alert"
    }
  ]
}
```

### 3.2 设备联动规则

```json
{
  "type": "device_linkage",
  "name": "开门亮灯",
  "conditions": {
    "type": "threshold",
    "device_id": "door_sensor_001",
    "property": "status",
    "operator": "==",
    "value": 1
  },
  "actions": [
    {
      "type": "control",
      "device_id": "light_001",
      "command": "turn_on"
    }
  ],
  "cooldown": 30
}
```

### 3.3 定时任务规则

```json
{
  "type": "schedule",
  "name": "每天8点开启设备",
  "conditions": {
    "type": "time",
    "cron": "0 8 * * *"
  },
  "actions": [
    {
      "type": "control",
      "device_id": "ac_001",
      "command": "power_on"
    }
  ]
}
```

### 3.4 数据转发规则

```json
{
  "type": "forward",
  "name": "数据转发到云端",
  "conditions": {
    "type": "threshold",
    "device_id": "sensor_001",
    "property": "*",
    "operator": "always",
    "interval": 60
  },
  "actions": [
    {
      "type": "forward",
      "endpoint": "https://cloud.example.com/api/data",
      "format": "json",
      "headers": {
        "Authorization": "Bearer xxx"
      }
    }
  ]
}
```

---

## 四、数据模型设计

### 4.1 规则表

```sql
-- 规则定义表
CREATE TABLE rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    enabled INTEGER DEFAULT 1,
    priority INTEGER DEFAULT 100,
    conditions TEXT NOT NULL,      -- JSON 存储条件
    actions TEXT NOT NULL,         -- JSON 存储动作
    cooldown_seconds INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 规则执行历史
CREATE TABLE rule_executions (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL,
    triggered_at TEXT NOT NULL,
    conditions_met TEXT NOT NULL,  -- 满足条件的结果
    actions_executed TEXT NOT NULL, -- 执行的动作
    success INTEGER NOT NULL,
    error_message TEXT,
    FOREIGN KEY (rule_id) REFERENCES rules(id)
);

-- 规则触发日志
CREATE TABLE rule_triggers (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL,
    device_id TEXT,
    property TEXT,
    old_value TEXT,
    new_value TEXT,
    triggered_at TEXT NOT NULL,
    FOREIGN KEY (rule_id) REFERENCES rules(id)
);
```

### 4.2 DTO 定义

```rust
// 创建规则请求
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub conditions: ConditionDto,
    pub actions: Vec<ActionDto>,
    pub cooldown_seconds: Option<u64>,
}

// 条件 DTO
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum ConditionDto {
    Threshold {
        device_id: String,
        property: String,
        operator: String,
        value: f64,
    },
    DeviceState {
        device_id: String,
        state: String,
    },
    Time {
        cron: String,
    },
    And {
        left: Box<ConditionDto>,
        right: Box<ConditionDto>,
    },
    Or {
        left: Box<ConditionDto>,
        right: Box<ConditionDto>,
    },
}

// 动作 DTO
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum ActionDto {
    Alarm {
        level: String,
        message: String,
    },
    ControlDevice {
        device_id: String,
        command: String,
        parameters: Option<HashMap<String, String>>,
    },
    SetProperty {
        device_id: String,
        property: String,
        value: String,
    },
    Forward {
        endpoint: String,
        format: String,
        headers: Option<HashMap<String, String>>,
    },
    Notify {
        channel: String,
        template: String,
    },
    Delay {
        duration_ms: u64,
        actions: Vec<ActionDto>,
    },
}
```

---

## 五、API 设计

### 5.1 规则管理 API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | /api/v1/rules | 获取规则列表 |
| GET | /api/v1/rules/{id} | 获取规则详情 |
| POST | /api/v1/rules | 创建规则 |
| PUT | /api/v1/rules/{id} | 更新规则 |
| DELETE | /api/v1/rules/{id} | 删除规则 |
| POST | /api/v1/rules/{id}/enable | 启用规则 |
| POST | /api/v1/rules/{id}/disable | 禁用规则 |
| POST | /api/v1/rules/{id}/test | 测试规则 |
| GET | /api/v1/rules/{id}/executions | 获取执行历史 |
| GET | /api/v1/rules/triggers | 获取触发日志 |

### 5.2 API 示例

**创建规则**

```bash
POST /api/v1/rules
Content-Type: application/json

{
  "name": "温度过高告警",
  "description": "温度超过50℃时发送告警",
  "priority": 10,
  "conditions": {
    "type": "threshold",
    "device_id": "temp_sensor_001",
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
  "cooldown_seconds": 300
}
```

**响应**

```json
{
  "msg": "",
  "code": 0,
  "result": {
    "id": "rule_001",
    "name": "温度过高告警",
    "enabled": true,
    "priority": 10,
    "created_at": "2026-03-15 12:00:00"
  }
}
```

**测试规则**

```bash
POST /api/v1/rules/{id}/test
Content-Type: application/json

{
  "mock_data": {
    "temperature": 55
  }
}
```

**响应**

```json
{
  "msg": "",
  "code": 0,
  "result": {
    "matched": true,
    "actions_would_execute": [
      {
        "type": "alarm",
        "level": "warning",
        "message": "温度超过阈值，当前温度 55℃"
      }
    ],
    "execution_time_ms": 15
  }
}
```

---

## 六、执行流程

### 6.1 事件驱动流程

```
设备数据上报
    │
    ▼
┌─────────────────┐
│  EventListener  │
│  (事件监听器)   │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  条件匹配      │
│  (遍历规则)    │
└────────┬────────┘
         │
         ▼
    ┌────────┐
    │ 命中?  │──否──▶ 结束
    └────┬───┘
         │是
         ▼
┌─────────────────┐
│  冷却检查      │
│  (cooldown)    │
└────────┬────────┘
         │
         ▼
    ┌────────┐
    │ 冷却中?│──是──▶ 结束
    └────┬───┘
         │否
         ▼
┌─────────────────┐
│  执行动作      │
│  (并行/串行)   │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  记录执行日志  │
└─────────────────┘
```

### 6.2 定时任务流程

```
定时触发 (Cron)
    │
    ▼
┌─────────────────┐
│   Scheduler     │
│  (定时调度器)   │
└────────┬────────┘
         │
         ▼
    (同事件驱动流程)
```

---

## 七、前端界面设计

### 7.1 规则列表页

```
┌─────────────────────────────────────────────────────────────┐
│  规则引擎                                          [+新建规则] │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  🔍 搜索规则...                    筛选: [全部 ▼] [启用 ▼]  │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ 📋 温度过高告警                    ✅ 优先级: 10    │   │
│  │    温度 > 50℃ → 发送告警           最后更新: 10分钟前│   │
│  │                                            [编辑][禁用]│   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ 📋 开门亮灯                         ✅ 优先级: 20    │   │
│  │    门磁开 → 开灯                    最后更新: 1小时前 │   │
│  │                                            [编辑][禁用]│   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 7.2 规则配置页

```
┌─────────────────────────────────────────────────────────────┐
│  编辑规则                                       [保存] [测试]│
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  规则名称: [温度过高告警                                    ]│
│  描述:     [温度超过阈值时发送告警                         ]│
│                                                             │
│  ────────────────── 触发条件 ──────────────────            │
│                                                             │
│  当 [设备: 温度传感器1号 ▼] 的 [温度 ▼] 属性                │
│  [大于 ▼] [50] ℃                                          │
│                                                             │
│  [+添加条件]  [且/或]                                       │
│                                                             │
│  ────────────────── 执行动作 ──────────────────             │
│                                                             │
│  [+添加动作]                                                │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ 🔔 发送告警                                             │   │
│  │    级别: [警告 ▼]                                       │   │
│  │    消息: 温度超过阈值，当前温度 {{temperature}}℃       │   │
│  │                                              [删除]    │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ────────────────── 高级设置 ──────────────────             │
│                                                             │
│  优先级: [10]      冷却时间: [300] 秒                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 八、实施计划

> ⚠️ **注意**：定时任务功能已实现（Jobs API），无需重复开发！

### 8.1 已有功能

**Jobs API** (`/api/v1/jobs`)

| 功能 | 状态 | 说明 |
|------|------|------|
| Cron 调度 | ✅ | 支持标准 Cron 表达式 |
| HTTP 任务 | ✅ | 调用 HTTP 接口 |
| 脚本任务 | ✅ | Python/PowerShell/Bash |
| 设备命令 | ✅ | 触发设备动作 |
| SQL 任务 | ✅ | 执行 SQL（待完善） |
| 执行历史 | ✅ | 记录每次执行结果 |
| 手动触发 | ✅ | 立即执行 |
| 启用/禁用 | ✅ | 动态控制 |

### 8.2 待开发功能

| 功能 | 优先级 | 说明 |
|------|--------|------|
| 规则 CRUD | P0 | 条件+动作组合 |
| 阈值条件 | P0 | 数据触发 |
| 设备联动 | P0 | 状态触发 |
| 告警动作 | P0 | 发送告警 |
| 控制动作 | P0 | 设备控制 |
| 数据转发 | P1 | 转发到云端 |
| 通知渠道 | P1 | 邮件/Webhook |

### 8.3 阶段一：基础框架（Week 1）

- [x] ~~规则数据模型~~ (可复用 Jobs 表或新建)
- [ ] 规则 CRUD API
- [ ] 条件评估器
- [ ] 动作执行器
- [ ] 基础 Web 页面

### 8.4 阶段二：条件引擎（Week 2）

- [ ] 阈值条件评估
- [ ] 设备状态条件
- [ ] 条件组合（AND/OR）
- [ ] 事件监听集成（复用现有 EventBus）

### 8.5 阶段三：动作执行（Week 3）

- [ ] 告警动作（调用现有 AlarmService）
- [ ] 控制设备动作（调用现有 DeviceService）
- [ ] 延迟动作
- [ ] 执行日志

### 8.6 阶段四：高级功能（Week 4+）

- [ ] 数据转发动作（调用 HTTP）
- [ ] 通知渠道
- [ ] 规则调试
- [ ] 统计分析

---

## 九、技术选型

### 9.1 Cron 解析

```rust
// 使用 cron crate
use cron::Schedule;

// 解析 Cron 表达式
let schedule = Schedule::from_str("0 8 * * *").unwrap();
```

### 9.2 规则存储

- 使用现有 SQLite
- 条件和动作 JSON 序列化存储

### 9.3 定时任务

- 使用 tokio 的 timer 或独立的 scheduler
- 支持秒级 Cron 表达式

---

## 十、总结

| 特性 | 说明 |
|------|------|
| **可视化** | Web 界面配置，无需编码 |
| **灵活性** | 条件组合、动作链式执行 |
| **可扩展** | 易于添加新的条件类型和动作 |
| **可观测** | 执行日志、触发记录 |
| **轻量级** | 边缘网关友好 |

---

*设计完成*
