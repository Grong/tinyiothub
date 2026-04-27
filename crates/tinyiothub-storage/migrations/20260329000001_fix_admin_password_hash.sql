-- ============================================================================
-- 修复管理员用户密码哈希
-- ============================================================================
-- 问题：初始迁移使用了占位符哈希 'hashed_admin123'，这不是有效的 bcrypt 格式
-- 解决：将占位符哈希替换为特殊标记，供 ensure_default_admin_user 识别并修复
-- 注意：此迁移之后，admin 用户将由 ensure_default_admin_user 在应用启动时创建
-- ============================================================================

-- 将迁移脚本中创建的 admin 用户的密码哈希标记为待修复
-- ensure_default_admin_user 会检测到 'FIX_ME_admin_hash' 并重新设置正确密码
UPDATE users SET password_hash = 'FIX_ME_admin_hash' WHERE username = 'admin' AND password_hash = 'hashed_admin123';
