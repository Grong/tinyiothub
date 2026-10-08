-- AI 运行透明化 Task 2：judgments 加 narrative 列（判定中文叙述；写入方在
-- 后续任务填充，本迁移只加列）。历史行 narrative 为 NULL。
-- 逆迁移：ALTER TABLE judgments DROP COLUMN narrative;
ALTER TABLE judgments ADD COLUMN narrative TEXT;
