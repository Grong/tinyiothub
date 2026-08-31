-- agent 工具名数据迁移(thing-rename follow-ups Task 3)。
-- /tools/toggle 把 catalog 工具 id 写入 agent_configs.config 的 tool_denylist
-- (config/service.rs toggle_tool);/tools/effective 以 denylist 精确匹配 MCP
-- 注册名(effective_tool_names → filter_by_denylist)。静态兜底 catalog 长期
-- 残留 PR-1 旧名(search_devices/get_device/create_device/delete_device),
-- 且旧版默认 denylist 含 "delete_device"(81d073fc 前),存量 config 中的旧名
-- 静默失配:toggle 无效、delete_thing 未被默认禁用。
-- 实现说明:SQLite 无 JSON 数组元素级重写,用 replace() 精确替换带引号的
-- 完整 token("delete_device" 含首尾引号),不会误伤 "delete_device_extra"
-- 等前缀/子串值,也不触碰 JSON 结构本身(同 20260828000001 模式)。
-- 注:agent_tools.tool_overrides 为死列(仅级联删除、无读写),不在迁移范围。
UPDATE agent_configs SET config = replace(config, '"search_devices"', '"search_things"')
WHERE config LIKE '%"search_devices"%';

UPDATE agent_configs SET config = replace(config, '"get_device"', '"get_thing"')
WHERE config LIKE '%"get_device"%';

UPDATE agent_configs SET config = replace(config, '"create_device"', '"create_thing"')
WHERE config LIKE '%"create_device"%';

UPDATE agent_configs SET config = replace(config, '"delete_device"', '"delete_thing"')
WHERE config LIKE '%"delete_device"%';
