-- T1: thing_alarms 补 is_suppressed 持久层
-- 背景：AlarmStatus::Suppressed 枚举早已存在但无落库列，导致
-- batch_update_alarm_status 对 Suppressed 空操作、按 Suppressed 过滤静默
-- 返回全部、list_active_* 把被抑制报警仍计入活跃。
-- 语义：is_suppressed 与 is_acknowledged/is_resolved 正交；状态推导优先级
-- resolved > suppressed > acknowledged > active（见 row_to_alarm）。
ALTER TABLE thing_alarms ADD COLUMN is_suppressed INTEGER NOT NULL DEFAULT 0;
