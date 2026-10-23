-- E4d：物显式 zone 引用（placement.zoneRef 物化）。
-- additive：只增列；空值 = 无显式引用，读取侧按 point-in-polygon 解析。
ALTER TABLE things ADD COLUMN zone_ref TEXT;
