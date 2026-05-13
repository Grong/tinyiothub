# Edge Gateway v0.2 — 完整架构设计

> 设计日期: 2026-05-13
> 状态: Draft
> 前置: MQTT Gateway Pairing v0.1（已完成）

## 概述

Target: 将 edge gateway 从当前骨架（配对 + 心跳，约 175 行）升级为完整的边缘网关运行时。

Core insight: **Edge = Cloud 精简版 + MQTT 通信 + 边缘特性**。Edge 直接复用 `tinyiothub-core`（traits + 模型 + rule engine）、`tinyiothub-storage`（SQLite repos）、`tinyiothub-runtime`（协议驱动）三个已有 crate，只在上层写一个精简的服务编排层。

运行环境: 嵌入式 Linux 裸进程，512MB-1GB RAM，4-8GB 存储。

## 架构概览

```
┌─────────────────────────────────────────────────┐
│  Cloud (SaaS)                                    │
│  HTTP/Axum → modules/ → services → repositories  │
│  Communication: HTTP (in) + MQTT (out to edge)   │
└────────────────────┬────────────────────────────┘
                     │ MQTT
                     │ tinyiothub/{ws}/gateway/{gw_id}/...
                     │
┌────────────────────┴────────────────────────────┐
│  Edge (Embedded Linux)                           │
│  MQTT → services → repositories (local SQLite)   │
│  Communication: MQTT (to cloud)                  │
│                                                   │
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐      │
│  │ Gateway  │ │ Command  │ │  Telemetry   │      │
│  │ Service  │ │ Service  │ │   Service    │      │
│  └──────────┘ └──────────┘ └──────────────┘      │
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐      │
│  │ Config   │ │  Device  │ │    Alarm     │      │
│  │ Service  │ │ Service  │ │   Service    │      │
│  └──────────┘ └──────────┘ └──────────────┘      │
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐      │
│  │  Driver  │ │ Offline  │ │   Monitor    │      │
│  │ Service  │ │  Buffer  │ │   Service    │      │
│  └──────────┘ └──────────┘ └──────────────┘      │
│  ┌──────────────┐ ┌──────────────┐               │
│  │ SelfHealing  │ │  Heartbeat   │               │
│  │   Service    │ │   Service    │               │
│  └──────────────┘ └──────────────┘               │
│                                                   │
│  Reused Crates:                                   │
│  tinyiothub-core (traits + models + rule engine)  │
│  tinyiothub-storage (SQLite repos)                │
│  tinyiothub-runtime (protocol drivers)            │
│  tinyiothub-error (error types)                   │
└─────────────────────────────────────────────────┘
```

## Crate 复用

Edge 直接依赖已有 workspace crates，不重复造轮子：

```toml
# edge/Cargo.toml
[dependencies]
tinyiothub-core = { path = "../crates/tinyiothub-core" }
tinyiothub-storage = { path = "../crates/tinyiothub-storage" }
tinyiothub-runtime = { path = "../crates/tinyiothub-runtime" }
tinyiothub-error = { path = "../crates/tinyiothub-error" }
```

这意味着 `DeviceRepository` trait + SQLite 实现、设备模型（Device, DeviceProperty, DeviceCommand）、告警规则引擎、Modbus/ONVIF/SNMP 协议驱动——全部复用。

## 模块映射：Cloud → Edge

| Cloud 模块 | Edge | 说明 |
|-----------|------|------|
| `device` | 精简版 | 本地设备 CRUD + 属性读写，复用 DeviceRepository |
| `drivers` | 直接用 | 复用 tinyiothub-runtime 的 Modbus/ONVIF/SNMP/MQTT 驱动 |
| `gateway` | 已有 + 增强 | 配对 + MQTT 通信 + 消息路由 |
| `alarm` | 精简版 | 本地告警评估，复用 core 的 rule engine |
| `self_healing` | 精简版 | 本地探针 + 自动恢复 |
| `template` | 精简版 | 本地设备模板，从 cloud 同步 |
| `event` | 精简版 | 遥测采集 + 转发到 cloud |
| `heartbeat` | 已有 | 心跳上报 |
| `cron` | 精简版 | 定时扫描、定时任务 |
| `monitoring` | 新增 | 资源监控（CPU/内存/磁盘/进程） |
| `auth/user/role/permission` | 不需要 | Edge 无用户体系 |
| `tenant/workspace` | 不需要 | Edge 只有单一 gateway 身份 |
| `agent/chat/mcp` | 不需要 | AI 能力在 cloud |
| `marketplace` | 不需要 | 市场在 cloud |
| `notification` | 不需要 | 通知由 cloud 统一发送 |
| `plugin` | 不需要 | 插件体系在 cloud |
| `tag` | 不需要 | 标签在 cloud |

