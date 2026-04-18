# 边缘智能体产品设计与实现方案

> **项目**: TinyIoTHub + OpenClaw 边缘智能体
> **日期**: 2026-03-27
> **状态**: 已审查（含 Phase 1 实现计划 + Skills 设计）

## 1. 背景与目标

### 1.1 项目背景

TinyIoTHub 目前已具备成熟的云端 SaaS 物联网平台能力：
- 设备管理（CRUD、多协议驱动：Modbus TCP/SNMP/Simulated）
- 告警引擎（规则、事件、通知）
- 自动化（条件触发、动作执行）
- 事件系统（SSE 实时推送）
- 基础 MCP Server（8 个工具）

**白皮书愿景**：将 TinyIoTHub 升级为"AI 驱动的自主型边缘计算平台"，核心特性：
- 接入即自治：自然语言描述设备 → 自动匹配/生成驱动 → 分钟级上线
- 运行即自愈：分级自愈（L0-L3）+ 心跳探针 → 无人值守
- 云端协同：驱动库 + 故障知识库持续进化
- 自然语言运维：对话式设备问询与配置

### 1.2 集成架构

```
┌─────────────────────────────────────────────────────────┐
│  用户（自然语言）                                        │
└─────────────────┬───────────────────────────────────────┘
                  │ "3号厂房温湿度传感器为什么离线？"
                  ↓
┌─────────────────────────────────────────────────────────┐
│  OpenClaw（AI 编排器）                                   │
│  - 对话管理                                             │
│  - 意图识别                                             │
│  - 技能编排                                             │
│  - 调用 MCP Tools                                       │
└─────────────────┬───────────────────────────────────────┘
                  │ MCP over HTTP
                  ↓
┌─────────────────────────────────────────────────────────┐
│  TinyIoTHub API（同一进程）                             │
│  ┌───────────────────────────────────────────────────┐ │
│  │ /mcp 端点（MCP 协议处理）                          │ │
│  │                                                   │ │
│  │ MCP Tools（ToolHandler 注册表）：                  │ │
│  │   - device_*：设备 CRUD、读写、命令                │ │
│  │   - driver_*：驱动匹配、生成、加载、测试          │ │
│  │   - heartbeat_*：心跳上报、状态查询                │ │
│  │   - self_heal_*：自愈策略、恢复执行               │ │
│  │   - knowledge_*：知识查询、知识贡献                │ │
│  └───────────────────────────────────────────────────┘ │
│  ┌───────────────────────────────────────────────────┐ │
│  │ /api/v1/* REST API（Web UI）                      │ │
│  └───────────────────────────────────────────────────┘ │
└─────────────────┬───────────────────────────────────────┘
                  │ 内部函数调用
                  ↓
┌─────────────────────────────────────────────────────────┐
│  TinyIoTHub Rust 后端（Axum）                           │
│  ┌─────────────┐ ┌─────────────┐ ┌───────────────────┐  │
│  │ 设备服务    │ │ 驱动管理    │ │ 自愈引擎          │  │
│  │             │ │             │ │ - 探针调度器     │  │
│  │             │ │             │ │ - 策略评估器     │  │
│  │             │ │             │ │ - 动作执行器     │  │
│  └─────────────┘ └─────────────┘ └───────────────────┘  │
│  ┌─────────────┐ ┌─────────────┐ ┌───────────────────┐  │
│  │ 心跳上报   │ │ 知识服务    │ │ 云端同步         │  │
│  │ 服务       │ │             │ │ 服务              │  │
│  └─────────────┘ └─────────────┘ └───────────────────┘  │
└─────────────────┬───────────────────────────────────────┘
                  │ 结构化心跳 / 云端 API
                  ↓
┌─────────────────────────────────────────────────────────┐
│  云端平台（未来）                                        │
│  - 驱动库（500+ 驱动）                                  │
│  - 故障知识库                                           │
│  - 多智能体聚合                                        │
│  - L2/L3 告警与工单                                     │
└─────────────────────────────────────────────────────────┘
```

### 1.3 技术前提

- OpenClaw 作为 AI 编排器，使用 MCP 协议调用工具
- TinyIoTHub MCP Server 嵌入 API 进程（`api/src/api/mcp/`），HTTP 传输
- 驱动通过 libloading 动态加载（`.so` 文件）
- 设备数据通过 SQLx + SQLite 持久化
- JWT Token 透传：OpenClaw → MCP → API 层统一验证

---

## 2. 分阶段实现计划

### 第一阶段：MCP 工具扩展（基础层）

**目标**：为 OpenClaw 提供丰富的 MCP 工具集，使其能够全面控制 TinyIoTHub。

#### 2.1 MCP 工具清单

