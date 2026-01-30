# 设备Profile接口文档

## 概述

设备Profile接口提供了获取指定设备完整信息的功能，包括设备基本信息、属性列表、指令列表、最近事件和统计信息。这是一个综合性的接口，适用于设备详情页面的数据展示。

## 接口信息

- **路径**: `GET /api/v1/devices/{device_id}/profile`
- **认证**: 需要JWT认证
- **权限**: 需要设备查看权限

## 请求参数

### 路径参数

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| device_id | string | 是 | 设备ID |

## 响应格式

### 成功响应 (200 OK)

```json
{
  "success": true,
  "result": {
    "device": {
      "id": "device_001",
      "name": "温度传感器01",
      "display_name": "车间温度传感器",
      "description": "用于监测车间环境温度",
      "device_type": "sensor",
      "protocol": "modbus",
      "connection_string": "192.168.1.100:502",
      "is_enabled": true,
      "created_at": "2024-01-01 10:00:00",
      "updated_at": "2024-01-07 15:30:00"
    },
    "is_online": true,
    "properties": [
      {
        "id": "prop_001",
        "device_id": "device_001",
        "name": "temperature",
        "display_name": "温度",
        "description": "当前环境温度",
        "data_type": "float",
        "unit": "°C",
        "min_value": -40.0,
        "max_value": 100.0,
        "default_value": "25.0",
        "is_read_only": 1,
        "created_at": "2024-01-01 10:00:00",
        "current_value": "23.5",
        "last_update_time": "2024-01-07 15:30:00",
        "alarm_status": 0
      }
    ],
    "commands": [
      {
        "id": "cmd_001",
        "device_id": "device_001",
        "name": "reset",
        "display_name": "重置设备",
        "description": "重置设备到默认状态",
        "parameters": "{\"confirm\": \"boolean\"}",
        "created_at": "2024-01-01 10:00:00"
      }
    ],
    "recent_events": [
      {
        "id": "event_001",
        "device_id": "device_001",
        "event_type": "alarm",
        "level": "error",
        "title": "温度过高告警",
        "message": "设备温度超过阈值 85°C，当前温度 92°C",
        "data": {
          "temperature": 92.0,
          "threshold": 85.0,
          "unit": "°C"
        },
        "source": "temperature_sensor",
        "created_at": "2024-01-07 15:30:00",
        "acknowledged_at": null,
        "resolved_at": null,
        "status": "active"
      }
    ],
    "statistics": {
      "total_properties": 12,
      "online_properties": 10,
      "offline_properties": 2,
      "readonly_properties": 8,
      "writable_properties": 4,
      "total_commands": 5,
      "total_events": 25,
      "active_alarms": 2,
      "last_update_time": "2024-01-07 15:30:00"
    },
    "generated_at": "2024-01-07 16:00:00"
  }
}
```

### 错误响应

#### 设备不存在 (200 OK)
```json
{
  "success": false,
  "message": "设备不存在"
}
```

#### 认证失败 (401 Unauthorized)
```json
{
  "success": false,
  "message": "认证失败"
}
```

#### 权限不足 (403 Forbidden)
```json
{
  "success": false,
  "message": "权限不足"
}
```

## 数据结构说明

### DeviceProfile

| 字段名 | 类型 | 描述 |
|--------|------|------|
| device | Device | 设备基本信息 |
| is_online | boolean | 设备在线状态 |
| properties | DeviceProperty[] | 设备属性列表 |
| commands | DeviceCommand[] | 设备指令列表 |
| recent_events | DeviceEvent[] | 最近事件列表（最多5条） |
| statistics | DeviceProfileStatistics | 设备统计信息 |
| generated_at | string | 配置文件生成时间 |

### DeviceProfileStatistics

| 字段名 | 类型 | 描述 |
|--------|------|------|
| total_properties | number | 属性总数 |
| online_properties | number | 在线属性数 |
| offline_properties | number | 离线属性数 |
| readonly_properties | number | 只读属性数 |
| writable_properties | number | 可写属性数 |
| total_commands | number | 指令总数 |
| total_events | number | 事件总数 |
| active_alarms | number | 活跃告警数 |
| last_update_time | string? | 最后更新时间 |

## 使用场景

1. **设备详情页面**: 一次性获取设备的完整信息，用于展示设备概览
2. **设备快照**: 生成设备当前状态的快照，用于报告或备份
3. **设备诊断**: 获取设备的完整状态信息，用于故障诊断
4. **设备配置导出**: 导出设备的完整配置信息

## 性能考虑

- 该接口会查询多个数据表，响应时间可能较长
- 建议在需要完整设备信息时使用，避免频繁调用
- 对于实时数据更新，建议使用单独的属性或事件接口

## 示例代码

### JavaScript/TypeScript

```typescript
async function getDeviceProfile(deviceId: string): Promise<DeviceProfile> {
  const response = await fetch(`/api/v1/devices/${deviceId}/profile`, {
    headers: {
      'Authorization': `Bearer ${token}`,
      'Content-Type': 'application/json'
    }
  });
  
  const data = await response.json();
  
  if (!data.success) {
    throw new Error(data.message);
  }
  
  return data.result;
}

// 使用示例
try {
  const profile = await getDeviceProfile('device_001');
  console.log(`设备 ${profile.device.name} 有 ${profile.properties.length} 个属性`);
  console.log(`在线状态: ${profile.is_online ? '在线' : '离线'}`);
} catch (error) {
  console.error('获取设备配置文件失败:', error.message);
}
```

### Rust

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct ApiResponse<T> {
    success: bool,
    result: Option<T>,
    message: Option<String>,
}

async fn get_device_profile(client: &reqwest::Client, device_id: &str) -> Result<DeviceProfile, Box<dyn std::error::Error>> {
    let url = format!("/api/v1/devices/{}/profile", device_id);
    
    let response: ApiResponse<DeviceProfile> = client
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await?
        .json()
        .await?;
    
    if response.success {
        Ok(response.result.unwrap())
    } else {
        Err(response.message.unwrap_or("Unknown error".to_string()).into())
    }
}
```

## 更新日志

- **v1.0.0** (2024-01-07): 初始版本，支持基本的设备配置文件获取功能