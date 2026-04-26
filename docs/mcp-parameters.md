> ⚠️ **已弃用**：本文档描述的独立 `mcp/` crate 参数设计已过时。MCP Server 现已内嵌到 `api/src/api/mcp/` 中，直接调用内部服务。本文件仅保留供历史参考。

# TinyIoTHub MCP 协议支持 - 详细参数设计

> 基于现有 TinyIoTHub DTO 结构复用设计

## 一、现有 DTO 结构复用

### 1.1 引用现有结构

| 现有 DTO | 文件位置 | MCP 用途 |
|----------|----------|----------|
| `Device` | `dto/entity/device.rs` | 设备信息 |
| `DeviceProperty` | `dto/entity/device_property.rs` | 设备属性（含运行时 current_value） |
| `DeviceQueryParams` | `dto/entity/device.rs` | 设备查询参数 |
| `CreateDeviceRequest` | `dto/entity/device.rs` | 创建设备请求 |
| `AlarmDto` | `dto/entity/alarm.rs` | 告警信息 |
| `AlarmStatisticsDto` | `dto/entity/alarm.rs` | 告警统计 |
| `ApiResponse<T>` | `dto/response/api_response.rs` | 统一响应格式 |
| `DeviceCommand` | `dto/entity/device_command.rs` | 设备命令 |
| `PaginationQuery` | `dto/request/pagination.rs` | 分页参数 |

### 1.2 统一响应格式

复用 `ApiResponse<T>` 结构：

```rust
// 现有结构：dto/response/api_response.rs
pub struct ApiResponse<T> {
    pub msg: String,     // 错误信息，成功时为空字符串
    pub code: i32,       // 0=成功，-1=失败
    pub result: Option<T> // 实际数据，失败时为 null
}
```

**MCP 返回转换**：

| TinyIoTHub API 返回 | MCP 返回 |
|---------------------|----------|
| `{"code": 0, "msg": "", "result": [...]}` | `{"success": true, "data": [...]}` |
| `{"code": -1, "msg": "error", "result": null}` | `{"success": false, "error": "error"}` |

---

## 二、工具参数与返回设计

### 2.1 search_devices - 搜索设备

**功能**：通过关键词搜索设备，支持按标签过滤，返回精简结果以节省 token。

**MCP 输入参数**（JSON Schema）：

```json
{
  "name": "search_devices",
  "description": "通过关键词搜索设备，支持按标签过滤",
  "inputSchema": {
    "type": "object",
    "required": ["keyword"],
    "properties": {
      "keyword": {
        "type": "string",
        "description": "搜索关键词（在 name、display_name、address、description 中模糊匹配）"
      },
      "tag": {
        "type": "string",
        "description": "按标签名称过滤（部分匹配）"
      },
      "limit": {
        "type": "integer",
        "description": "最大返回数量",
        "default": 20,
        "minimum": 1,
        "maximum": 50
      }
    }
  }
}
```

**返回结构**：`SearchDevicesResponse`（精简字段，节省 token）

```json
{
  "msg": "",
  "code": 0,
  "result": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "name": "temp_sensor_001",
      "display_name": "温度传感器 1 号",
      "device_type": "sensor",
      "address": "192.168.1.100:502",
      "description": "仓库温度监测",
      "position": "仓库A区",
      "driver_name": "modbus_tcp",
      "device_model": "DHT22",
      "protocol_type": "modbus",
      "state": 1,
      "is_online": true,
      "last_heartbeat": "2026-03-15 09:15:30",
      "properties": [
        {
          "id": "prop_001",
          "device_id": "550e8400-e29b-41d4-a716-446655440000",
          "name": "temperature",
          "display_name": "温度",
          "data_type": "float",
          "unit": "℃",
          "current_value": "25.6"
        }
      ],
      "created_at": "2026-01-15 10:00:00",
      "updated_at": "2026-03-15 09:15:30"
    }
  ]
}
```

**MCP 转换后**：

```json
{
  "success": true,
  "data": [/* 同上 result 数组 */],
  "pagination": { "page": 1, "page_size": 20, "total": 15 }
}
```

---

### 2.2 get_device - 获取设备详情

**复用 DTO**：`Device`

**MCP 输入参数**：

```json
{
  "name": "get_device",
  "description": "获取单个设备的完整详细信息",
  "inputSchema": {
    "type": "object",
    "properties": {
      "device_id": {
        "type": "string",
        "description": "设备唯一标识（UUID）或名称"
      },
      "include_properties": { "type": "boolean", "default": true },
      "include_commands": { "type": "boolean", "default": true }
    },
    "required": ["device_id"]
  }
}
```

**API 调用**：
```
GET /api/v1/devices/{device_id}?include_properties=true&include_commands=true
```

**复用返回结构**：`ApiResponse<Device>`

---

### 2.3 get_device_status - 获取设备状态

**复用 DTO**：部分 `Device` 字段

**MCP 输入参数**：

```json
{
  "name": "get_device_status",
  "description": "快速获取设备的在线状态",
  "inputSchema": {
    "type": "object",
    "properties": {
      "device_id": { "type": "string", "description": "设备唯一标识（UUID）或名称" }
    },
    "required": ["device_id"]
  }
}
```

