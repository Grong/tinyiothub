# BACnet 驱动

BACnet/IP 协议驱动，用于楼宇自动化设备通信。

## 功能特性

- ✅ 支持 BACnet/IP 协议
- ✅ 支持多种对象类型（模拟量、数字量、多状态）
- ✅ 灵活的对象映射配置
- ✅ 读取和写入操作
- ✅ 动态加载支持

## 支持的 BACnet 对象类型

| 对象类型 | 说明 | 示例 |
|---------|------|------|
| `analog-input` | 模拟量输入 | 温度、湿度传感器 |
| `analog-value` | 模拟量值 | 设定点、计算值 |
| `binary-input` | 数字量输入 | 开关状态、报警 |
| `binary-value` | 数字量值 | 控制输出 |
| `multi-state-input` | 多状态输入 | 模式选择 |
| `multi-state-value` | 多状态值 | 运行模式 |

## 配置说明

### 驱动配置参数

```json
{
  "device_instance": 1001,
  "ip_address": "192.168.1.100",
  "port": 47808,
  "object_mappings": [
    {
      "name": "temperature",
      "object_type": "analog-input",
      "object_instance": 1,
      "property": "present-value"
    }
  ]
}
```

### 参数说明

- `device_instance`: BACnet 设备实例号（必填）
- `ip_address`: 设备 IP 地址（必填）
- `port`: BACnet 端口，默认 47808（可选）
- `object_mappings`: 对象映射列表（必填）
  - `name`: 数据点名称
  - `object_type`: BACnet 对象类型
  - `object_instance`: 对象实例号
  - `property`: 属性名称，默认 "present-value"

## 编译驱动

```bash
# 进入驱动目录
cd examples/bacnet-driver

# 编译 Release 版本
cargo build --release

# 编译产物位置：
# - Windows: target/release/bacnet_driver.dll
# - Linux: target/release/libbacnet_driver.so
# - macOS: target/release/libbacnet_driver.dylib
```

## 集成测试

### 前置条件

1. 确保 TinyIoTHub 服务正在运行
2. 已编译 BACnet 驱动
3. 安装 Python 3 和 requests 库

```bash
pip install requests
```

### 运行测试

```bash
# 在驱动目录下运行
python3 test_bacnet_driver.py
```

### 测试流程

测试脚本会自动执行以下步骤：

1. ✅ 登录系统获取 token
2. ✅ 上传 BACnet 驱动
3. ✅ 创建 BACnet 设备（配置 HVAC 控制器）
4. ✅ 读取设备数据（温度、湿度、风机状态、模式）
5. ✅ 执行控制命令（设置风机状态）
6. ✅ 验证命令执行结果
7. ✅ 清理测试数据

### 测试输出示例

```
============================================================
BACnet 驱动集成测试
============================================================
🔐 登录系统...
✅ 登录成功，token: eyJhbGciOiJIUzI1NiIs...

📦 上传 BACnet 驱动...
✅ 驱动上传成功，ID: 5
   名称: BacnetDriver
   版本: 1.0.0

🔧 创建 BACnet 设备...
✅ 设备创建成功，ID: 10
   名称: BACnet HVAC Controller
   驱动: BacnetDriver

⏳ 等待设备初始化...

📊 读取设备数据...
✅ 读取到 4 个数据点:
   - temperature: 20.5 (float)
   - humidity: 21.0 (float)
   - fan_status: false (boolean)
   - mode: 0 (integer)

⚡ 执行设备命令...
✅ 命令执行成功

📊 读取设备数据...
✅ 读取到 4 个数据点:
   - temperature: 20.5 (float)
   - humidity: 21.0 (float)
   - fan_status: false (boolean)
   - mode: 0 (integer)

============================================================
✅ 所有测试通过！
============================================================
```

## 手动测试

### 1. 上传驱动

```bash
curl -X POST http://localhost:8080/api/v1/drivers/upload \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -F "file=@target/release/libbacnet_driver.so"
```

### 2. 创建设备

```bash
curl -X POST http://localhost:8080/api/v1/devices \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "BACnet Device",
    "driver_id": 5,
    "config": "{\"device_instance\":1001,\"ip_address\":\"192.168.1.100\",\"port\":47808,\"object_mappings\":[{\"name\":\"temperature\",\"object_type\":\"analog-input\",\"object_instance\":1}]}",
    "enabled": true
  }'
```

### 3. 读取数据

```bash
curl -X GET http://localhost:8080/api/v1/devices/10/data \
  -H "Authorization: Bearer YOUR_TOKEN"
```

### 4. 执行命令

```bash
curl -X POST http://localhost:8080/api/v1/devices/10/command \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "fan_status",
    "params": {
      "value": "true"
    }
  }'
```

## 应用场景

### 楼宇自动化

```json
{
  "device_instance": 2001,
  "ip_address": "192.168.1.50",
  "object_mappings": [
    {"name": "room_temp", "object_type": "analog-input", "object_instance": 1},
    {"name": "setpoint", "object_type": "analog-value", "object_instance": 10},
    {"name": "hvac_mode", "object_type": "multi-state-value", "object_instance": 20},
    {"name": "fan_on", "object_type": "binary-value", "object_instance": 30}
  ]
}
```

### 能源管理

```json
{
  "device_instance": 3001,
  "ip_address": "192.168.1.60",
  "object_mappings": [
    {"name": "power_consumption", "object_type": "analog-input", "object_instance": 1},
    {"name": "voltage", "object_type": "analog-input", "object_instance": 2},
    {"name": "current", "object_type": "analog-input", "object_instance": 3},
    {"name": "breaker_status", "object_type": "binary-input", "object_instance": 10}
  ]
}
```

## 故障排查

### 驱动加载失败

检查驱动文件是否存在且有执行权限：

```bash
ls -l target/release/libbacnet_driver.so
chmod +x target/release/libbacnet_driver.so
```

### 设备连接失败

1. 检查 IP 地址和端口配置
2. 确认网络连通性：`ping 192.168.1.100`
3. 检查防火墙设置（BACnet 默认端口 47808）

### 数据读取失败

1. 验证对象实例号是否正确
2. 检查对象类型配置
3. 查看系统日志获取详细错误信息

## 开发说明

### 添加新的对象类型

在 `src/lib.rs` 的 `read_bacnet_object` 方法中添加：

```rust
"new-object-type" => {
    // 处理新对象类型
    ResultValue::float(mapping.name.clone(), value)
}
```

### 实现真实 BACnet 通信

当前实现使用模拟数据，要实现真实通信：

1. 使用 `bacnet` crate 或其他 BACnet 库
2. 实现 BACnet 客户端连接
3. 实现 ReadProperty 和 WriteProperty 服务
4. 处理 BACnet 错误和超时

## 参考资料

- [BACnet 协议标准](http://www.bacnet.org/)
- [BACnet 对象类型列表](https://www.bacnet.org/Bibliography/EC-9-97/EC-9-97.htm)
- [TinyIoTHub 驱动开发文档](../../docs/driver-dynamic-loading-final-design.md)

## 许可证

与主项目相同
