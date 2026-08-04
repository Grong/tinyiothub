-- 为 users.phone 添加局部唯一索引，确保非空手机号全局唯一，同时保留 NULL 的灵活性

-- 1. 先把空字符串转为 NULL，避免被唯一索引视为有效值
UPDATE users SET phone = NULL WHERE phone = '';

-- 2. 对已有重复 phone，保留 id 最小（最早创建）的一条，其余置 NULL
UPDATE users
SET phone = NULL
WHERE id NOT IN (
    SELECT MIN(id)
    FROM users
    WHERE phone IS NOT NULL
    GROUP BY phone
);

-- 3. 创建局部唯一索引
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_phone_unique ON users(phone) WHERE phone IS NOT NULL;
