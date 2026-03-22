# 市场 API

## 概述

市场 API 提供驱动市场和模板市场的浏览与安装功能。通过市场 API，用户可以浏览官方和社区贡献的驱动和设备模板，并一键安装到本地系统。

## 接口列表

### 获取市场模板列表

```
GET /api/v1/marketplace/templates
```

获取可用的设备模板列表。

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "tmpl_001",
      "name": "工业温湿度传感器",
      "description": "适用于工业环境的温湿度传感器模板",
      "version": "1.0.0",
      "category": "sensor",
      "driver_name": "modbus_tcp",
      "thumbnail": "https://marketplace.example.com/tmpl_001.png",
      "downloads": 1250,
      "rating": 4.8,
      "author": "TinyIoTHub Team",
      "tags": ["temperature", "humidity", "industrial"],
      "created_at": "2024-01-01",
      "updated_at": "2024-01-05"
    }
  ]
}
```

---

### 获取市场模板详情

```
GET /api/v1/marketplace/templates/{id}
```

获取指定模板的详细信息。

**响应示例：**

```json
{
  "success": true,
  "result": {
    "id": "tmpl_001",
    "name": "工业温湿度传感器",
    "description": "适用于工业环境的温湿度传感器模板",
    "version": "1.0.0",
    "category": "sensor",
    "driver_name": "modbus_tcp",
    "thumbnail": "https://marketplace.example.com/tmpl_001.png",
    "downloads": 1250,
    "rating": 4.8,
    "author": "TinyIoTHub Team",
    "tags": ["temperature", "humidity", "industrial"],
    "config_template": {
      "host": "192.168.1.100",
      "port": 502,
      "slave_id": 1,
      "registers": [
        { "name": "temperature", "address": 40001, "type": "float" },
        { "name": "humidity", "address": 40003, "type": "float" }
      ]
    },
    "created_at": "2024-01-01",
    "updated_at": "2024-01-05"
  }
}
```

---

### 安装市场模板

```
POST /api/v1/marketplace/templates/{id}/install
```

将模板安装到本地系统。

**请求体：**

```json
{
  "version": "1.0.0"
}
```

**响应示例：**

```json
{
  "success": true,
  "result": "tmpl_local_001"
}
```

---

### 获取市场驱动列表

```
GET /api/v1/marketplace/drivers
```

获取可用的驱动列表。

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "drv_001",
      "name": "BACnet 驱动",
      "description": "支持 BACnet/IP 协议的楼宇自动化设备",
      "version": "1.2.0",
      "category": "building_automation",
      "thumbnail": "https://marketplace.example.com/drv_001.png",
      "downloads": 890,
      "rating": 4.6,
      "author": "Community Contributor",
      "tags": ["bacnet", "building", "automation"],
      "created_at": "2024-01-01",
      "updated_at": "2024-01-05"
    }
  ]
}
```

---

### 获取市场驱动详情

```
GET /api/v1/marketplace/drivers/{id}
```

---

### 安装市场驱动

```
POST /api/v1/marketplace/drivers/{id}/install
```

将驱动安装到本地系统。

**请求体：**

```json
{
  "version": "1.2.0"
}
```

**响应示例：**

```json
{
  "success": true,
  "result": "bacnet_driver"
}
```

## 模板数据结构

### TemplateMetadata

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 模板 ID |
| name | string | 模板名称 |
| description | string | 描述 |
| version | string | 版本号 |
| category | string | 分类 |
| driver_name | string | 关联驱动 |
| thumbnail | string? | 缩略图 URL |
| downloads | number | 下载次数 |
| rating | number | 评分 |
| author | string | 作者 |
| tags | string[] | 标签 |
| created_at | string | 创建时间 |
| updated_at | string | 更新时间 |

## 驱动数据结构

### DriverMetadata

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 驱动 ID |
| name | string | 驱动名称 |
| description | string | 描述 |
| version | string | 版本号 |
| category | string | 分类 |
| thumbnail | string? | 缩略图 URL |
| downloads | number | 下载次数 |
| rating | number | 评分 |
| author | string | 作者 |
| tags | string[] | 标签 |
| created_at | string | 创建时间 |
| updated_at | string | 更新时间 |

## 使用场景

### 1. 浏览并安装传感器模板

```javascript
// 获取模板列表
const templates = await fetch('/api/v1/marketplace/templates');

// 安装模板
await fetch('/api/v1/marketplace/templates/tmpl_001/install', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ version: '1.0.0' })
});
```

### 2. 安装第三方驱动

```javascript
// 获取驱动列表
const drivers = await fetch('/api/v1/marketplace/drivers');

// 安装 BACnet 驱动
await fetch('/api/v1/marketplace/drivers/drv_001/install', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ version: '1.2.0' })
});
```

## 错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 400 | 无效的请求参数 |
| 401 | 未认证 |
| 404 | 模板或驱动不存在 |
| 500 | 安装失败（网络或权限问题） |
