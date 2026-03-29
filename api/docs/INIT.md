# 用户初始化与种子数据设计

## 1. 核心原则

### 1.1 迁移脚本只做结构，不做数据
- `CREATE TABLE`、`CREATE INDEX`、`ALTER TABLE` 等 DDL 在迁移脚本中
- **禁止**在迁移脚本中 `INSERT` 用户数据
- 原因：密码哈希需要运行时计算，不能写在 SQL 里

### 1.2 用户创建由应用层管理
- 管理员用户由 `ensure_default_admin_user()` 在应用启动时创建
- 使用 `bcrypt::hash()` 计算密码哈希
- 只有当数据库中没有用户时才创建

### 1.3 种子数据通过专用接口注入
- 测试数据（额外用户、设备等）通过 `/api/v1/system/seed` 接口注入
- 该接口可重复调用（使用 `INSERT OR IGNORE` 保证幂等）
- 仅在开发/测试环境启用

---

## 2. 当前问题修复

### 2.1 问题根因
迁移脚本 `20260106000002_rebuild_database_with_snake_case.sql` 中：
```sql
-- 错误：密码哈希是占位符，不是真正的 bcrypt 哈希
INSERT INTO users (id, username, password_hash, display_name, is_enabled) VALUES
('admin-user-001', 'admin', 'hashed_admin123', '系统管理员', true);
```

`bcrypt::verify("admin123", "hashed_admin123")` 会失败，因为 `hashed_admin123` 不是有效的 bcrypt 哈希格式。

### 2.2 修复方案

**第一步**：删除迁移脚本中的用户创建 SQL（已完成）

**第二步**：确保 `ensure_default_admin_user` 正确工作
- 已在 `initialization.rs` 中修复：检测 `password_hash == "hashed_admin123"` 时重新设置正确密码

**第三步**：删除旧的假哈希用户（用户需手动操作）
```bash
# 重启 API 服务，会自动修复 admin 用户密码
```

---

## 3. 后续维护指南

### 3.1 添加新的默认用户
在 `api/src/api/system/initialization.rs` 的 `ensure_default_admin_user()` 函数中添加：

```rust
// 创建额外的默认用户
let extra_users = vec![
    ("operator", "operator123", "操作员", "operator@tinyiothub.local"),
    ("viewer", "viewer123", "查看者", "viewer@tinyiothub.local"),
];

for (username, password, display_name, email) in extra_users {
    if User::find_by_username(db, username).await?.is_none() {
        let req = CreateUserRequest {
            username: username.to_string(),
            password: password.to_string(),
            display_name: Some(display_name.to_string()),
            email: Some(email.to_string()),
            ..Default::default()
        };
        User::create(db, &req).await?;
    }
}
```

### 3.2 添加新的测试用户（种子数据）
在 `api/src/api/system/seed.rs` 中添加（如果不存在则创建）：
```rust
// 使用 INSERT OR IGNORE，重复调用安全
INSERT OR IGNORE INTO users (id, username, password_hash, email, display_name, is_enabled, created_at)
VALUES ('user-test-001', 'test1', '<valid_bcrypt_hash>', 'test1@example.com', '测试用户1', 1, datetime('now'));
```

### 3.3 生成正确的 bcrypt 哈希
在项目根目录运行：
```bash
cd api && cargo run --example gen_hash
```
或临时在 `initialization.rs` 中加一行日志打印：
```rust
let hash = hash_password("admin123").unwrap();
tracing::info!("admin123 bcrypt hash: {}", hash);
```

---

## 4. 默认账号

| 用户名 | 密码 | 角色 |
|--------|------|------|
| admin | admin123 | 系统管理员 |
| operator | operator123 | 操作员（待添加） |
| test1 | admin123 | 查看者（待添加） |

---

## 5. 相关文件

- `api/src/api/system/initialization.rs` - 系统初始化，用户创建
- `api/src/api/system/seed.rs` - 种子数据注入（待创建）
- `api/src/api/auth/login.rs` - 登录逻辑
- `api/src/dto/entity/user.rs` - User 实体，包含密码验证
- `api/src/utils/password.rs` - bcrypt 哈希/验证工具
- `api/migrations/` - SQL 迁移脚本（仅 DDL）