| 分类 | 工具名 | 描述 | 优先级 |
|------|--------|------|--------|
| **device** | `create_device` | 从自然语言描述或结构化输入创建设备 | P0 |
| | `update_device` | 更新设备配置 | P0 |
| | `delete_device` | 删除设备 | P0 |
| | `list_devices` | *已有* 分页列表，支持过滤 | P1 |
| | `get_device` | *已有* 单个设备详情 | P1 |
| | `get_device_status` | *已有* 在线/离线状态 + 信号强度 | P1 |
| | `read_properties` | 批量读取设备属性 | P0 |
| | `write_properties` | 批量写入设备属性 | P0 |
| | `send_command` | *已有* 发送设备命令 | P1 |
| | `get_device_history` | 时序数据查询 | P0 |
| | `get_device_metrics` | 设备性能指标（CPU/内存/网络） | P1 |
| | `export_device_report` | 生成设备运行报告 | P2 |
| **driver** | `list_drivers` | *已有* 列出可用驱动 | P1 |
| | `match_driver` | 按品牌/型号/协议自动匹配驱动 | P0 |
| | `generate_driver` | 从自然语言描述 AI 生成驱动 | P0 |
| | `load_driver` | 加载驱动到网关 | P0 |
| | `unload_driver` | 卸载驱动 | P1 |
| | `test_driver` | 冒烟测试驱动 | P0 |
| | `get_driver_config_schema` | 获取驱动配置参数 | P1 |
| **heartbeat** | `report_heartbeat` | 推送网关健康状态到云端 | P0 |
| | `get_heartbeat_status` | 获取当前网关健康状态 | P0 |
| | `configure_heartbeat` | 配置探针间隔和阈值 | P1 |
| **self_heal** | `get_self_heal_policy` | 获取当前 L0-L3 自愈策略 | P0 |
| | `execute_self_heal_action` | 手动触发恢复动作 | P0 |
| | `get_recovery_history` | 查看历史恢复事件 | P1 |
| **knowledge** | `query_knowledge_base` | 搜索故障解决方案 | P1 |
| | `contribute_knowledge` | 提交新故障解决方案 | P2 |
| | `sync_knowledge` | 同步本地知识 ↔ 云端 | P2 |

#### 2.2 工具详细设计

> **工具命名规范**：MCP 工具统一使用 `verb_object` 格式（如 `create_device`），与现有 TinyIoTHub MCP 工具命名惯例保持一致（如 `list_devices`、`get_device_status`）。

##### 2.2.1 create_device

**输入**：
```json
{
  "name": "string",
  "type": "sensor | actuator | gateway",
  "protocol": "modbus_tcp | modbus_rtu | snmp | http | onvif | simulated",
  "config": {
    // 互斥组：只能出现其中一组，由 interface 字段决定类型
    // ethernet 接口
    "ip": "string (optional)",
    "port": "number (optional)",
    // serial 接口（interface: "serial" 时使用）
    "serial": {
      "port": "/dev/ttyUSB0",
      "baudrate": 9600,
      "data_bits": 8,
      "stop_bits": 1,
      "parity": "none"
    },
    // lora 接口（interface: "lora" 时使用）
    "lora": {
      "device_eui": "string",
      "app_eui": "string",
      "app_key": "string"
    }
  },
  "interface": "serial | ethernet | can | lora",
  "points": [
    {"name": "温度", "address": "40101", "type": "float32", "access": "read"},
    {"name": "湿度", "address": "40102", "type": "float32", "access": "read"}
  ],
  "description": "string (optional)"
}
```

**输出**：
```json
{
  "device_id": "uuid",
  "status": "created",
  "driver_id": "string",
  "auto_test_result": {
    "passed": true,
    "read_values": [{"point": "温度", "value": 25.6}],
    "elapsed_ms": 120
  }
}
```

##### 2.2.2 match_driver

**输入**：
```json
{
  "brand": "string (optional)",
  "model": "string (optional)",
  "protocol": "modbus_tcp | modbus_rtu | snmp | http",
  "interface": "serial | ethernet"
}
```

**输出**：
```json
{
  "matched": true,
  "driver_id": "string",
  "driver_name": "string",
  "confidence": 0.95,
  "config_schema": { ... },
  "cloud_available": false
}
```

##### 2.2.3 generate_driver

**输入**：
```json
{
  "protocol": "modbus_rtu",
  "points": [
    {"name": "温度", "register": 40101, "data_type": "float32"},
    {"name": "湿度", "register": 40102, "data_type": "float32"}
  ],
  "description": "XX品牌温湿度传感器，Modbus RTU"
}
```

**输出**：
```json
{
  "generated": true,
  "driver_id": "string",
  "code_preview": "class XXSensorDriver: ...",
  "test_passed": true,
  "cloud_sync_status": "pending"
}
```

##### 2.2.4 report_heartbeat

**输入**：
```json
{
  "gateway_id": "GW-001",
  "timestamp": "2026-03-27T10:00:00Z",
  "self_check": {
    "cpu": 45,
    "memory": 60,
    "disk": 30,
    "network": {"eth0": "up", "lora": "up"},
    "services": {"modbus_master": "running", "http_server": "running", "lora_ns": "running"}
  },
  "devices": [
    {"id": "sensor_1", "status": "online", "last_data": "2026-03-27T09:59:30Z", "rssi": -85},
    {"id": "sensor_2", "status": "offline", "last_data": "2026-03-27T09:45:00Z", "rssi": null}
  ],
  "auto_actions": [
    // type 有效值: restart_driver | rejoin_lora | reconnect_device | clean_logs | create_ticket
    // result 有效值: success | failed
    {"type": "restart_driver", "target": "modbus_1", "result": "success", "timestamp": "..."}
  ]
}
```

##### 2.2.5 execute_self_heal_action

