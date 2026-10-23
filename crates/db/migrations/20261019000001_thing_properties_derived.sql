-- 园区物：派生属性（spec 2026-10-19 §二）
-- source: 'template'（模板拷贝，存量默认）| 'derived'（表达式派生）
-- expression/refs 仅 source='derived' 时有值；refs 为 JSON [{thing_id, property_name}]
ALTER TABLE thing_properties ADD COLUMN source TEXT NOT NULL DEFAULT 'template';
ALTER TABLE thing_properties ADD COLUMN expression TEXT;
ALTER TABLE thing_properties ADD COLUMN refs TEXT;
-- E6 索引无需新建：devices 已在 20260825000001 改名 things，
-- idx_things_parent_id ON things(parent_id) 由该迁移创建。
