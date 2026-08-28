-- 持久化策略动作名迁移(device→thing PR-2 Task 5 修复轮)。
-- workspace_autonomy_policy.allowed_actions / denied_actions 为 JSON 数组 TEXT,
-- 存量行可能含旧动作名 "wipe_device"/"reboot_device"。代码侧已更名
-- wipe_thing/reboot_thing,不迁移则 gate_check 对旧动作名静默失配
-- (如:deny 规则不再拦截 LLM 发起的 wipe_thing)。
-- 实现说明:SQLite 无 JSON 数组元素级重写,用 replace() 精确替换带引号的
-- 完整 token("wipe_device" 含首尾引号),不会误伤 "wipe_device_extra" 等
-- 前缀/子串值,也不触碰 JSON 结构本身。
UPDATE workspace_autonomy_policy
SET allowed_actions = replace(allowed_actions, '"wipe_device"', '"wipe_thing"'),
    denied_actions  = replace(denied_actions,  '"wipe_device"', '"wipe_thing"')
WHERE allowed_actions LIKE '%"wipe_device"%'
   OR denied_actions  LIKE '%"wipe_device"%';

UPDATE workspace_autonomy_policy
SET allowed_actions = replace(allowed_actions, '"reboot_device"', '"reboot_thing"'),
    denied_actions  = replace(denied_actions,  '"reboot_device"', '"reboot_thing"')
WHERE allowed_actions LIKE '%"reboot_device"%'
   OR denied_actions  LIKE '%"reboot_device"%';
