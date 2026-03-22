# 系统管理 API

## 概述

系统管理 API 提供系统配置、产品管理、任务调度和系统特性等管理功能。

## 路由结构

系统 API 通过子路由组织多个功能模块：

```
/api/v1/system/
├── configuration/...  # 系统配置
├── features           # 系统特性
├── initialization     # 系统初始化
├── tasks/...          # 任务管理
└── products/...       # 产品管理
```

## 子模块说明

### 系统配置 (`/api/v1/system/configuration/`)

获取和更新系统配置参数。

**主要端点：**

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/system/configuration` | 获取系统配置 |
| PUT | `/api/v1/system/configuration` | 更新系统配置 |

**配置响应示例：**

```json
{
  "success": true,
  "result": {
    "system": {
      "name": "TinyIoTHub",
      "version": "1.2.0",
      "log_level": "info"
    },
    "mqtt": {
      "host": "192.168.1.124",
      "port": 1883,
      "username": "admin",
      "qos": 1
    },
    "database": {
      "type": "sqlite",
      "path": "./data/iot.db"
    }
  }
}
```

---

### 系统特性 (`/api/v1/system/features`)

获取系统支持的特性列表和状态。

**响应示例：**

```json
{
  "success": true,
  "result": {
    "features": [
      {
        "name": "device_management",
        "enabled": true,
        "description": "设备管理功能"
      },
      {
        "name": "alarm_management",
        "enabled": true,
        "description": "告警管理功能"
      },
      {
        "name": "marketplace",
        "enabled": true,
        "description": "驱动和模板市场"
      },
      {
        "name": "automation",
        "enabled": true,
        "description": "自动化规则"
      }
    ]
  }
}
```

---

### 系统初始化 (`/api/v1/system/initialization`)

系统初始化相关接口，包括创建默认管理员账号等。

---

### 任务管理 (`/api/v1/system/tasks/`)

查看和管理系统调度任务。

**主要端点：**

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/system/tasks` | 获取任务列表 |
| GET | `/api/v1/system/tasks/{id}` | 获取任务详情 |

---

### 产品管理 (`/api/v1/system/products/`)

产品管理接口，用于管理设备产品型号。

**主要端点：**

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/system/products` | 获取产品列表 |
| POST | `/api/v1/system/products` | 创建产品 |
| GET | `/api/v1/system/products/{id}` | 获取产品详情 |
| PUT | `/api/v1/system/products/{id}` | 更新产品 |
| DELETE | `/api/v1/system/products/{id}` | 删除产品 |

**创建产品请求体：**

```json
{
  "name": "智能温度传感器",
  "model": "TS-2024",
  "manufacturer": "科技公司",
  "description": "高精度温度传感器，支持 Modbus TCP"
}
```

**产品响应示例：**

```json
{
  "success": true,
  "result": {
    "id": "prod_001",
    "name": "智能温度传感器",
    "model": "TS-2024",
    "manufacturer": "科技公司",
    "description": "高精度温度传感器，支持 Modbus TCP",
    "driver_name": "modbus_tcp",
    "created_at": "2024-01-01 10:00:00",
    "updated_at": "2024-01-01 10:00:00"
  }
}
```

## 产品数据结构

### Product

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 产品 ID |
| name | string | 产品名称 |
| model | string | 型号 |
| manufacturer | string | 制造商 |
| description | string? | 描述 |
| driver_name | string? | 关联驱动 |
| created_at | string | 创建时间 |
| updated_at | string | 更新时间 |

## 使用场景

### 1. 获取系统配置

```javascript
const config = await fetch('/api/v1/system/configuration');
console.log('MQTT Host:', config.result.mqtt.host);
```

### 2. 创建产品

```json
POST /api/v1/system/products
{
  "name": "Modbus 温湿度传感器",
  "model": "TH-2024",
  "manufacturer": "传感器科技",
  "description": "工业级 Modbus TCP 温湿度传感器"
}
```

### 3. 检查功能开关

```javascript
const { result } = await fetch('/api/v1/system/features');
const automationEnabled = result.features.find(f => f.name === 'automation')?.enabled;
```

## 错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 400 | 请求参数错误 |
| 401 | 未认证 |
| 404 | 资源不存在 |
| 500 | 服务器内部错误 |