**输入**：
```json
{
  "level": "L0 | L1 | L2 | L3",
  "target": "driver_name | device_id | system",
  "action": "restart_driver | rejoin_lora | reconnect_device | clean_logs | create_ticket",
  "force": false
}
```

**输出**：
```json
{
  "action_id": "uuid",
  "executed": true,
  // result 有效值: success | failed | pending_approval
  "result": "success",
  "details": "string",
  "logs": ["action started", "driver stopped", "driver started"]
}
```

---

## 3. 第二阶段：自愈引擎

### 3.1 架构设计

```
┌──────────────────────────────────────────────────────────┐
│  自愈引擎                                                 │
│                                                          │
│  探针调度器（cron 驱动，可配置周期）                      │
│  ┌────────────────────────────────────────────────────┐  │
│  │ System Probe（系统探针）     周期：10m             │  │
│  │   - CPU 使用率                                      │  │
│  │   - 内存使用率                                      │  │
│  │   - 磁盘使用率                                      │  │
│  │   - 网络连通性（ping）                              │  │
│  │   - 关键进程存活                                    │  │
│  └────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────┐  │
│  │ Device Probe（设备探针）    周期：30m             │  │
│  │   - 设备在线状态                                    │  │
│  │   - 数据刷新超时                                    │  │
│  │   - 数据合理性校验                                  │  │
│  └────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────┐  │
│  │ Task Probe（任务探针）     周期：15m             │  │
│  │   - 数据上云任务                                    │  │
│  │   - 联动规则执行状态                                │  │
│  └────────────────────────────────────────────────────┘  │
│                           ↓                               │
│  策略评估器                                               │
│  ┌────────────────────────────────────────────────────┐  │
│  │  对比探针结果与阈值                                  │  │
│  │  判定严重等级 → L0 / L1 / L2 / L3                  │  │
│  │  发出恢复事件                                        │  │
│  └────────────────────────────────────────────────────┘  │
│                           ↓                               │
│  动作执行器                                               │
│  ┌────────────────────────────────────────────────────┐  │
│  │ L0：仅记录日志，不上报                              │  │
│  │ L1：restart_driver / rejoin_lora / reconnect_device │  │
│  │ L2：report_cloud + 清理日志                         │  │
│  │ L3：report_cloud + 生成工单 + 禁止自动重启          │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

### 3.2 自愈策略配置

```yaml
self_healing:
  enabled: true

  levels:
    L0:
      actions: [log_only]
      conditions:
        - type: signal_weak
          threshold: -110  # dBm
        - type: single_timeout
          count: 1

    L1:
      actions: [restart_driver, rejoin_lora, reconnect_device]
      conditions:
        - type: process_dead
        - type: device_timeout
          count: 3
        - type: lora_rejoin_failed
          count: 2

    L2:
      actions: [report_cloud, clean_logs]
      conditions:
        - type: devices_offline_ratio
          threshold: 0.2  # 20% 设备离线
        - type: disk_usage
          threshold: 85  # 磁盘使用百分比 (0-100)
        - type: consecutive_failures
          count: 5

    L3:
      actions: [report_cloud, create_ticket]
      require_approval: true
      conditions:
        - type: bus_short_circuit
        - type: core_service_crash
        - type: memory_leak_suspected
```

### 3.3 新增 API 端点

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/v1/self-healing/policies` | 获取自愈策略配置 |
| PUT | `/v1/self-healing/policies` | 更新自愈策略 |
| POST | `/v1/self-healing/actions/{level}` | 执行指定级别恢复动作 |
| GET | `/v1/self-healing/events` | 获取恢复历史 |
| GET | `/v1/heartbeat/probes` | 获取探针状态 |
| POST | `/v1/heartbeat/probes/configure` | 配置探针参数 |

---

## 4. 第三阶段：自然语言设备接入

### 4.1 端到端流程

```
用户: "串口1接入XX品牌温湿度传感器，Modbus RTU，40101温度，40102湿度"

OpenClaw（意图识别）:
  → brand: "XX品牌"
  → interface: "串口1"
  → protocol: "Modbus RTU"
  → points:
      - {name: "温度", register: 40101, type: "float32"}
      - {name: "湿度", register: 40102, type: "float32"}

OpenClaw 调用 MCP Tools:
  ┌─────────────────────────────────────────────────────────┐
  │ 1. match_driver(brand="XX品牌", protocol="Modbus RTU")  │
  │    → 命中：返回缓存驱动                                  │
  │    → 未命中：generate_driver(...) → AI 生成驱动代码      │
  └─────────────────────────────────────────────────────────┘
  ┌─────────────────────────────────────────────────────────┐
  │ 2. create_device(config={                               │
  │      interface: "serial",                               │
  │      port: "/dev/ttyUSB0",                              │
  │      protocol: "modbus_rtu",                            │
  │      points: [...]                                       │
  │    })                                                    │
  └─────────────────────────────────────────────────────────┘
  ┌─────────────────────────────────────────────────────────┐
  │ 3. test_driver(device_id="xxx")                         │
  │    → 冒烟测试：连续读取 5 次                             │
  │    → 数据合理性校验                                     │
  │    → 返回测试报告                                       │
  └─────────────────────────────────────────────────────────┘
  ┌─────────────────────────────────────────────────────────┐
  │ 4. report_heartbeat()                                   │
  │    → 推送网关状态到云端                                  │
  └─────────────────────────────────────────────────────────┘

OpenClaw 响应用户:
  "✅ 设备已接入成功！

   设备名称：XX品牌温湿度传感器
   协议：Modbus RTU（串口1）
   驱动状态：已加载并通过测试

   当前数据：
   - 温度：25.6°C
   - 湿度：65.2%RH

   报告已生成，包含详细的测试结果和连接拓扑图。"
```