## 目录结构

```
edge/src/
├── main.rs                  # 入口 + 事件循环
├── config.rs                # EdgeConfig + GatewayCredentials (已有)
├── app_state.rs             # AppState 组装
├── modules/
│   ├── gateway/             # MQTT 通信 + 配对
│   │   ├── types.rs
│   │   ├── service.rs       # MQTT client, publish/subscribe, 消息路由
│   │   └── pairing.rs       # 配对码 + announce (已有)
│   ├── device/              # 本地设备管理
│   │   ├── types.rs
│   │   └── service.rs       # CRUD、属性/指令代理
│   ├── driver/              # 驱动管理
│   │   ├── types.rs
│   │   └── service.rs       # 驱动生命周期、扫描发现
│   ├── telemetry/           # 遥测采集
│   │   ├── types.rs
│   │   └── service.rs       # 定时采集、变换、转发
│   ├── command/             # 指令执行
│   │   ├── types.rs
│   │   └── service.rs       # 接收 cloud 指令 → 路由到驱动
│   ├── config_mgmt/         # 配置管理
│   │   ├── types.rs
│   │   └── service.rs       # 本地 YAML + cloud 下发覆盖
│   ├── alarm/               # 本地告警
│   │   ├── types.rs
│   │   └── service.rs       # 规则评估、告警生成
│   ├── self_healing/        # 自愈
│   │   ├── types.rs
│   │   └── service.rs       # 探针调度、自动恢复
│   ├── offline/             # 离线缓冲
│   │   ├── types.rs
│   │   └── service.rs       # SQLite 缓冲、连接恢复后回放
│   ├── monitor/             # 健康监控
│   │   ├── types.rs
│   │   └── service.rs       # 资源监控、自检
│   └── heartbeat/           # 心跳
│       ├── types.rs
│       └── service.rs       # 定时上报状态
└── shared/
    ├── error.rs             # Edge 错误类型
    └── storage.rs           # SQLite 初始化 + migration
```

Edge 去掉 HTTP 层，不设 `handler/` 子目录。每个模块只有 `types.rs` + `service.rs`。

## MQTT Topic 设计

```
Edge → Cloud（上报）:
  tinyiothub/{ws}/gateway/{gw}/status           # 心跳 + 健康状态
  tinyiothub/{ws}/gateway/{gw}/telemetry         # 设备遥测数据
  tinyiothub/{ws}/gateway/{gw}/device/discover   # 设备发现结果
  tinyiothub/{ws}/gateway/{gw}/alarm             # 告警事件
  tinyiothub/{ws}/gateway/{gw}/log               # 日志上报（可选）

Cloud → Edge（下发）:
  tinyiothub/{ws}/gateway/{gw}/command           # 指令下发
  tinyiothub/{ws}/gateway/{gw}/config             # 配置下发
  tinyiothub/{ws}/gateway/{gw}/config/ack         # 配置确认（edge → cloud）

Pairing（认证前）:
  tinyiothub/pairing/announce                     # Edge 广播配对码
  tinyiothub/pairing/ack                          # Cloud 返回凭证
```

## AppState

```rust
pub struct AppState {
    pub config: EdgeConfig,
    pub credentials: GatewayCredentials,
    pub db: Arc<Database>,
    pub device_repo: Arc<dyn DeviceRepository>,
    pub device_service: Arc<DeviceService>,
    pub driver_service: Arc<DriverService>,
    pub telemetry_service: Arc<TelemetryService>,
    pub command_service: Arc<CommandService>,
    pub config_service: Arc<ConfigService>,
    pub alarm_service: Arc<AlarmService>,
    pub self_healing_service: Arc<SelfHealingService>,
    pub gateway_service: Arc<GatewayService>,
    pub offline_buffer: Arc<OfflineBuffer>,
    pub monitor_service: Arc<MonitorService>,
}
```

## 事件循环（main.rs）

