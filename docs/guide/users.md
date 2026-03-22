# 用户管理

用户管理系统负责账号、权限和认证管理，确保不同用户访问相应的功能和数据。

## 用户角色

TinyIoTHub 使用 RBAC（基于角色的访问控制）模型：

| 角色 | 说明 | 典型用户 |
|------|------|----------|
| 管理员 | 完全访问权限 | 系统管理员 |
| 操作员 | 设备管理和告警处理 | 运维人员 |
| 查看者 | 只读访问 | 访客、分析师 |

### 预定义角色权限

| 权限 | 管理员 | 操作员 | 查看者 |
|------|:------:|:------:|:------:|
| 设备管理（增删改） | ✓ | ✓ | ✗ |
| 设备查看 | ✓ | ✓ | ✓ |
| 告警管理 | ✓ | ✓ | ✗ |
| 驱动管理 | ✓ | ✗ | ✗ |
| 用户管理 | ✓ | ✗ | ✗ |
| 系统配置 | ✓ | ✗ | ✗ |
| 数据导出 | ✓ | ✓ | ✓ |

## 用户操作

### 创建用户

1. 进入「用户管理」页面
2. 点击「添加用户」
3. 填写用户信息：
   - **用户名**：登录账号（唯一）
   - **显示名称**：界面显示名称
   - **邮箱**：用于通知和密码找回
   - **手机号**：用于短信通知
   - **角色**：选择所属角色
4. 点击「保存」

**API 创建用户：**

```http
POST /api/v1/users/management/users
Content-Type: application/json

{
  "username": "zhangsan",
  "name": "张三",
  "email": "zhangsan@example.com",
  "password": "SecurePass123!",
  "role_ids": ["role_002"],
  "enabled": true
}
```

### 修改密码

用户可自行修改密码：

1. 点击右上角头像
2. 选择「修改密码」
3. 输入当前密码和新密码
4. 确认修改

**API 修改密码：**

```http
PUT /api/v1/users/management/users/{id}
Content-Type: application/json

{
  "password": "NewPassword123!"
}
```

### 重置密码

管理员可重置用户密码：

1. 在用户列表找到目标用户
2. 点击「重置密码」
3. 输入新密码
4. 用户首次登录需要修改密码

### 启用/禁用用户

- **禁用用户**：禁止该用户登录，但保留其数据
- **启用用户**：恢复用户登录权限

## 角色管理

### 创建自定义角色

管理员可创建自定义角色：

```http
POST /api/v1/users/roles
Content-Type: application/json

{
  "name": "数据分析师",
  "description": "可查看设备和数据，但不能操作设备",
  "permission_ids": ["device:read", "alarm:read", "data:export"]
}
```

### 权限说明

TinyIoTHub 的权限分为以下资源类型：

**设备相关：**

| 权限标识 | 说明 |
|----------|------|
| device:read | 查看设备列表和详情 |
| device:write | 创建设备、编辑设备配置 |
| device:delete | 删除设备 |
| device:command | 下发设备命令 |

**告警相关：**

| 权限标识 | 说明 |
|----------|------|
| alarm:read | 查看告警列表和详情 |
| alarm:write | 创建和编辑告警规则 |
| alarm:acknowledge | 确认告警 |

**系统相关：**

| 权限标识 | 说明 |
|----------|------|
| user:read | 查看用户列表 |
| user:write | 创建和编辑用户 |
| system:config | 修改系统配置 |

**自动化相关：**

| 权限标识 | 说明 |
|----------|------|
| automation:read | 查看自动化规则 |
| automation:write | 创建和编辑自动化规则 |
| automation:execute | 手动执行自动化 |

**驱动相关：**

| 权限标识 | 说明 |
|----------|------|
| driver:read | 查看驱动列表 |
| driver:write | 加载/卸载驱动 |
| driver:develop | 开发自定义驱动 |

### 更新角色权限

```http
PUT /api/v1/users/roles/{id}
Content-Type: application/json

{
  "permission_ids": ["device:read", "alarm:read", "alarm:acknowledge"]
}
```

## 会话管理

### 登录会话

- JWT Token 认证
- 默认有效期：**24 小时**
- 可在系统配置中修改有效期

### 强制登出

管理员可强制下线指定用户：

1. 在用户列表点击目标用户
2. 点击「强制下线」
3. 用户下次请求时将被要求重新登录

### 查看在线用户

管理员可查看当前在线用户列表：

```http
GET /api/v1/auth/session
```

## 多租户支持

TinyIoTHub 支持多租户架构：

- 每个租户拥有独立的设备、用户和数据
- API Key 用于 Open API 身份验证（租户级别）
- 用户权限在租户内部有效

### 租户管理 API

```http
GET /api/v1/tenants                    # 获取租户列表
POST /api/v1/tenants                  # 创建租户
GET /api/v1/tenants/{id}              # 获取租户详情
PUT /api/v1/tenants/{id}              # 更新租户
DELETE /api/v1/tenants/{id}           # 删除租户
```

### API Key 管理

每个租户可创建多个 API Key：

```http
POST /api/v1/tenants/{id}/api-keys    # 创建 API Key
GET /api/v1/tenants/{id}/api-keys    # 获取 API Key 列表
DELETE /api/v1/tenants/{id}/api-keys/{key_id}  # 撤销 API Key
```

## 常见问题

**Q：忘记管理员密码怎么办？**
- 通过数据库直接重置（需要服务器访问权限）
- 重建管理员用户

**Q：如何限制用户只能看到自己的设备？**
- 通过组织架构（Organization）分配设备归属
- 用户只能看到其所属组织的设备

**Q：可以同时给用户分配多个角色吗？**
- 可以，用户可以同时属于多个角色
- 最终权限为所有角色权限的并集