### 4.2 自然语言意图 Schema

OpenClaw 提取的结构化信息，供 MCP Tools 使用：

```json
{
  "intent": "device_onboarding",
  "entities": {
    "brand": "string (optional)",
    "model": "string (optional)",
    "interface": "serial | ethernet | can | lora",
    "interface_detail": "串口1 | /dev/ttyUSB0 | eth0",
    "protocol": "modbus_tcp | modbus_rtu | snmp | http | onvif",
    "ip": "string (optional)",
    "port": "number (optional)",
    "points": [
      {
        "name": "string",
        "register": "number",
        "address": "string (alternative to register)",
        "data_type": "float32 | int16 | int32 | bool | string",
        "access": "read | write | read_write"
      }
    ],
    "description": "string (optional)"
  },
  "confidence": 0.0-1.0,
  "ambiguities": [
    {"field": "register_format", "question": "寄存器格式是单字还是双字？"}
  ]
}
```

### 4.3 驱动生成流程

```
用户描述 → OpenClaw 提取意图 → generate_driver MCP Tool
                                        ↓
                               云端 LLM（代码生成）
                                        ↓
                               生成 Python/C 驱动代码
                                        ↓
                               沙箱测试（冒烟 + 稳定性）
                                        ↓
                               通过 → 持久化加载
                               失败 → 返回错误，提示修正
                                        ↓
                               同步至云端驱动库
```

---

## 5. 第四阶段：云端知识闭环

### 5.1 知识流动

```
┌─────────────────┐                              ┌─────────────────┐
│  本地 TinyIoTHub │                              │    云端平台      │
│                 │                              │                 │
│  新设备接入成功  ──────────────────────────────→  驱动同步至驱动库  │
│                 │                              │                 │
│  故障本地解决   ──────────────────────────────→  方案同步至知识库  │
│                 │                              │                 │
│  驱动代码生成   ──────────────────────────────→  代码审查 → 市场   │
│                 │                              │                 │
│  自愈成功       ──────────────────────────────→  策略持续优化       │
│                 │                              │                 │
│  ←──────────────────────────────  新驱动可用    │                 │
│  ←──────────────────────────────  优化策略下发  │                 │
└─────────────────┘                              └─────────────────┘
```

### 5.2 知识库数据结构

```json
{
  "knowledge_id": "uuid",
  "category": "fault_resolution | driver_config | optimization_tip",
  "tags": ["modbus", "timeout", "lora"],
  "problem": {
    "description": "Modbus 设备频繁超时",
    "conditions": {
      "protocol": "modbus_rtu",
      " symptom": "连续 3 次超时后设备离线"
    }
  },
  "solution": {
    "actions": [
      {"type": "adjust_timeout", "value": 3000},
      {"type": "enable_retry", "count": 3}
    ],
    "success_rate": 0.95
  },
  "contributor": {
    "type": "auto | manual",
    "agent_id": "GW-001",
    "timestamp": "2026-03-27T10:00:00Z"
  }
}
```

---

## 6. 技术实现细节

### 6.1 MCP Server 扩展

> **前提说明**：`mcp/` 目录已存在，包含基础 MCP Server 实现（8 个工具）。本节描述在现有基础上扩展新工具类别的文件结构变更。

**文件结构**：
```
mcp/src/
├── main.rs              # STDIO 入口
├── transport.rs         # STDIO/HTTP 传输层
├── tools/
│   ├── mod.rs           # 工具注册表
│   ├── device.rs        # 设备类工具（8个 → 12个）
│   ├── driver.rs        # 驱动类工具（6个）
│   ├── heartbeat.rs     # 心跳类工具（3个）
│   ├── self_heal.rs     # 自愈类工具（3个）
│   └── knowledge.rs     # 知识类工具（3个）
├── handlers/
│   ├── device_handler.rs
│   ├── driver_handler.rs
│   ├── heartbeat_handler.rs
│   ├── self_heal_handler.rs
│   └── knowledge_handler.rs
└── config.rs
```

### 6.2 自愈引擎新增模块

**文件结构**（在 `api/src/` 下新增）：
```
api/src/
├── domain/
│   └── self_healing/
│       ├── mod.rs
│       ├── probe.rs         # 探针定义
│       ├── policy.rs        # 策略定义
│       ├── evaluator.rs     # 策略评估器
│       └── executor.rs      # 动作执行器
├── application/
│   └── self_healing_service.rs
├── infrastructure/
│   └── self_healing/
│       ├── probe_scheduler.rs
│       ├── actions/
│       │   ├── mod.rs
│       │   ├── restart_driver.rs
│       │   ├── rejoin_lora.rs
│       │   └── clean_logs.rs
│       └── cloud_reporter.rs
└── api/
    └── self_healing/
        ├── mod.rs
        ├── handlers.rs
        └── routes.rs
```

### 6.3 心跳数据结构