```rust
async fn run_authenticated(config: EdgeConfig, creds: GatewayCredentials) -> ! {
    let state = AppState::new(config, creds).await?;

    // 1. 从 cloud 拉配置
    state.config_service.sync_from_cloud().await?;

    // 2. 首次设备扫描
    state.driver_service.scan_all().await?;

    // 3. 回放离线缓冲
    if let Ok(count) = state.offline_buffer.flush(&state.gateway_service).await {
        tracing::info!(count, "Flushed offline buffer");
    }

    // 4. 定时器
    let mut telemetry_tick = tokio::time::interval(state.config.telemetry_interval);
    let mut heartbeat_tick = tokio::time::interval(state.config.heartbeat_interval);
    let mut monitor_tick = tokio::time::interval(state.config.monitor_interval);

    loop {
        // 连接断开 → 自动重连 + 缓冲
        if !state.gateway_service.mqtt.is_alive() {
            state.offline_buffer.activate();
            state.gateway_service.mqtt.reconnect().await;
            state.offline_buffer.flush(&state.gateway_service).await.ok();
            state.config_service.sync_from_cloud().await.ok();
        }

        tokio::select! {
            _ = telemetry_tick.tick() => {
                if let Err(e) = state.telemetry_service.collect_and_forward().await {
                    tracing::warn!(?e, "Telemetry collect failed");
                }
            }
            _ = heartbeat_tick.tick() => {
                state.heartbeat_service.beat().await;
                state.monitor_service.report().await.ok();
            }
            _ = monitor_tick.tick() => {
                state.alarm_service.evaluate(&[]).await.ok();
                state.self_healing_service.run_probes().await.ok();
            }
            msg = state.gateway_service.recv() => {
                state.route_message(msg).await;
            }
        }
    }
}
```

Main.rs 只做事件循环编排，具体逻辑全部在 service 里。

## 模块设计

### Gateway Service (`modules/gateway/service.rs`)

MQTT 连接生命周期管理 + 消息路由。

```rust
pub struct GatewayService {
    credentials: GatewayCredentials,
    pub mqtt: EdgeMqttClient,
    message_tx: mpsc::Sender<GatewayMessage>,
}

pub enum GatewayMessage {
    Command(DeviceCommand),      // cloud 下发的指令
    Config(ConfigPayload),       // cloud 下发的配置
}
```

职责:
- 建立/重连 MQTT 连接（指数退避，最大 5 分钟间隔）
- 订阅所有下行 topic（command、config）
- 解析收到的消息为 `GatewayMessage` 枚举，发到内部 channel
- 提供 `publish_telemetry()`、`publish_alarm()`、`publish_discover()` 便捷方法
- 连接断开时通知其他模块（通过 `mpsc::Sender`）

### Device Service (`modules/device/service.rs`)

复用 `tinyiothub-core::models::device` 模型和 `DeviceRepository` trait。

```rust
pub struct DeviceService {
    repo: Arc<dyn DeviceRepository>,
    driver_registry: Arc<DriverRegistry>,
}
```

方法:
- `create_device(req: CreateDeviceRequest) -> Result<Device>`
- `read_properties(device_id) -> Result<Vec<DeviceProperty>>`
- `write_property(device_id, prop) -> Result<()>`
- `list_devices() -> Result<Vec<Device>>`
- `sync_from_cloud(devices: Vec<CreateDeviceRequest>) -> Result<SyncResult>`

去掉 pagination、tag、event_bus 等 cloud-only 依赖，只保留核心 CRUD + 属性读写。

### Driver Service (`modules/driver/service.rs`)

驱动复用 `tinyiothub-runtime` crate，Edge 新增生命周期管理 + 设备扫描。

```rust
pub trait Driver: Send + Sync {
    fn name(&self) -> &str;
    async fn scan(&self, config: &DriverConfig) -> Result<Vec<DiscoveredDevice>>;
    async fn read_property(&self, device: &Device, property: &str) -> Result<Value>;
    async fn write_property(&self, device: &Device, property: &str, value: Value) -> Result<()>;
    async fn execute_command(&self, device: &Device, command: &str, params: Value) -> Result<Value>;
    async fn health_check(&self, device: &Device) -> Result<DeviceHealth>;
    async fn reconnect(&self, device: &Device) -> Result<()>;
}

pub struct DiscoveredDevice {
    pub name: String,
    pub protocol: String,        // "modbus" | "onvif" | "snmp" | "mqtt"
    pub address: String,         // "192.168.1.100:502"
    pub properties: HashMap<String, Value>,
}
```

**驱动实例:**

| 驱动 | 来源 | 扫描方式 |
|------|------|---------|
| Modbus RTU | tinyiothub-runtime | 串口扫描 + 地址扫描 |
| Modbus TCP | tinyiothub-runtime | IP 范围扫描 + 端口 502 |
| ONVIF | tinyiothub-runtime | WS-Discovery 组播 |
| SNMP | tinyiothub-runtime | IP 范围扫描 + SNMP Walk |
| MQTT | tinyiothub-runtime | Topic 发现 |

