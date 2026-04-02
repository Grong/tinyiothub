-- 添加 phone_number 字段到 users 表
-- 用于微信和短信登录功能
-- 创建时间: 2026-03-30
--
-- 注意: SQLite 不支持 ALTER TABLE ADD COLUMN with UNIQUE 约束
-- 因此我们先添加列，然后通过唯一索引来强制 UNIQUE 约束

ALTER TABLE users ADD COLUMN phone_number VARCHAR(20);

-- 创建唯一索引以强制 phone_number 的唯一性（同时作为查询索引）
CREATE UNIQUE INDEX idx_users_phone_number ON users(phone_number);