```rust
// api/src/domain/self_healing/probe.rs

pub struct HeartbeatReport {
    pub gateway_id: String,
    pub timestamp: DateTime<Utc>,
    pub self_check: SystemStatus,
    pub devices: Vec<DeviceStatus>,
    pub auto_actions: Vec<AutoAction>,
}

pub struct SystemStatus {
    pub cpu: u8,           // 百分比
    pub memory: u8,         // 百分比
    pub disk: u8,           // 百分比
    pub network: HashMap<String, NetworkInterfaceStatus>,
    pub services: HashMap<String, ServiceStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProbeType {
    System,
    Device,
    Task,
}

pub enum SeverityLevel {
    L0,  // 仅记录
    L1,  // 本地自愈
    L2,  // 上报云端 + 本地处理
    L3,  // 上报云端 + 生成工单
}
```

---

## 7. 实施顺序与依赖

```
第一阶段（MCP 工具扩展）
├── P0: device_create, device_read_properties, device_write_properties
├── P0: driver_match, driver_generate, driver_load, driver_test
├── P0: heartbeat_report, heartbeat_get_status
├── P0: self_heal_get_policy, self_heal_execute
├── P1: device_update, device_delete, device_history, device_metrics
├── P1: driver_unload, driver_config_schema
├── P1: heartbeat_configure
├── P1: self_heal_get_recovery_history
├── P2: device_export_report
└── P2: knowledge_*

第二阶段（自愈引擎）
├── 探针调度器
├── 策略评估器
├── L0/L1 动作执行器
├── 心跳上报服务（结构化 JSON → 云端）
└── L2/L3 告警与工单（云端联动）

第三阶段（NL 设备接入）
├── OpenClaw 意图识别集成
├── 端到端 NL → 设备上线流程
├── 驱动生成与测试
└── 实施验收报告生成

第四阶段（云端知识闭环）
├── 云端驱动库同步
├── 故障知识库同步
├── 知识贡献与查询
└── 第三方驱动市场（远期）
```

---

## 8. 测试策略

### 8.1 单元测试

- 探针调度器：验证各探针按配置周期执行
- 策略评估器：验证阈值判定逻辑（L0/L1/L2/L3）
- 动作执行器：验证每个动作的正确性
- MCP 工具：验证输入解析和输出构造

### 8.2 集成测试

- MCP Server：OpenClaw 调用完整工具链
- 自愈引擎：探针 → 评估 → 动作 → 日志 完整流程
- 心跳上报：网关 → 云端接收 端到端

### 8.3 端到端测试

- NL 描述 → OpenClaw → MCP → 设备上线 → 数据读取
- 设备故障 → 探针检测 → 自愈执行 → 恢复确认

---

## 9. 风险与应对

| 风险 | 影响 | 应对措施 |
|------|------|----------|
| 驱动生成代码质量不可控 | 生成的驱动可能不稳定 | 沙箱测试 + 人工审核流程 |
| 自愈动作误判导致服务中断 | L3 误操作可能影响生产 | L3 设置人工审批门槛 |
| 云端同步网络不可用 | 本地知识无法同步 | 本地缓存，离线可用 |
| OpenClaw 意图识别错误 | 设备配置错误 | 二次确认机制 |
| 多设备并发自愈冲突 | 同时重启同一驱动 | 动作执行加锁 |

---

## 10. 后续规划

- **V2.0**：扩展协议覆盖（BACnet、OPC UA）、多模态设备识别
- **V3.0**：时序预测故障预警、跨设备联动优化、数字孪生集成

---

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAR | 架构图修复（1.2+1.3），4项决策 |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | Not run |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 2 | CLEAR | 4 issues, all resolved |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | Not run |

### Eng Review Findings (Round 1 - detailed)

**Resolved issues:**
1. ✅ Handler registry pattern — replace match explosion with `HashMap<ToolHandler>` registry
2. ✅ NotImplemented error variant — add `ToolError::NotImplemented(String)` for Phase 1 stubs
3. ✅ Pagination clamp — `page_size.clamp(1, 1000)` on all list tools
4. ✅ Structured stub for `generate_driver` — returns phase info for OpenClaw
5. ✅ Backend endpoints in Phase 1 — ~11 new API endpoints for heartbeat/self_heal/knowledge
6. ✅ Batch query for `report_heartbeat` — prevents unbounded device query
7. ✅ Time range bounds for `get_device_history` — 7-day max window
8. ✅ Embedded MCP — MCP from separate crate to `api/src/api/mcp/`
9. ✅ Skills creation — device-onboarding, heartbeat-query, device-status, alarm-management
10. ✅ Architecture confirmed — OpenClaw skill → MCP over HTTP → TinyIoTHub API /mcp

**Critical gap identified:**
- No tests for stub tool error responses — should verify each stub returns `NotImplemented` with correct phase info

### Eng Review Findings (Round 2 - this review)

**Resolved issues:**
1. ✅ Section 1.2 架构图 — 更新为嵌入式 MCP，与 Section 12 一致
2. ✅ Section 1.3 技术前提 — 更新为嵌入式 MCP + JWT 透传说明
3. ✅ JWT 验证点 — MCP handler 复用现有 Bearer JWT middleware（`context.rs`）
4. ✅ 工具 handler 调用层次 — 确认直接调用 domain service 函数，不走 HTTP
5. ✅ 测试补充 — Task 9 新增 MCP → 后端集成测试
6. ✅ JWT 缓存 — 在请求 context 中缓存解析结果，TTL 为 token 剩余有效期

