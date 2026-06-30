# 模拟设备驱动升级设计

> 目标：升级 SimulatedDriver 的传感器数据仿真能力，使其更接近生产环境真实行为，支持报警测试和产品演示。

## 1. 需求背景

### 当前状况

`SimulatedDriver`（`crates/tinyiothub-runtime/src/driver/drivers/simulated_driver.rs`，约 380 行）提供基本的随机游走数据生成：

- 随机游走算法：动量 0.92 + 均匀噪声 + 可配漂移速度
- 硬编码 3 种属性名：`temperature`（范围 [30,40]°C）、`humidity`（[20,100]%）、`power_status`（每 5 tick 翻转）
- 单一异常类型：概率 2%，跳变 1.5x-2.5x，持续 5-15 tick
- 设备之间完全独立，无关联

### 升级目标

1. **传感器数据更真实**——增加日周期规律、趋势漂移、高斯噪声
2. **异常模型更丰富**——支持缓慢漂移、尖峰突跳、间歇抖动、信号卡死四种异常
3. **设备间有关联**——基于现有标签（tags）系统，同标签设备共享环境上下文
4. **设备类型覆盖广**——通过属性名模式匹配，自动适配多种设备类型

### 使用场景

- **报警测试**：用多种异常类型测试不同报警规则（阈值、趋势、防抖等）
- **产品演示**：生成自然、有规律、可理解的数据，给客户展示平台功能

## 2. 属性名称 → 行为规则映射

### 核心思路

驱动内部维护一张「名称模式 → 行为规则」表，根据属性名（模糊匹配）自动推断：

- 数据类型、基准值、日波动振幅
- 噪声特征、单位

不再硬编码少数属性名，未知属性也有合理的默认行为。

### 映射表

| 名称模式 | 数据类型 | 基准值 | 日波动幅度 | 噪声 (高斯 σ) | 单位 |
|---|---|---|---|---|---|
| `*temp*`, `temperature` | float | 25 | ±8 | 0.3 | °C |
| `*humidity*`, `*humid*` | float | 60 | ±15 | 1.0 | % |
| `*vibration*`, `*vib*` | float | 2.0 | ±1.5 | 0.2 | mm/s |
| `*current*`, `*amp*` | float | 10 | ±5 | 0.5 | A |
| `*voltage*`, `*volt*` | float | 220 | ±5 | 1.0 | V |
| `*power*`, `*watt*`, `*kw*` | float | 50 | ±30 | 2.0 | W |
| `*speed*`, `*rpm*` | float | 1500 | ±200 | 30 | rpm |
| `*flow*` | float | 100 | ±30 | 3.0 | m³/h |
| `*pressure*` | float | 1.0 | ±0.3 | 0.05 | MPa |
| `*level*` | float | 50 | ±10 | 1.0 | % |
| `*energy*`, `*kwh*` | float | 累计递增 | — | — | kWh |
| `*status*`, `*state*` | enum/string | — | 状态切换 | 间歇跳变 | — |
| `*switch*`, `*relay*` | boolean | false | 随机触发 | 持续保持 | — |
| 其他未知 | 按 data_type | 取范围中值 | 小幅随机 | 低噪声 | — |

### 实现

文件：`simulated/patterns.rs`

```rust
struct PropertyBehavior {
    baseline: f64,
    daily_amplitude: f64,
    noise_sigma: f64,
    unit: String,
}

fn match_property(name: &str, data_type: &str) -> PropertyBehavior;
```

- 按名称做大小写不敏感的包含匹配
- `energy` / `kwh` 类型使用累计递增模式（每次读数累加一个小的随机增量）
- `status` / `state` 类型在枚举值之间按概率切换
- `switch` / `relay` / boolean 型有持续保持期，非每 tick 随机翻转

## 3. 信号合成模型

### 信号公式

```
output = baseline
       + periodic(t)        // 日周期正弦波
       + trend(t)           // 长期缓慢漂移
       + noise(t)           // 高斯噪声（替代现有均匀噪声）
       + anomaly(t)         // 异常分量（正常时为 0）
       + group_context(t)   // 标签关联的环境上下文（正常时为 0）
```

各分量独立计算后叠加，最终按属性类型 clamp 到合理范围（如温度 [−40, 85]°C）。

### 日周期分量（periodic）

```rust
fn periodic(tick: u64, amplitude: f64, phase_offset: f64) -> f64 {
    // 24h 对应 ticks 数（假设 interval=1000ms，一天 = 86400 ticks）
    let day_ticks = 86400.0 / (interval_ms / 1000.0);
    let angle = 2.0 * PI * (tick as f64 / day_ticks + phase_offset);
    amplitude * angle.sin()
}
```

- `phase_offset` 每个设备独立随机（0~1 均匀分布），保证同组设备不会完全同步
- `amplitude` 由属性名映射表提供默认值，可通过 `daily_amplitude_scale` 配置缩放

