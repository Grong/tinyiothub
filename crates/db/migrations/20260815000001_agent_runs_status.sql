-- Run 生命周期状态（Task 9 僵尸 reconcile）：现有行均为完成时落库 →
-- 'completed'；'running' 行（未来 run 开始时写入的路径）若因进程崩溃
-- 残留，启动时由 reconcile 标记 'interrupted'。
ALTER TABLE agent_runs ADD COLUMN status TEXT NOT NULL DEFAULT 'completed';
