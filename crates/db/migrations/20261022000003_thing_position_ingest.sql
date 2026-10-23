-- E4d：位置摄取门。仅当 manifest placement.mode=runtime 且 fromSource 指向
-- 已声明的 kind=position source 时置 1；其余（fixed/declared/未声明）为 0，
-- DataServer 忽略保留的 position_* 读数。
ALTER TABLE things ADD COLUMN position_ingest INTEGER NOT NULL DEFAULT 0;
