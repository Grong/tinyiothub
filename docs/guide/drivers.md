# 驱动管理

驱动（Driver）是 TinyIoTHub 与各种设备通信的桥梁，每种通信协议对应一个驱动模块。系统支持内置驱动和动态加载驱动两种模式。

## 内置驱动

### 支持的协议

| 驱动 | 协议类型 | 说明 |
|------|----------|------|
| modbus_rtu | 串口 | Modbus RTU 协议（RS-485/RS-232） |
| modbus_tcp | 网络 | Modbus TCP 协议 |
| onvif | 网络 | ONVIF 视频监控设备发现和控制 |
| snmp | 网络 | SNMP 协议网络设备监控 |
| ping | 网络 | ICMP Ping 主机存活检测 |

### Modbus RTU

**适用场景**：通过串口连接的工业设备，如 PLC、仪表、传感器。

**配置参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| port | string | 是 | 串口名称（如 COM1、/dev/ttyUSB0） |
| baud_rate | number | 是 | 波特率（9600、19200、115200） |
| data_bits | number | 否 | 数据位（默认 8） |
| stop_bits | number | 否 | 停止位（默认 1） |
| parity | string | 否 | 校验位（none/odd/even） |
| slave_id | number | 否 | 从机地址（默认 1） |
| timeout_ms | number | 否 | 超时时间（默认 5000ms） |

### Modbus TCP

**适用场景**：通过网络连接的 Modbus 设备，如工业控制器、仪表。

**配置参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| host | string | 是 | 设备 IP 地址 |
| port | number | 是 | 端口号（默认 502） |
| slave_id | number | 否 | 从机地址（默认 1） |
| timeout_ms | number | 否 | 超时时间（默认 5000ms） |
| retry_count | number | 否 | 重试次数（默认 3） |

### ONVIF

**适用场景**：IP 摄像头、NVR 设备。

**配置参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| host | string | 是 | 摄像头 IP 地址 |
| port | number | 否 | 端口号（默认 80） |
| username | string | 是 | 用户名 |
| password | string | 是 | 密码 |

**功能支持：**
- 设备发现（WS-Discovery）
- 获取设备信息
- 获取音视频流地址
- 云台控制（PTZ）
- 快照获取

### SNMP

**适用场景**：网络设备监控，如交换机、路由器、服务器。

**配置参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| host | string | 是 | 设备 IP 地址 |
| port | number | 否 | SNMP 端口（默认 161） |
| community | string | 否 | 社区名（默认 public） |
| version | string | 否 | SNMP 版本（v1/v2c/v3） |
| timeout_ms | number | 否 | 超时时间（默认 3000ms） |

### Ping

**适用场景**：主机存活检测、网络诊断。

**配置参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| host | string | 是 | 目标 IP 或主机名 |
| interval_ms | number | 否 | 检测间隔（默认 5000ms） |
| timeout_ms | number | 否 | 超时时间（默认 3000ms） |
| failure_threshold | number | 否 | 失败阈值（连续失败次数触发告警） |

## 自定义驱动

TinyIoTHub 支持通过动态加载方式添加自定义驱动。

### 开发自定义驱动

1. 在 `api/drivers/` 目录下创建驱动模块
2. 实现驱动接口（实现连接、读写、断开等方法）
3. 编译驱动为动态库
4. 将驱动文件放入 `drivers/` 目录
5. 调用动态加载 API 或重启系统

### 驱动接口要求

```rust
pub trait Driver {
    fn name(&self) -> &str;
    fn connect(&mut self, config: &Value) -> Result<()>;
    fn disconnect(&mut self) -> Result<()>;
    fn read(&mut self, address: &str) -> Result<Value>;
    fn write(&mut self, address: &str, value: &Value) -> Result<()>;
}
```

### 动态加载驱动 API

```http
POST /api/v1/drivers/dynamic/load
Content-Type: application/json

{
  "driver_name": "custom_driver"
}
```

```http
DELETE /api/v1/drivers/dynamic/{name}/unload
```

```http
GET /api/v1/drivers/dynamic/list
```

## 驱动状态

| 状态 | 标识 | 说明 |
|------|------|------|
| 已加载 | 🟢 | 驱动正常工作 |
| 未加载 | ⚪ | 驱动未启用 |
| 加载失败 | 🔴 | 驱动初始化失败 |
| 重试中 | 🟡 | 连接失败，正在重试 |

## 驱动配置

### 在设备中使用驱动

创建设备时选择对应的驱动类型，并填写驱动配置：

```json
{
  "driver_name": "modbus_tcp",
  "connection_config": {
    "host": "192.168.1.100",
    "port": 502,
    "slave_id": 1,
    "timeout_ms": 5000
  }
}
```

### 查看驱动配置参数

```http
GET /api/v1/drivers/{name}/config
```

## 驱动日志

驱动运行日志可在「系统日志」中查看：

| 日志级别 | 说明 |
|----------|------|
| ERROR | 驱动错误，需要处理 |
| WARN | 驱动警告，可能存在问题 |
| INFO | 正常运行信息 |
| DEBUG | 调试信息 |

**查看驱动日志：**

```http
GET /api/v1/monitoring/logs?source=driver&name=modbus_tcp
```

## 常见问题

**Q：设备通信失败怎么排查？**
1. 确认 IP 地址和端口配置正确
2. 检查网络连通性（ping 测试）
3. 确认设备支持的协议版本
4. 检查防火墙设置

**Q：Modbus 读取数据不正确？**
1. 确认寄存器地址映射正确
2. 检查数据类型（16位整数 vs 32位浮点）
3. 确认字节序（大端/小端）设置
4. 验证寄存器地址偏移

**Q：如何开发新的驱动？**
1. 参考现有驱动的代码结构
2. 实现 `Driver` trait 接口
3. 注册驱动到驱动管理器
4. 编写单元测试
5. 在测试环境验证后提交