### 趋势分量（trend）

```rust
fn trend(tick: u64, rate: f64) -> f64 {
    rate * tick as f64
}
```

- `rate` 默认为 0.0（无漂移）
- 可配正/负值，模拟传感器老化（正值）或信号衰减（负值）

### 噪声分量（noise）

使用 Box-Muller 变换生成高斯噪声，替代现有 `(random - 0.5) * drift_speed * 0.5` 的均匀噪声：

```rust
fn gaussian_noise(rng: &mut impl Rng, sigma: f64) -> f64 {
    // Box-Muller transform
    let u1: f64 = rng.gen_range(0.0..1.0);
    let u2: f64 = rng.gen_range(0.0..1.0);
    sigma * (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}
```

- `sigma` 由属性名映射表提供默认值，可通过 `noise_level` 配置缩放

### 实现

文件：`simulated/signal.rs`

```rust
struct SignalComposer {
    interval_ms: u64,
    daily_amplitude_scale: f64,
    noise_level: f64,
    drift_rate: f64,
}

impl SignalComposer {
    fn compose(&self, behavior: &PropertyBehavior, tick: u64,
               phase_offset: f64, rng: &mut impl Rng,
               group_ctx: Option<&EnvironmentContext>) -> f64;
}
```

## 4. 异常模型升级

### 四种异常类型

| 异常类型 | 触发方式 | 表现形式 | 报警测试用途 |
|---|---|---|---|
| **缓慢漂移** | 配置概率随机触发 | 叠加方向性漂移（如 +0.5°C/tick），持续至自然消退或达到边界 | 趋势报警、预警能力 |
| **尖峰突跳** | 配置概率随机触发 | 瞬时跳变 1.5x~3x 正常范围，1~3 tick 后恢复 | 瞬时报警误报/漏报测试 |
| **间歇性抖动** | 配置概率随机触发 | 值在正常/异常间快速横跳，持续 10~30 tick | 报警防抖/延时确认逻辑 |
| **信号卡死** | 配置概率随机触发 | 值冻结在当前读数不变，持续 20~60 tick | 传感器故障、数据新鲜度告警 |

### 异常状态机

每种异常有独立的状态跟踪：

```rust
enum AnomalyState {
    Inactive,
    Drift { direction: f64, rate: f64, remaining: u32 },
    Spike { original_value: f64, spike_value: f64, remaining: u32 },
    Jitter { normal_value: f64, abnormal_value: f64, remaining: u32 },
    Stuck { frozen_value: f64, remaining: u32 },
}
```

- 任意时刻最多一个异常状态激活
- 异常叠加在正常信号之上：`output += anomaly_offset`
- 异常持续时间、幅度随机（在配置范围内）

### 默认配置

| 异常类型 | 概率 | 持续 tick 范围 | 幅度范围 |
|---|---|---|---|
| 缓慢漂移 | 1% | 30~120 | 0.1~0.5/tick |
| 尖峰突跳 | 2% | 1~3 | 1.5x~3x |
| 间歇性抖动 | 1% | 10~30 | ±2x 正常范围 |
| 信号卡死 | 1% | 20~60 | — |

- 总异常概率约 5%
- 可通过 `anomaly_probability` 全局缩放
- 可通过 `enable_anomaly` 开关

### 实现

文件：`simulated/anomaly.rs`

```rust
struct AnomalyEngine {
    state: AnomalyState,
    drift_probability: f64,
    spike_probability: f64,
    jitter_probability: f64,
    stuck_probability: f64,
}

impl AnomalyEngine {
    fn tick(&mut self, normal_value: f64, rng: &mut impl Rng) -> f64;
}
```

## 5. 基于标签的设备关联

### 原理

利用设备现有的 `tags` 字段（`Vec<Tag>`，通过 `tag_bindings` 表关联查询），约定同名标签的设备共享环境上下文。

### 示例

```
设备 A: tags = ["workshop_A", "production_line_1"]
设备 B: tags = ["workshop_A", "production_line_2"]
设备 C: tags = ["workshop_B", "production_line_1"]
```

- A 和 B 共享 `workshop_A` 的环境上下文（温度基准一起波动）
- A 和 C 共享 `production_line_1` 的环境上下文（负载一起变化）
- 一个设备可属于多个关联组（叠加效应）

### EnvironmentContext

按标签名全局缓存，一个标签对应一个环境上下文实例：

```rust
struct EnvironmentContext {
    tag_name: String,
    temperature_offset: f64,    // 该区域的温度基准偏移
    load_factor: f64,            // 负载系数 (0.0~1.0)，影响电流、功率
    voltage_offset: f64,         // 电压基准偏移
    phase_base: f64,             // 相位基准（作为组内设备 phase_offset 的基准）
    tick_offset: u64,            // 创建时刻的 tick 偏移
}
```