**复用返回结构**：`Device` 中的实时字段

```json
{
  "success": true,
  "data": {
    "device_id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "temp_sensor_001",
    "state": 1,
    "is_online": true,
    "last_heartbeat": "2026-03-15 09:15:30"
  }
}
```

---

### 2.4 read_sensor_data - 读取传感器数据

**复用 DTO**：`DeviceProperty`（运行时字段 `current_value`）

**MCP 输入参数**：

```json
{
  "name": "read_sensor_data",
  "description": "读取传感器的实时数据",
  "inputSchema": {
    "type": "object",
    "properties": {
      "device_id": { "type": "string", "description": "设备唯一标识" },
      "properties": {
        "type": "array",
        "items": { "type": "string" },
        "description": "要读取的属性名称列表，如 [\"temperature\", \"humidity\"]"
      },
      "timeout_ms": { "type": "integer", "default": 5000 }
    },
    "required": ["device_id"]
  }
}
```

**API 调用**：
```
POST /api/v1/devices/{device_id}/properties/read
Content-Type: application/json
```

**需要新增 API**：批量读取属性，返回 `Vec<DeviceProperty>`

**复用返回结构**：`ApiResponse<Vec<DeviceProperty>>`

```json
{
  "msg": "",
  "code": 0,
  "result": [
    {
      "id": "prop_001",
      "device_id": "550e8400-e29b-41d4-a716-446655440000",
      "name": "temperature",
      "display_name": "温度",
      "data_type": "float",
      "unit": "℃",
      "current_value": "25.6",
      "alarm_status": 0
    }
  ]
}
```

---

### 2.5 send_command - 发送控制命令

**复用 DTO**：`DeviceCommand`

**MCP 输入参数**：

```json
{
  "name": "send_command",
  "description": "向设备发送控制命令",
  "inputSchema": {
    "type": "object",
    "properties": {
      "device_id": { "type": "string" },
      "command": { "type": "string", "description": "命令名称（复用 DeviceCommand.name）" },
      "parameters": { "type": "object", "description": "命令参数（复用 DeviceCommand.parameters）" },
      "timeout_ms": { "type": "integer", "default": 10000 }
    },
    "required": ["device_id", "command"]
  }
}
```

**API 调用**：
```
POST /api/v1/devices/{device_id}/commands/execute
```

**复用返回结构**：现有命令执行响应

---

### 2.6 list_alarms - 获取告警列表

**复用 DTO**：`AlarmDto`

**MCP 输入参数**：

```json
{
  "name": "list_alarms",
  "description": "列出告警事件",
  "inputSchema": {
    "type": "object",
    "properties": {
      "status": {
        "type": "string",
        "enum": ["active", "acknowledged", "resolved", "all"],
        "default": "active"
      },
      "device_id": { "type": "string" },
      "severity": { "type": "string" },
      "limit": { "type": "integer", "default": 20 },
      "offset": { "type": "integer", "default": 0 }
    }
  }
}
```

**API 调用**：
```
GET /api/v1/alarms?status=active&limit=20
```

**复用返回结构**：`ApiResponse<Vec<AlarmDto>>`

```json
{
  "msg": "",
  "code": 0,
  "result": [
    {
      "id": "alarm_001",
      "device_id": "550e8400-e29b-41d4-a716-446655440000",
      "device_name": "temp_sensor_001",
      "alarm_type": "threshold",
      "alarm_level": "warning",
      "message": "温度超过阈值",
      "alarm_value": "35",
      "threshold_value": "30",
      "status": "active",
      "is_acknowledged": false,
      "created_at": "2026-03-15T09:10:00Z"
    }
  ]
}
```

---

### 2.7 acknowledge_alarm - 确认告警

**复用 DTO**：`AlarmDto`

**MCP 输入参数**：

```json
{
  "name": "acknowledge_alarm",
  "description": "确认告警",
  "inputSchema": {
    "type": "object",
    "properties": {
      "alarm_id": { "type": "string" },
      "comment": { "type": "string" }
    },
    "required": ["alarm_id"]
  }
}
```

**API 调用**：
```
POST /api/v1/alarms/{alarm_id}/acknowledge
```

**复用返回结构**：`ApiResponse<AlarmDto>`

---

### 2.8 get_alarm_statistics - 获取告警统计

**复用 DTO**：`AlarmStatisticsDto`

**MCP 输入参数**：

```json
{
  "name": "get_alarm_statistics",
  "description": "获取告警统计信息",
  "inputSchema": {
    "type": "object",
    "properties": {
      "time_range": { "type": "string", "enum": ["today", "week", "month", "all"], "default": "today" }
    }
  }
}
```

**API 调用**：
```
GET /api/v1/alarms/statistics
```

**复用返回结构**：`ApiResponse<AlarmStatisticsDto>`

```json
{
  "msg": "",
  "code": 0,
  "result": {
    "total_count": 25,
    "active_count": 3,
    "acknowledged_count": 12,
    "resolved_count": 10
  }
}
```

---

### 2.9 query_device_history - 查询设备历史数据

