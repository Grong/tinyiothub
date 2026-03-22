# 设备模板 API

## 概述

设备模板 API 提供设备模板的增删改查功能。设备模板预先定义了设备的驱动类型、配置参数、数据点和属性映射，可以快速创建设备。

## 接口列表

### 获取模板列表

```
GET /api/v1/device-templates
```

获取所有设备模板。

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| name | string | 否 | 按名称模糊筛选 |
| driver_name | string | 否 | 按驱动名称筛选 |
| category | string | 否 | 按分类筛选 |
| page | number | 否 | 页码，默认 1 |
| page_size | number | 否 | 每页数量，默认 20 |

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "tmpl_001",
      "name": "Modbus TCP 温湿度传感器",
      "description": "标准 Modbus TCP 协议温湿度传感器模板",
      "category": "sensor",
      "driver_name": "modbus_tcp",
      "version": "1.0.0",
      "device_count": 12,
      "thumbnail": null,
      "tags": ["temperature", "humidity", "modbus"],
      "created_at": "2024-01-01 10:00:00",
      "updated_at": "2024-01-05 10:00:00"
    }
  ]
}
```

---

### 获取模板详情

```
GET /api/v1/device-templates/{id}
```

**响应示例：**

```json
{
  "success": true,
  "result": {
    "id": "tmpl_001",
    "name": "Modbus TCP 温湿度传感器",
    "description": "标准 Modbus TCP 协议温湿度传感器模板",
    "category": "sensor",
    "driver_name": "modbus_tcp",
    "version": "1.0.0",
    "config_template": {
      "host": "192.168.1.100",
      "port": 502,
      "slave_id": 1,
      "timeout_ms": 5000
    },
    "properties": [
      {
        "name": "temperature",
        "display_name": "温度",
        "data_type": "float",
        "unit": "°C",
        "address": 40001,
        "register_type": "holding"
      },
      {
        "name": "humidity",
        "display_name": "湿度",
        "data_type": "float",
        "unit": "%RH",
        "address": 40003,
        "register_type": "holding"
      }
    ],
    "commands": [
      {
        "name": "reset",
        "display_name": "重置设备",
        "description": "将设备恢复到默认状态",
        "parameters": []
      }
    ],
    "device_count": 12,
    "tags": ["temperature", "humidity", "modbus"],
    "created_at": "2024-01-01 10:00:00",
    "updated_at": "2024-01-05 10:00:00"
  }
}
```

---

### 创建模板

```
POST /api/v1/device-templates
```

**请求体：**

```json
{
  "name": "自定义温湿度传感器",
  "description": "基于 Modbus TCP 的自定义温湿度传感器",
  "category": "sensor",
  "driver_name": "modbus_tcp",
  "version": "1.0.0",
  "config_template": "{\"host\":\"192.168.1.100\",\"port\":502,\"slave_id\":1}",
  "properties": "[{\"name\":\"temperature\",\"display_name\":\"温度\",\"data_type\":\"float\",\"unit\":\"°C\",\"address\":40001}]",
  "tags": ["custom", "temperature"]
}
```

---

### 更新模板

```
PUT /api/v1/device-templates/{id}
```

---

### 删除模板

```
DELETE /api/v1/device-templates/{id}
```

---

### 使用模板创建设备

```
POST /api/v1/devices/from-template
```

使用指定模板创建设备。

**请求体：**

```json
{
  "template_id": "tmpl_001",
  "name": "车间温度传感器",
  "display_name": "一楼车间温度",
  "config": {
    "host": "192.168.1.150",
    "port": 502,
    "slave_id": 2
  }
}
```

**响应示例：**

```json
{
  "success": true,
  "result": {
    "device": {
      "id": "device_new_001",
      "name": "车间温度传感器",
      "display_name": "一楼车间温度",
      "driver_name": "modbus_tcp",
      "state": 1,
      "created_at": "2024-01-07 16:00:00"
    },
    "template_id": "tmpl_001"
  }
}
```

---

### 验证模板配置

```
POST /api/v1/device-templates/validate
```

验证模板配置的合法性。

**请求体：**

```json
{
  "template_id": "tmpl_001",
  "config": {
    "host": "192.168.1.150",
    "port": 502
  }
}
```

---

### 预览模板设备

```
POST /api/v1/device-templates/preview
```

预览基于模板创建设备的配置预览。

**请求体：**

```json
{
  "template_id": "tmpl_001",
  "name": "测试设备",
  "config": {
    "host": "192.168.1.200"
  }
}
```

**响应示例：**

```json
{
  "success": true,
  "result": {
    "name": "测试设备",
    "driver_name": "modbus_tcp",
    "config": {
      "host": "192.168.1.200",
      "port": 502,
      "slave_id": 1,
      "timeout_ms": 5000
    },
    "properties": [
      {
        "name": "temperature",
        "address": 40001
      }
    ],
    "warnings": []
  }
}
```

## 数据结构

### DeviceTemplate

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 模板 ID |
| name | string | 模板名称 |
| description | string? | 描述 |
| category | string | 分类：sensor、actuator、camera、gateway |
| driver_name | string | 驱动名称 |
| version | string | 版本号 |
| config_template | string | 默认配置模板（JSON） |
| properties | string | 属性定义（JSON） |
| commands | string | 指令定义（JSON） |
| device_count | number | 使用该模板的设备数量 |
| tags | string? | 标签 |
| created_at | string | 创建时间 |
| updated_at | string | 更新时间 |

## 使用场景

### 1. 使用模板快速创建设备

```javascript
// 使用模板创建设备
const device = await fetch('/api/v1/devices/from-template', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    template_id: 'tmpl_001',
    name: '三楼温度传感器',
    config: {
      host: '192.168.1.200',
      slave_id: 3
    }
  })
});
```

### 2. 批量创建同类型设备

```javascript
const locations = ['一楼', '二楼', '三楼', '四楼'];
const hosts = ['192.168.1.100', '192.168.1.101', '192.168.1.102', '192.168.1.103'];

for (let i = 0; i < locations.length; i++) {
  await fetch('/api/v1/devices/from-template', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      template_id: 'tmpl_001',
      name: `${locations[i]}温度传感器`,
      config: { host: hosts[i] }
    })
  });
}
```

## 错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 400 | 请求参数错误 |
| 404 | 模板不存在 |
| 500 | 服务器内部错误 |
