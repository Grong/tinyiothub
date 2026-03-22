# 用户管理 API

## 概述

用户管理 API 提供用户账号、角色和权限的完整管理功能，包括用户 CRUD、角色管理和细粒度权限控制。

## 路由结构

用户 API 通过嵌套路由组织：

```
/api/v1/users/
├── management/...  # 用户 CRUD
├── roles/...       # 角色管理
└── permissions/... # 权限管理
```

## 用户管理 (`/api/v1/users/management/`)

### 获取用户列表

```
GET /api/v1/users/management/users
```

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| name | string | 否 | 按用户名模糊筛选 |
| email | string | 否 | 按邮箱筛选 |
| enabled | boolean | 否 | 是否启用 |
| role_id | string | 否 | 角色 ID |
| page | number | 否 | 页码，默认 1 |
| page_size | number | 否 | 每页数量，默认 20 |

---

### 获取用户详情

```
GET /api/v1/users/management/users/{id}
```

**响应示例：**

```json
{
  "success": true,
  "result": {
    "id": "user_001",
    "username": "admin",
    "name": "系统管理员",
    "email": "admin@example.com",
    "phone": "13800138000",
    "enabled": true,
    "last_login_at": "2024-01-07 15:30:00",
    "created_at": "2024-01-01 10:00:00",
    "updated_at": "2024-01-07 15:30:00",
    "roles": [
      {
        "id": "role_001",
        "name": "管理员",
        "description": "系统管理员角色"
      }
    ]
  }
}
```

---

### 创建用户

```
POST /api/v1/users/management/users
```

**请求体：**

```json
{
  "username": "zhangsan",
  "name": "张三",
  "email": "zhangsan@example.com",
  "password": "Password123!",
  "phone": "13800138000",
  "enabled": true,
  "role_ids": ["role_002"]
}
```

---

### 更新用户

```
PUT /api/v1/users/management/users/{id}
```

---

### 删除用户

```
DELETE /api/v1/users/management/users/{id}
```

**响应：** `204 No Content`

---

## 角色管理 (`/api/v1/users/roles/`)

### 获取角色列表

```
GET /api/v1/users/roles
```

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "role_001",
      "name": "管理员",
      "description": "系统管理员，拥有所有权限",
      "is_system": true,
      "user_count": 2,
      "created_at": "2024-01-01 10:00:00",
      "updated_at": "2024-01-01 10:00:00"
    },
    {
      "id": "role_002",
      "name": "操作员",
      "description": "设备管理和告警处理",
      "is_system": false,
      "user_count": 5,
      "created_at": "2024-01-01 10:00:00",
      "updated_at": "2024-01-01 10:00:00"
    }
  ]
}
```

---

### 获取角色详情

```
GET /api/v1/users/roles/{id}
```

---

### 创建角色

```
POST /api/v1/users/roles
```

**请求体：**

```json
{
  "name": "设备管理员",
  "description": "负责设备日常管理",
  "permission_ids": ["perm_001", "perm_002", "perm_003"]
}
```

---

### 更新角色

```
PUT /api/v1/users/roles/{id}
```

---

### 删除角色

```
DELETE /api/v1/users/roles/{id}
```

---

### 获取角色权限

```
GET /api/v1/users/roles/{id}/permissions
```

## 权限管理 (`/api/v1/users/permissions/`)

### 获取权限列表

```
GET /api/v1/users/permissions
```

**响应示例：**

```json
{
  "success": true,
  "result": [
    {
      "id": "perm_001",
      "name": "device:read",
      "display_name": "查看设备",
      "description": "查看设备列表和详情",
      "resource": "device",
      "action": "read"
    },
    {
      "id": "perm_002",
      "name": "device:write",
      "display_name": "管理设备",
      "description": "创建设备、编辑设备配置",
      "resource": "device",
      "action": "write"
    },
    {
      "id": "perm_003",
      "name": "alarm:read",
      "display_name": "查看告警",
      "description": "查看告警列表和详情",
      "resource": "alarm",
      "action": "read"
    }
  ]
}
```

---

### 获取用户权限

```
GET /api/v1/users/permissions/user/{user_id}
```

获取指定用户的有效权限列表。

---

### 获取角色权限

```
GET /api/v1/users/permissions/role/{role_id}
```

## 预定义角色

| 角色 | ID | 说明 |
|------|------|------|
| 管理员 | role_001 | 完全访问权限 |
| 操作员 | role_002 | 设备管理和告警处理 |
| 查看者 | role_003 | 只读访问权限 |

## 预定义权限

### 设备权限

| 权限标识 | 说明 |
|----------|------|
| device:read | 查看设备 |
| device:write | 管理设备 |
| device:delete | 删除设备 |
| device:command | 下发命令 |

### 告警权限

| 权限标识 | 说明 |
|----------|------|
| alarm:read | 查看告警 |
| alarm:write | 管理告警规则 |
| alarm:acknowledge | 确认告警 |

### 系统权限

| 权限标识 | 说明 |
|----------|------|
| user:read | 查看用户 |
| user:write | 管理用户 |
| system:config | 系统配置 |

## 使用场景

### 1. 创建操作员账号

```json
POST /api/v1/users/management/users
{
  "username": "operator01",
  "name": "操作员01",
  "email": "operator01@example.com",
  "password": "SecurePass123!",
  "enabled": true,
  "role_ids": ["role_002"]
}
```

### 2. 创建自定义角色

```json
POST /api/v1/users/roles
{
  "name": "数据分析师",
  "description": "可查看设备和数据，但不能操作",
  "permission_ids": [
    "device:read",
    "alarm:read",
    "data:export"
  ]
}
```

### 3. 更新用户角色

```json
PUT /api/v1/users/management/users/user_003
{
  "role_ids": ["role_002", "role_003"]
}
```

## 错误码

| HTTP 状态码 | 说明 |
|-------------|------|
| 200 | 请求成功 |
| 204 | 删除成功 |
| 400 | 请求参数错误（用户名重复等） |
| 404 | 用户或角色不存在 |
| 500 | 服务器内部错误 |
