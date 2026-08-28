-- 持久化策略动作名迁移(device→thing PR-2 Task 5 修复轮 1+2)。
-- 三处持久化 spot 的存量旧动作名 "wipe_device"/"reboot_device" 不随代码更名
-- 自动迁移,导致静默失配(如:deny 规则不再拦截 LLM 发起的 wipe_thing):
--   1. workspace_autonomy_policy.allowed_actions/denied_actions — JSON 数组 TEXT;
--   2. policy_rules.target — 裸 TEXT 精确值或 glob 模式,仅翻转精确相等值;
--   3. workspaces.heartbeat_trust_config — TrustConfig JSON(默认 '' 空串),
--      其中 blocked_tools / allowed_destructive_tools 均为工具名数组。
-- 实现说明:SQLite 无 JSON 数组元素级重写,JSON 列用 replace() 精确替换带引号的
-- 完整 token("wipe_device" 含首尾引号),不会误伤 "wipe_device_extra" 等
-- 前缀/子串值,也不触碰 JSON 结构本身;policy_rules.target 为裸值,用精确
-- 相等 UPDATE,glob 模式(如 'wipe_*')天然不受影响。
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

UPDATE policy_rules SET target = 'wipe_thing' WHERE target = 'wipe_device';
UPDATE policy_rules SET target = 'reboot_thing' WHERE target = 'reboot_device';

UPDATE workspaces
SET heartbeat_trust_config = replace(heartbeat_trust_config, '"wipe_device"', '"wipe_thing"')
WHERE heartbeat_trust_config LIKE '%"wipe_device"%';

UPDATE workspaces
SET heartbeat_trust_config = replace(heartbeat_trust_config, '"reboot_device"', '"reboot_thing"')
WHERE heartbeat_trust_config LIKE '%"reboot_device"%';