**MCP 输入参数**：

```json
{
  "name": "query_device_history",
  "description": "查询设备历史数据",
  "inputSchema": {
    "type": "object",
    "properties": {
      "device_id": { "type": "string" },
      "property": { "type": "string", "description": "属性名称" },
      "start_time": { "type": "string", "description": "开始时间 ISO8601" },
      "end_time": { "type": "string", "description": "结束时间 ISO8601" },
      "interval": { "type": "string", "enum": ["raw", "1m", "5m", "15m", "1h", "1d"], "default": "raw" },
      "limit": { "type": "integer", "default": 100 }
    },
    "required": ["device_id", "start_time", "end_time"]
  }
}
```

**需要新增 API**：`GET /api/v1/devices/{id}/history`

**返回结构**（新设计，复用类似结构）：

```json
{
  "msg": "",
  "code": 0,
  "result": {
    "device_id": "xxx",
    "property": "temperature",
    "records": [
      { "timestamp": "2026-03-14 00:00:00", "value": "22.5" },
      { "timestamp": "2026-03-14 00:05:00", "value": "22.6" }
    ],
    "statistics": {
      "count": 288,
      "min": 20.1,
      "max": 28.5,
      "avg": 24.3
    }
  }
}
```

---

### 2.10 list_drivers - 获取驱动列表

**MCP 输入参数**：

```json
{
  "name": "list_drivers",
  "description": "列出所有可用的设备驱动",
  "inputSchema": {
    "type": "object",
    "properties": {
      "protocol_type": { "type": "string" }
    }
  }
}
```

**API 调用**：
```
GET /api/v1/drivers
```

**复用返回结构**：现有驱动列表

---

## 三、API 对照表

| MCP 工具 | HTTP 方法 | API 路径 | 复用 DTO | 状态 |
|----------|-----------|----------|----------|------|
| search_devices | GET | /api/v1/devices | `Device`, `DeviceQueryParams` | ✅ 已有 |
| get_device | GET | /api/v1/devices/{id} | `Device` | ✅ 已有 |
| get_device_status | GET | /api/v1/devices/{id} | `Device` 部分字段 | ✅ 已有 |
| read_sensor_data | POST | /api/v1/devices/{id}/properties/read | `DeviceProperty` | 🆕 需新增 |
| send_command | POST | /api/v1/devices/{id}/commands/execute | `DeviceCommand` | ✅ 已有 |
| list_alarms | GET | /api/v1/alarms | `AlarmDto` | ✅ 已有 |
| acknowledge_alarm | POST | /api/v1/alarms/{id}/acknowledge | `AlarmDto` | ✅ 已有 |
| get_alarm_statistics | GET | /api/v1/alarms/statistics | `AlarmStatisticsDto` | ✅ 已有 |
| query_device_history | GET | /api/v1/devices/{id}/history | 新设计 | 🆕 需新增 |
| list_drivers | GET | /api/v1/drivers | 驱动信息 | ✅ 已有 |

---

## 四、错误响应格式

复用现有 `ApiResponse<T>::error()` ：

```json
{
  "msg": "设备不存在",
  "code": -1,
  "result": null
}
```

**MCP 转换**：

```json
{
  "success": false,
  "error": {
    "code": -1,
    "message": "设备不存在"
  }
}
```

---

## 五、MCP 工具定义文件

基于现有 DTO，生成 MCP 工具定义：

```typescript
// mcp/tools/device.ts
import { Device, DeviceQueryParams } from './dto/entity/device';
import { DeviceProperty } from './dto/entity/device_property';
import { AlarmDto } from './dto/entity/alarm';
import { ApiResponse } from './dto/response/api_response';

export const tools = {
  search_devices: {
    name: 'search_devices',
    description: '列出所有 IoT 设备，支持分页和过滤',
    inputSchema: {
      type: 'object' as const,
      properties: {
        page: { type: 'integer', default: 1 },
        page_size: { type: 'integer', default: 20 },
        name: { type: 'string' },
        device_type: { type: 'string' },
        driver_name: { type: 'string' },
        state: { type: 'integer' },
        include_properties: { type: 'boolean', default: false }
      }
    },
    responseSchema: {} as ApiResponse<Device[]>
  },
  
  read_sensor_data: {
    name: 'read_sensor_data',
    description: '读取传感器的实时数据',
    inputSchema: {
      type: 'object' as const,
      properties: {
        device_id: { type: 'string' },
        properties: { type: 'array', items: { type: 'string' } },
        timeout_ms: { type: 'integer', default: 5000 }
      },
      required: ['device_id']
    },
    responseSchema: {} as ApiResponse<DeviceProperty[]>
  },
  
  list_alarms: {
    name: 'list_alarms',
    description: '列出告警事件',
    inputSchema: {
      type: 'object' as const,
      properties: {
        status: { type: 'string', enum: ['active', 'acknowledged', 'resolved', 'all'] },
        device_id: { type: 'string' },
        limit: { type: 'integer', default: 20 }
      }
    },
    responseSchema: {} as ApiResponse<AlarmDto[]>
  }
};
```

---

*详细设计完成日期：2026-03-15*