**扫描流程:**
1. 读本地配置获取每个驱动的 scan_config
2. 并行调用各驱动的 `scan()`
3. 汇总 `DiscoveredDevice[]`
4. 与本地 Device 列表 diff（新增/离线/更新）
5. 通过 MQTT 上报 `device/discover` 结果到 cloud

### Telemetry Service (`modules/telemetry/service.rs`)

```rust
pub struct TelemetryService {
    device_service: Arc<DeviceService>,
    gateway_service: Arc<GatewayService>,
    offline_buffer: Arc<OfflineBuffer>,
    transform_rules: Arc<RwLock<Vec<TransformRule>>>,
}
```

- 定时遍历所有设备，读取属性
- 应用变换规则（可选 JS 表达式）
- 组装 `TelemetryReport` 上报 cloud
- 上报失败 → 写 `OfflineBuffer`

### Command Service (`modules/command/service.rs`)

```rust
pub struct CommandService {
    device_service: Arc<DeviceService>,
    driver_registry: Arc<DriverRegistry>,
    gateway_service: Arc<GatewayService>,
}
```

- 接收 cloud 指令 → 查找设备 → 获取驱动 → `driver.execute_command()`
- 执行结果通过 telemetry topic 回传 cloud

### Config Mgmt Service (`modules/config_mgmt/service.rs`)

本地 YAML baseline + cloud 下发覆盖。

```rust
pub struct Config {
    pub gateway: GatewaySettings,     // 心跳间隔、采集间隔等
    pub drivers: Vec<DriverConfig>,   // 要启用的驱动及连接参数
    pub devices: Vec<DeviceConfig>,   // 已知设备列表
    pub alarm_rules: Vec<AlarmRule>,  // 本地告警规则
    pub transform_rules: Vec<TransformRule>,
}
```

- 启动时加载 `config.yaml`
- Cloud 下发配置时合并覆盖（同 key 覆盖）
- 配置保存到本地 YAML
- 驱动/设备变化触发重载 + 重新扫描

### Offline Buffer (`modules/offline/service.rs`)

```rust
pub struct OfflineBuffer {
    db: Arc<Database>,
    active: AtomicBool,
    max_records: usize,  // 默认 100000
}
```

- MQTT 断开时自动激活，遥测 + 告警写入 SQLite
- 超过上限覆盖最旧记录（FIFO）
- 连接恢复后批量回放（每批 500 条）
- 发送成功就删记录，失败留到下一批

### Self Healing Service (`modules/self_healing/service.rs`)

```rust
pub trait Probe: Send + Sync {
    fn name(&self) -> &str;
    async fn check(&self, device: &Device, driver: &dyn Driver) -> Result<ProbeResult>;
}

pub struct ProbeResult {
    pub healthy: bool,
    pub message: Option<String>,
    pub metric: Option<f64>,
}
```

- 定时运行所有探针
- 不健康 → 尝试自动恢复：
  - `device_reachable` → 重启驱动连接
  - `device_unresponsive` → 发送重启指令
  - `memory_high` → 触发 GC 清理缓存

### Alarm Service (`modules/alarm/service.rs`)

复用 `tinyiothub-core` rule engine。

- 遥测采集后评估所有规则
- 触发生成 `AlarmEvent`，恢复添加 `resolved_at`
- 告警通过 MQTT 上报 cloud（失败则缓冲）
- Cloud 可下发新规则

### Monitor Service (`modules/monitor/service.rs`)

```rust
pub struct HealthReport {
    pub cpu_percent: f64,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub disk_free_mb: u64,
    pub uptime_secs: u64,
    pub mqtt_connected: bool,
    pub driver_status: HashMap<String, DriverStatus>,
    pub device_count: usize,
    pub offline_buffer_size: usize,
}
```

- 通过 status topic 随心跳上报

### Heartbeat Service (`modules/heartbeat/service.rs`)

已有功能的模块化封装:
- 定时上报在线状态 + 时间戳 + uptime
- 附带上一次 HealthReport

## SQLite 表设计

已有表（`tinyiothub-storage` 提供，Edge 直接复用）:
- `devices` — DeviceRepository 管理的设备表
- `device_properties` — 设备属性
- `device_commands` — 指令记录

Edge 新增 2 个表:

```sql
-- 离线缓冲表
CREATE TABLE IF NOT EXISTS offline_buffer (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    msg_type TEXT NOT NULL,       -- "telemetry" | "alarm" | "log"
    topic TEXT,
    payload BLOB NOT NULL,
    created_at INTEGER NOT NULL,  -- Unix timestamp ms
    retry_count INTEGER DEFAULT 0
);

-- 配置版本跟踪
CREATE TABLE IF NOT EXISTS config_meta (
    key TEXT PRIMARY KEY,
    cloud_version TEXT,
    local_version TEXT,
    updated_at INTEGER NOT NULL
);
```

告警规则存储在 config.yaml，本地告警事件暂存内存（连接恢复即上报），不建独立表。

## 完整启动流程

```
Edge 启动
    │
    ├── 1. 加载 EdgeConfig（环境变量 + 默认值）
    │
    ├── 2. 检查本地凭证文件 credentials.json
    │       │
    │       ├── 不存在 → Pairing 模式
    │       │   ├── 生成配对码
    │       │   ├── MQTT 匿名连接
    │       │   ├── 定期广播 announce（已有 token bucket 限制）
    │       │   └── 等待 PairingAck → 保存凭证 → 重启
    │       │
    │       └── 存在 ✓
    │           │
    │           ├── 3. 初始化 SQLite（migrate，建新表）
    │           │
    │           ├── 4. 加载本地 config.yaml
    │           │
    │           ├── 5. MQTT 认证连接 + 订阅下行 topic
    │           │
    │           ├── 6. 从 cloud 拉取最新配置（sync_from_cloud）
    │           │
    │           ├── 7. 启动驱动，执行首次设备扫描
    │           │
    │           ├── 8. 回放离线缓冲
    │           │
    │           └── 9. 进入主事件循环
    │                   ├── telemetry_tick → 采集 + 上报
    │                   ├── heartbeat_tick → 状态上报
    │                   ├── monitor_tick → 自愈 + 告警
    │                   └── mqtt_msg → 指令 / 配置
    │
    └── 连接断开？
            ├── offline_buffer_active = true
            ├── 遥测 + 告警写入 SQLite 缓冲
            └── 重连 → 拉配置 → Flush 缓冲 → 恢复正常
```

## 数据流

```
                    Cloud
                      │
              ┌───────┼───────┐
              │ MQTT          │ MQTT
              ▼               ▼
    ┌─────────────┐   ┌─────────────┐
    │  下行       │   │  上行       │
    │ command     │   │ telemetry   │
    │ config      │   │ alarm       │
    │             │   │ heartbeat   │
    └──────┬──────┘   │ discover    │
           │          └──────▲──────┘
           ▼                 │
    ┌─────────────┐   ┌──────┴──────┐
    │ ConfigSvc   │   │ Connected?  │──no──▶ OfflineBuffer
    │ DeviceSvc   │   └──────┬──────┘
    │ DriverSvc   │          │yes
    └──────┬──────┘   ┌──────┴──────┐
           ▼          │ GatewaySvc  │
    ┌──────────┐      │ publish_*   │
    │ SQLite   │      └─────────────┘
    └──────────┘
```

## 与现有代码的关系

| 文件 | 变更 |
|------|------|
| `edge/src/main.rs` | 重写：事件循环编排（约 80 行） |
| `edge/src/config.rs` | 保留 + 扩展：增加 telemetry/monitor/heartbeat interval |
| `edge/Cargo.toml` | 添加 4 个 crate 依赖 |
| `edge/src/mqtt_client.rs` | 移入 `modules/gateway/service.rs` |
| `edge/src/pairing.rs` | 移入 `modules/gateway/pairing.rs` |
| `edge/src/device_discovery.rs` | 重写为 `modules/driver/service.rs` |
| `edge/src/app_state.rs` | 新增 |
| `edge/src/modules/*/` | 新增 9 个模块 |
| `edge/src/shared/` | 新增（error.rs, storage.rs） |

## Open Questions

1. **本地告警事件是否需要持久化到 SQLite？** 当前设计是内存暂存，连接恢复即上报。如果离线期间 edge 重启，未上报的告警会丢失。可以考虑写入 `offline_buffer`（统一用离线缓冲表）。

2. **Config YAML 的 cloud_version 冲突策略？** "Cloud 覆盖同 key" 是简单策略。如果用户在 cloud 和本地同时改了同一配置项，cloud 赢。以后可能需要更精细的 per-field 覆盖标记。

3. **驱动热加载？** 当前设计是配置变更后重启驱动连接（reload），不是热加载驱动二进制。驱动本身在 `tinyiothub-runtime` crate 编译时链接。