- 环境上下文创建时随机初始化参数
- 环境上下文本身也可以缓慢漂移（模拟区域温度变化）
- 同标签设备在该上下文基础上叠加各自的个体差异

### 配置

在驱动选项中可通过 `correlation_tags` 控制哪些标签参与关联：

```
correlation_tags = "*"          // 默认，所有标签参与关联
correlation_tags = "area_*,zone_*" // 只有匹配这些模式的标签参与
correlation_tags = ""           // 禁用关联
```

### 实现

文件：`simulated/correlation.rs`

```rust
struct CorrelationManager {
    contexts: HashMap<String, EnvironmentContext>,
}

impl CorrelationManager {
    fn get_or_create(&mut self, tag_name: &str, rng: &mut impl Rng) -> &EnvironmentContext;
}
```

使用全局单例（`LazyLock<Mutex<HashMap<String, EnvironmentContext>>>`），同进程内所有 SimulatedDriver 实例共享。

## 6. 代码结构

### 当前

```
crates/tinyiothub-runtime/src/driver/drivers/
├── mod.rs
├── simulated_driver.rs          // ~380 行，全部逻辑
├── modbus_driver.rs
└── snmp_driver.rs
```

### 升级后

```
crates/tinyiothub-runtime/src/driver/drivers/
├── mod.rs
├── simulated_driver.rs          // 驱动主体（read_data、execute_command、配置）~200 行
├── simulated/
│   ├── mod.rs                   // 模块导出
│   ├── signal.rs                // 信号合成引擎（公式计算、分量叠加）
│   ├── anomaly.rs               // 四种异常类型的状态机和生成逻辑
│   ├── patterns.rs              // 属性名 → 行为规则的匹配表
│   └── correlation.rs           // 标签关联：EnvironmentContext 缓存、组上下文
├── modbus_driver.rs
└── snmp_driver.rs
```

### 各模块职责

**`patterns.rs`** — 属性名模式匹配
- `PropertyBehavior` 结构体
- `match_property(name, data_type) -> PropertyBehavior`
- 内置 ~12 种属性名模式

**`signal.rs`** — 信号合成
- `SignalComposer` 结构体
- `compose(behavior, tick, rng, phase_offset, group_ctx) -> f64`

**`anomaly.rs`** — 异常注入
- `AnomalyEngine` 结构体
- `tick(&mut self, normal_value, rng) -> f64`（返回偏移量）

**`correlation.rs`** — 标签关联
- `EnvironmentContext` 结构体
- `CorrelationManager` 全局单例

**`simulated_driver.rs`** — 驱动主体（不膨胀）
- `read_data()` — 遍历 device.properties，调用 patterns → signal → anomaly → correlation 管道
- `execute_command()` — 命令执行（保持现有行为）
- 驱动配置读取
- `DeviceDriver` trait 实现

## 7. 配置体系

### 三级覆盖

```
全局默认（代码内置） → 驱动选项（driver_options） → 属性定义（device.properties）
```

### 驱动选项扩展

在现有 `#[driver_option]` 基础上新增：

| 配置项 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `interval` | number | 1000 | 刷新间隔 ms（已有） |
| `enable_periodic` | boolean | true | 启用日周期分量 |
| `enable_noise` | boolean | true | 启用噪声分量（已有类似） |
| `enable_anomaly` | boolean | true | 启用异常注入 |
| `anomaly_probability` | number | 0.05 | 异常总概率 |
| `daily_amplitude_scale` | number | 1.0 | 日波动幅度缩放 |
| `noise_level` | number | 1.0 | 噪声水平缩放 |
| `drift_rate` | number | 0.0 | 长期漂移速率 |
| `correlation_tags` | string | `*` | 参与关联的标签模式，空字符串禁用 |

### 属性级覆盖

`device.properties` 中已有的 `default_value`、`min_value`、`max_value` 字段可用于覆盖基准值和范围。

## 8. 向后兼容

- 接口不变：`DeviceDriver` trait 不变，`DriverWrapper`、`DataServer` 无需修改
- 配置兼容：现有 `driver_option` 全部保留，新选项有合理默认值
- 行为变化：数据变得更「规律」，但原有属性（temperature、humidity、power_status）的默认行为和当前保持一致
- 标签关联为可选功能：`correlation_tags = ""` 即可完全禁用

## 9. 不涉及的内容（本次不做）

- 设备生命周期模拟（在线/离线/故障/维护状态切换）—— 第二期
- 通信行为模拟（连接中断、延迟抖动、部分读取失败）—— 第二期
- 场景编排引擎（时间线驱动的多设备协同）—— 第二期
- 命令执行真实化（目前总是返回 true）—— 第二期
