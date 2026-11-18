-- 2026-11-02 调查：runtime_projection / SSE 每次取工作区最大账本 id，
-- 查询计划只能扫 idx_hub_ledger_ws_ts 的全部 (workspace_id, ts) 条目
-- （313 万行实测 2.1s）。长读锁会阻塞提交，AlarmEventHandler 因此在
-- busy_timeout 后报 "database is locked"。补 (workspace_id, id) 让
-- MAX(id) 直接定位索引末项。
CREATE INDEX IF NOT EXISTS idx_hub_ledger_ws_id ON hub_ledger(workspace_id, id);