### Phase 1 Scope Summary

| Category | In Scope | Deferred |
|----------|----------|----------|
| MCP 端点 `/mcp` | ✅ 嵌入式（复用 API 容器） | — |
| MCP tool surface (27 tools) | ✅ All | — |
| Backend API endpoints | ✅ ~11 new endpoints | — |
| Handler registry refactor | ✅ | — |
| OpenClaw Skills (4个) | ✅ device-onboarding, heartbeat, status, alarm | — |
| Skills prompts | ✅ 4 个引导文档 | — |
| JWT auth | ✅ Bearer JWT middleware 复用 + 解析结果缓存 | — |
| MCP → 后端集成测试 | ✅ Task 9 新增 | — |
| Self-healing engine | ❌ | Phase 2 |
| Cloud LLM driver gen | ❌ | Phase 3 |
| Knowledge cloud sync | ❌ | Phase 4 |
| BACnet/OPC UA | ❌ | V2.0 |

### Unresolved decisions
None — all decisions resolved during review.

---

## 11. OpenClaw Skills 设计

### 11.1 Skill 架构

OpenClaw 通过 Skill 调用 TinyIoTHub MCP 工具。每个 Skill 封装一组相关的工具调用和提示词。

```
OpenClaw
  └── Skill: tinyiothub-device-onboarding
              ├── Prompts: 设备接入引导词
              └── Tools: [create_device, match_driver, test_driver, ...]
       Skill: tinyiothub-heartbeat
              ├── Prompts: 心跳查询引导词
              └── Tools: [report_heartbeat, get_heartbeat_status]
```

### 11.2 Skill 目录结构

```
skills/
└── tinyiothub/
    ├── skill.yaml           # Skill 元数据
    └── prompts/
        ├── device-onboarding.md    # 设备接入引导
        ├── heartbeat-query.md       # 心跳查询引导
        ├── device-status.md         # 设备状态查询
        └── alarm-management.md      # 告警管理引导
```

### 11.3 Skill 定义

#### 11.3.1 device-onboarding（设备接入）

**文件**: `skills/tinyiothub/skill.yaml`

```yaml
name: tinyiothub-device-onboarding
description: 物联网设备快速接入，从自然语言描述到设备上线
version: 1.0.0
tools:
  - create_device
  - match_driver
  - generate_driver
  - test_driver
  - report_heartbeat
```

**Prompt** (`skills/tinyiothub/prompts/device-onboarding.md`):

```markdown
# 设备接入技能

你是一个物联网设备接入专家。当用户描述要接入设备时，按照以下步骤执行：

## 步骤 1: 理解设备信息
从用户描述中提取：
- 设备品牌/型号 (brand, model)
- 硬件接口 (interface: serial/ethernet/can/lora)
- 通信协议 (protocol: modbus/snmp/http/onvif)
- 数据点表 (points: 寄存器/变量)
- 网络配置 (ip, port)

## 步骤 2: 匹配驱动
调用 `match_driver` 工具，传入 protocol 和 brand。
- 如果匹配成功：获取驱动配置 schema
- 如果匹配失败：调用 `generate_driver` 生成驱动

## 步骤 3: 创建设备
调用 `create_device` 工具，传入：
- name: 设备名称
- device_type: sensor/actuator/gateway
- interface: 接口类型
- protocol: 协议类型
- config: 接口配置
- points: 数据点表

## 步骤 4: 测试验证
调用 `test_driver` 工具，验证设备通信：
- 冒烟测试：连续读取 5 次
- 数据合理性校验
- 返回测试报告

## 步骤 5: 上报心跳
调用 `report_heartbeat` 通知网关设备已接入

## 常见设备描述模板

用户可能这样描述：
- "串口1接入XX品牌温湿度传感器，Modbus RTU，40101温度，40102湿度"
- "以太网接入XX品牌PLC，IP 192.168.1.100，Modbus TCP"
- "LoRa DTU接入气表，设备EUI xxx"

提取信息后按步骤执行。
```

#### 11.3.2 heartbeat-query（心跳查询）

**文件**: `skills/tinyiothub/skill.yaml` (追加)

```yaml
tools:
  - get_heartbeat_status
  - configure_heartbeat
```

**Prompt** (`skills/tinyiothub/prompts/heartbeat-query.md`):

```markdown
# 心跳查询技能

你负责查询和配置网关心跳监控。

## 查询心跳状态
调用 `get_heartbeat_status` 获取当前网关健康状态。

用户可能问：
- "网关心跳正常吗？"
- "查看心跳状态"
- "系统健康状态如何？"

返回：CPU、内存、磁盘、网络、关键服务状态

## 配置心跳
调用 `configure_heartbeat` 修改探针配置。

用户可能说：
- "把心跳间隔改成10分钟"
- "禁用系统探针"

参数：
- interval_seconds: 心跳间隔（秒）
- probes.system: 系统探针配置
- probes.devices: 设备探针配置
```

#### 11.3.3 device-status（设备状态）

**文件**: `skills/tinyiothub/skill.yaml` (追加)

