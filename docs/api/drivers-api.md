# 驱动管理 API

## 概述

驱动管理 API 提供驱动的查询、配置以及动态加载/卸载功能。TinyIoTHub 支持内置驱动和动态加载驱动两种模式。

## 接口列表

### 获取驱动列表

```
GET /api/v1/drivers
```

获取所有可用的驱动列表（包括内置驱动和动态加载的驱动）。

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| name | string | 否 | 按名称模糊筛选 |

**响应示例：**

```json
{
  "success": true,
  "result": {
    "drivers": [
      {
        "id": "driver_modbus_tcp",
        "name": "modbus_tcp",
        "version": "1.0.0",
        "class_name": "ModbusTcpDriver",
        "device_num": 5,
        "description": "Modbus TCP 驱动，支持读写保持寄存器和输入寄存器",
        "options_descriptors": "[{\"name\":\"host\",\"type\":\"string\",\"required\":true},{\"name\":\"port\",\"type\":\"number\",\"default\":502}]",
        "location": "builtin",
        "created_at": "2024-01-01 10:00:00",
        "updated_at": "2024-01-01 10:00:00"
      },
      {
        "id": "driver_onvif",
        "name": "onvif",
        "version": "1.0.0",
        "class_name": "OnvifDriver",
        "device_num": 2,
        "description": "ONVIF 视频监控设备驱动",
        "options_descriptors": "[{\"name\":\"host\",\"type\":\"string\",\"required\":true},{\"name\":\"port\",\"type\":\"number\",\"default\":80}]",
        "location": "dynamic",
        "created_at": "2024-01-05 10:00:00",
        "updated_at": "2024-01-05 10:00:00"
      }
    ],
    "total": 8
  }
}
```

---

### 获取驱动名称列表

```
GET /api/v1/drivers/names
```

获取所有支持的驱动名称（不包含详细信息）。

**响应示例：**

```json
{
  "success": true,
  "result": {
    "drivers": [
      { "name": "modbus_rtu" },
      { "name": "modbus_tcp" },
      { "name": "onvif" },
      { "name": "snmp" },
      { "name": "ping" }
    ],
    "total": 5
  }
}
```

---

### 获取驱动详情

```
GET /api/v1/drivers/{name}
```

**路径参数：**

| 参数 | 类型 | 说明 |
|------|------|------|
| name | string | 驱动名称 |

**响应示例：**

```json
{
  "success": true,
  "result": {
    "driver": {
      "id": "driver_modbus_tcp",
      "name": "modbus_tcp",
      "version": "1.0.0",
      "class_name": "ModbusTcpDriver",
      "device_num": 5,
      "description": "Modbus TCP 驱动，支持读写保持寄存器和输入寄存器",
      "options_descriptors": "[{\"name\":\"host\",\"type\":\"string\",\"required\":true},{\"name\":\"port\",\"type\":\"number\",\"default\":502}]",
      "location": "builtin",
      "created_at": "2024-01-01 10:00:00",
      "updated_at": "2024-01-01 10:00:00"
    }
  }
}
```

---

### 获取驱动配置参数

```
GET /api/v1/drivers/{name}/config
```

获取指定驱动的配置参数定义和默认值。

**响应示例：**

```json
{
  "success": true,
  "result": {
    "driver_name": "modbus_tcp",
    "config_options": [
      {
        "name": "host",
        "display_name": "主机地址",
        "description": "Modbus TCP 设备 IP 地址",
        "type": "string",
        "required": true,
        "default_value": "192.168.1.1"
      },
      {
        "name": "port",
        "display_name": "端口",
        "description": "Modbus TCP 端口号",
        "type": "number",
        "required": false,
        "default_value": "502"
      },
      {
        "name": "slave_id",
        "display_name": "从机 ID",
        "description": "Modbus 从机地址",
        "type": "number",
        "required": false,
        "default_value": "1"
      },
      {
        "name": "timeout_ms",
        "display_name": "超时时间",
        "description": "通信超时时间（毫秒）",
        "type": "number",
        "required": false,
        "default_value": "5000"
      }
    ],
    "default_config": {
      "host": "192.168.1.1",
      "port": "502",
      "slave_id": "1",
      "timeout_ms": "5000"
    }
  }
}
```

---

### 检查驱动支持状态

```
GET /api/v1/drivers/{name}/supported
```

检查指定驱动是否被系统支持。

**响应示例：**

```json
{
  "success": true,
  "result": {
    "drivers": [],
    "total": 1
  }
}
```

---

### 动态加载驱动

```
POST /api/v1/drivers/dynamic/load
```

动态加载一个驱动模块（需要驱动文件已放置在 drivers 目录）。

**请求体：**

```json
{
  "driver_name": "custom_driver",
  "config": {}
}
```

**响应示例：**

```json
{
  "success": true,
  "result": "custom_driver"
}
```

---

### 动态卸载驱动

```
DELETE /api/v1/drivers/dynamic/{name}/unload
```

卸载一个已加载的动态驱动。

**响应示例：**

```json
{
  "success": true,
  "result": true
}
```

---

### 列出所有动态驱动

```
GET /api/v1/drivers/dynamic/list
```

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "name": "onvif",
      "status": "loaded",
      "loaded_at": "2024-01-05 10:00:00"
    },
    {
      "name": "snmp",
      "status": "loaded",
      "loaded_at": "2024-01-05 10:00:00"
    }
  ]
}
```

---

### 重新加载驱动目录

```
POST /api/v1/drivers/dynamic/reload
```

重新扫描并加载 drivers 目录下的所有驱动。

## 支持的内置驱动

| 驱动名称 | 协议 | 说明 |
|----------|------|------|
| modbus_rtu | 串口 | Modbus RTU 协议（RS-485/RS-232） |
| modbus_tcp | 网络 | Modbus TCP 协议 |
| onvif | 网络 | ONVIF 视频监控设备 |
| snmp | 网络 | SNMP 协议网络设备监控 |
| ping | 网络 | ICMP Ping 主机存活检测 |

## 驱动配置示例

### Modbus TCP 配置

```json
{
  "host": "192.168.1.100",
  "port": 502,
  "slave_id": 1,
  "timeout_ms": 5000,
  "retry_count": 3
}
```

### ONVIF 配置

```json
{
  "host": "192.168.1.200",
  "port": 80,
  "username": "admin",
  "password": "admin123"
}
```

### SNMP 配置

```json
{
  "host": "192.168.1.50",
  "port": 161,
  "community": "public",
  "version": "v2c",
  "timeout_ms": 3000
}
```

### Ping 配置

```json
{
  "host": "192.168.1.1",
  "interval_ms": 5000,
  "timeout_ms": 3000,
  "failure_threshold": 3
}
```

## 错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 400 | 请求参数错误 |
| 404 | 驱动不存在 |
| 500 | 驱动加载失败 |