```yaml
tools:
  - list_devices
  - get_device_status
  - read_properties
  - get_device_history
```

**Prompt** (`skills/tinyiothub/prompts/device-status.md`):

```markdown
# 设备状态查询技能

你负责查询设备在线状态和传感器数据。

## 查询设备列表
调用 `list_devices` 获取所有设备。

## 查询设备状态
调用 `get_device_status` 获取单个设备：
- 在线/离线状态
- 最后心跳时间
- 信号强度（RSSI）

用户可能问：
- "3号厂房的设备都在线吗？"
- "温湿度传感器为什么离线？"
- "查看设备状态"

## 读取传感器数据
调用 `read_properties` 读取当前值。

## 查询历史数据
调用 `get_device_history` 查询历史：
- 支持时间范围（默认24小时）
- 最大7天窗口
```

#### 11.3.4 alarm-management（告警管理）

**文件**: `skills/tinyiothub/skill.yaml` (追加)

```yaml
tools:
  - list_alarms
  - acknowledge_alarm
  - get_alarm_statistics
```

---

### 11.4 核心流程：设备接入详解

设备接入是最复杂的 Skill，以下是完整流程：

```
用户: "串口1接入XX品牌温湿度传感器，Modbus RTU，40101温度，40102湿度"
                                  ↓
┌─────────────────────────────────────────────────────────────┐
│ Step 1: 意图识别                                            │
│ - interface: "串口1"                                        │
│ - brand: "XX品牌"                                          │
│ - protocol: "Modbus RTU"                                   │
│ - points: [{name: "温度", register: 40101}, {name: "湿度", register: 40102}]│
└─────────────────────────────────────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────┐
│ Step 2: 驱动匹配 (match_driver)                            │
│ - 传入: protocol="modbus_rtu", brand="XX品牌"              │
│ - 命中驱动: modbus_rtu_generic                             │
│ - 返回: driver_id, config_schema                          │
└─────────────────────────────────────────────────────────────┘
                                  ↓ (未命中则 generate_driver)
┌─────────────────────────────────────────────────────────────┐
│ Step 3: 创建设备 (create_device)                           │
│ - name: "XX品牌温湿度传感器"                               │
│ - interface: "serial"                                    │
│ - protocol: "modbus_rtu"                                  │
│ - config: {port: "/dev/ttyUSB0", baudrate: 9600}        │
│ - points: [{name: "温度", address: "40101", type: "float32"}]│
└─────────────────────────────────────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────┐
│ Step 4: 测试验证 (test_driver)                             │
│ - 冒烟测试: 连续读取 5 次                                  │
│ - 数据合理性校验: 温度范围 -40~80°C                       │
│ - 返回: test_passed, read_values, elapsed_ms              │
└─────────────────────────────────────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────┐
│ Step 5: 上报心跳 (report_heartbeat)                        │
│ - gateway_id, timestamp                                    │
│ - device: {id, status: "online"}                         │
│ - auto_actions: []                                        │
└─────────────────────────────────────────────────────────────┘
                                  ↓
用户看到: "✅ 设备已接入成功！温度: 25.6°C, 湿度: 65.2%RH"
```

---

## 12. 实施计划（Phase 1）

### 12.1 架构确认

```
OpenClaw (AI 编排器，skill 驱动)
    ↓ Authorization: Bearer <jwt>
TinyIoTHub API :3002
    ├── /api/v1/*  — REST API（Web UI）
    └── /mcp       — MCP 协议端点（OpenClaw Skill 调用）
              ├── tools/list — 返回所有工具
              └── tools/call — 复用现有 Bearer JWT middleware
                               JWT 在 handlers.rs 解析后注入 context
                               工具 handler 直接调用 domain service 函数
```

**认证流程：**
1. OpenClaw 请求携带 `Authorization: Bearer <jwt>`
2. MCP handler 复用现有 `context.rs` 的 JWT 中间件逻辑
3. 解析出的 `user_id` / `tenant_id` 注入请求 context
4. **JWT 解析结果缓存**：在请求 context 中缓存，TTL 为 token 剩余有效期
5. 工具 handler 调用 service 函数时携带 context
6. API 层已有 SQL 注入防护和 RBAC 验证

### 12.2 文件变更

#### 新增文件

| 文件 | 描述 |
|------|------|
| `api/src/api/mcp/mod.rs` | MCP Router |
| `api/src/api/mcp/handlers.rs` | MCP 协议处理 |
| `api/src/api/mcp/tool_registry.rs` | ToolHandler trait + registry |
| `api/src/api/mcp/tools/mod.rs` | 工具定义导出 |
| `api/src/api/mcp/tools/device.rs` | 设备类别 12 个工具 |
| `api/src/api/mcp/tools/driver.rs` | 驱动类别 7 个工具 |
| `api/src/api/mcp/tools/heartbeat.rs` | 心跳类别 3 个工具 |
| `api/src/api/mcp/tools/self_heal.rs` | 自愈类别 3 个工具 |
| `api/src/api/mcp/tools/knowledge.rs` | 知识库 3 个工具 |
| `api/src/api/heartbeat/mod.rs` | 心跳端点 |
| `api/src/api/heartbeat/handlers.rs` | 心跳 API handlers |
| `api/src/api/self_healing/mod.rs` | 自愈端点 |
| `api/src/api/self_healing/handlers.rs` | 自愈 API handlers |
| `api/src/api/knowledge/mod.rs` | 知识库端点 |
| `api/src/api/knowledge/handlers.rs` | 知识库 API handlers |
| `api/src/dto/entity/heartbeat.rs` | 心跳 DTO |
| `api/src/dto/entity/self_healing.rs` | 自愈 DTO |
| `api/src/dto/entity/knowledge.rs` | 知识库 DTO |
| `skills/tinyiothub/skill.yaml` | Skill 元数据 |
| `skills/tinyiothub/prompts/device-onboarding.md` | 设备接入引导 |
| `skills/tinyiothub/prompts/heartbeat-query.md` | 心跳查询引导 |
| `skills/tinyiothub/prompts/device-status.md` | 设备状态引导 |
| `skills/tinyiothub/prompts/alarm-management.md` | 告警管理引导 |

#### 废弃文件

| 文件 | 原因 |
|------|------|
| `mcp/` crate | MCP 已嵌入 API，冗余 |

### 12.3 任务分解

#### Task 1: MCP 模块骨架（P0）
- [ ] 创建 `api/src/api/mcp/` 目录
- [ ] 实现 `tool_registry.rs` — ToolHandler trait + registry
- [ ] 实现 `handlers.rs` — `/mcp` 端点，tools/list + tools/call
- [ ] 注册到 `api/src/api/mod.rs`

#### Task 2: 设备类别工具（P0）
- [ ] `list_devices`, `get_device`, `get_device_status`
- [ ] `read_properties`, `write_properties`, `send_command`
- [ ] `create_device`, `update_device`, `delete_device`
- [ ] `get_device_history` (7天窗口), `get_device_metrics`, `export_device_report`

#### Task 3: 驱动类别工具（P0）
- [ ] `list_drivers`, `get_driver_config_schema`
- [ ] `match_driver` — 本地匹配 + 云端查询
- [ ] `generate_driver` — stub (Phase 3)
- [ ] `load_driver`, `unload_driver`, `test_driver`

#### Task 4: 心跳类别工具（P1）
- [ ] `report_heartbeat`, `get_heartbeat_status`, `configure_heartbeat`
- [ ] 端点: `POST/GET /heartbeat`

#### Task 5: 自愈类别工具（P1）
- [ ] `get_self_heal_policy`, `execute_self_heal_action`, `get_recovery_history`
- [ ] 端点: `GET/PUT /self-healing/policies`, `POST /self-healing/actions/:level`
- [ ] Phase 1 返回 stub

#### Task 6: 知识库类别工具（P2）
- [ ] `query_knowledge_base`, `contribute_knowledge`, `sync_knowledge`
- [ ] 端点: `GET/POST /knowledge`, `POST /knowledge/sync`
- [ ] Phase 1 返回 stub

#### Task 7: Skills 创建（P0）
- [ ] `skills/tinyiothub/skill.yaml` — Skill 元数据
- [ ] `prompts/device-onboarding.md` — 设备接入引导（核心）
- [ ] `prompts/heartbeat-query.md` — 心跳查询引导
- [ ] `prompts/device-status.md` — 设备状态引导
- [ ] `prompts/alarm-management.md` — 告警管理引导

#### Task 8: 废弃旧 MCP Server（P2）
- [ ] 删除 `mcp/` crate 或标记废弃

#### Task 9: 测试（P0）
- [ ] Tool registry 完整性测试
- [ ] NotImplemented 错误格式测试
- [ ] 分页参数 clamp 测试
- [ ] MCP → 后端集成测试（handlers.rs → service → DB 完整链路）

#### Task 10: 端到端验证（P0）
- [ ] `curl http://localhost:3002/mcp` — tools/list 返回 27 个工具
- [ ] OpenClaw skill 配置指向 `/mcp`
- [ ] 完整对话测试: "串口1接入温湿度传感器..."

### 12.4 任务依赖图

```
Task 1 (MCP骨架)
    ├── Task 2 (设备工具) ──→ Task 7 (device-onboarding skill)
    ├── Task 3 (驱动工具) ──→ Task 7 (device-onboarding skill)
    ├── Task 4 (心跳工具) ──→ Task 7 (heartbeat skill)
    ├── Task 5 (自愈工具)
    ├── Task 6 (知识库工具)
    └── Task 7 (Skills) ←─ 所有工具完成后
              │
              └── Task 9 (测试)
                       │
                       └── Task 10 (端到端验证)
```

### 12.5 Phase 1 交付物

| 交付物 | 描述 |
|--------|------|
| MCP 端点 `/mcp` | OpenClaw 可调用 |
| 27 个 MCP 工具 | 覆盖设备/驱动/心跳/自愈/知识库 |
| 4 个 OpenClaw Skills | device-onboarding (核心), heartbeat-query, device-status, alarm-management |
| 嵌入式部署 | 复用 API 容器，无需额外部署 |

### 12.6 后续规划

| Phase | 内容 |
|-------|------|
| Phase 2 | 自愈引擎（探针调度 + L0-L3 策略） |
| Phase 3 | 云端 LLM 驱动生成 |
| Phase 4 | 云端知识库同步 |
| V2.0 | BACnet/OPC UA 协议支持 |
| V3.0 | 时序预测故障预警 |
