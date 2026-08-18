-- Restore the January (20260106000002) per-device seed properties/actions into
-- thing_properties/thing_actions.
--
-- Why: 20260723000001 (thing_ontology_rebuild) drops and recreates `devices`.
-- Under PRAGMA foreign_keys=ON (sqlx default), SQLite performs an implicit
-- DELETE FROM devices on drop, which ON DELETE CASCADE wipes
-- device_properties/device_commands. The 20260727000001 cleanup's design
-- intent — "real rows are preserved via UNION from device_properties" — is
-- therefore void: by the time it runs, the real rows are already gone, and
-- its synthetic-seed deletion leaves the tables empty. Fresh installs and
-- DBs that applied this chain end up with empty thing_properties/thing_actions
-- and the thing profile API returns no properties/actions.
--
-- This migration re-inserts the original January seed rows (full metadata)
-- AFTER the cleanup, so nothing downstream deletes them again.
-- Guards:
--   - UNIQUE(device_id, name) + INSERT OR IGNORE → idempotent, never
--     overwrites existing rows (incl. user-created ones with the same name);
--   - WHERE EXISTS (devices) → lineages without the demo devices get zero
--     rows, no orphans (FK integrity is enforced at startup).

INSERT OR IGNORE INTO thing_properties
    (id, device_id, name, display_name, description, data_type, unit, min_value, max_value)
SELECT column1, column2, column3, column4, column5, column6, column7, column8, column9
FROM (VALUES
-- 车间环境传感器
('prop-env01-temp',     'device-env-01', 'temperature',    '温度',         '当前环境温度',           'number', '°C',   -20, 60),
('prop-env01-humid',    'device-env-01', 'humidity',        '湿度',         '当前环境相对湿度',       'number', '%',    0,   100),
('prop-env01-pressure', 'device-env-01', 'pressure',        '气压',         '当前大气压',             'number', 'hPa',  900, 1100),
('prop-env01-co2',      'device-env-01', 'co2_level',       'CO₂浓度',     '二氧化碳浓度',           'number', 'ppm',  0,   5000),
('prop-env01-battery',  'device-env-01', 'battery',         '电池电量',     '传感器电池剩余电量',     'number', '%',    0,   100),
-- 仓库环境传感器
('prop-env02-temp',     'device-env-02', 'temperature',     '温度',         '当前环境温度',           'number', '°C',   -20, 60),
('prop-env02-humid',    'device-env-02', 'humidity',        '湿度',         '当前环境相对湿度',       'number', '%',    0,   100),
('prop-env02-battery',  'device-env-02', 'battery',         '电池电量',     '传感器电池剩余电量',     'number', '%',    0,   100),
-- 冷链冰箱温度仪
('prop-cold-temp',      'device-cold-01', 'temperature',    '温度',         '冰箱内部温度',           'number', '°C',   -30, 10),
('prop-cold-door',      'device-cold-01', 'door_open',      '门状态',       '冰箱门是否打开',         'boolean','',     NULL, NULL),
('prop-cold-humidity',  'device-cold-01', 'humidity',       '湿度',         '冰箱内部湿度',           'number', '%',    0,   100),
('prop-cold-runtime',   'device-cold-01', 'runtime_hours',  '运行时长',     '压缩机累计运行小时数',   'number', 'h',    0,   NULL),
-- 入口摄像头
('prop-cam01-status',   'device-cam-01', 'power_status',    '电源状态',     '摄像头供电状态',         'boolean','',     NULL, NULL),
('prop-cam01-res',      'device-cam-01', 'resolution',      '分辨率',       '当前视频分辨率',         'string', '',     NULL, NULL),
('prop-cam01-motion',   'device-cam-01', 'motion_detected', '移动侦测',     '是否检测到移动物体',     'boolean','',     NULL, NULL),
('prop-cam01-storage',  'device-cam-01', 'storage_used',    '存储使用率',   'SD卡/NVR存储使用百分比', 'number', '%',    0,   100),
-- 车间摄像头
('prop-cam02-status',   'device-cam-02', 'power_status',    '电源状态',     '摄像头供电状态',         'boolean','',     NULL, NULL),
('prop-cam02-motion',   'device-cam-02', 'motion_detected', '移动侦测',     '是否检测到移动物体',     'boolean','',     NULL, NULL),
-- 装配机器人
('prop-robot-pos_x',    'device-robot-01', 'pos_x',         'X轴位置',     '机器人末端X坐标',        'number', 'mm',   -500, 500),
('prop-robot-pos_y',    'device-robot-01', 'pos_y',         'Y轴位置',     '机器人末端Y坐标',        'number', 'mm',   -500, 500),
('prop-robot-pos_z',    'device-robot-01', 'pos_z',         'Z轴位置',     '机器人末端Z坐标',        'number', 'mm',    0,   800),
('prop-robot-torque',   'device-robot-01', 'torque',        '关节扭矩',     '当前关节平均扭矩',       'number', 'Nm',   0,   150),
('prop-robot-speed',    'device-robot-01', 'speed',         '运行速度',     '当前运行速度百分比',     'number', '%',    0,   100),
('prop-robot-status',   'device-robot-01', 'run_status',    '运行状态',     '机器人运行状态',         'string', '',     NULL, NULL),
-- 边缘网关
('prop-gw-cpu',         'device-gw-01', 'cpu_usage',        'CPU使用率',    '网关CPU使用率',          'number', '%',    0,   100),
('prop-gw-memory',      'device-gw-01', 'memory_usage',     '内存使用率',   '网关内存使用率',          'number', '%',    0,   100),
('prop-gw-connected',   'device-gw-01', 'connected_devices','连接设备数',   '当前连接的子设备数量',    'number', '台',   0,   NULL),
('prop-gw-uptime',      'device-gw-01', 'uptime',           '运行时间',     '网关连续运行时间',        'number', 'h',    0,   NULL),
('prop-gw-network',     'device-gw-01', 'network_quality',  '网络质量',     '上行链路信号质量',        'number', '%',    0,   100),
-- 电力仪表
('prop-pow-voltage',    'device-power-01', 'voltage',       '电压',         '三相平均电压',           'number', 'V',    0,   500),
('prop-pow-current',    'device-power-01', 'current',       '电流',         '三相平均电流',           'number', 'A',    0,   1000),
('prop-pow-power',      'device-power-01', 'active_power',  '有功功率',     '当前总有功功率',         'number', 'kW',   0,   NULL),
('prop-pow-energy',     'device-power-01', 'energy_today',  '今日用电量',   '当日累计有功电能',       'number', 'kWh',  0,   NULL),
('prop-pow-pf',         'device-power-01', 'power_factor',  '功率因数',     '当前功率因数',           'number', '',     0,   1),
('prop-pow-frequency',  'device-power-01', 'frequency',     '频率',         '电网频率',               'number', 'Hz',   45,  65)
)
WHERE EXISTS (SELECT 1 FROM devices d WHERE d.id = column2);

INSERT OR IGNORE INTO thing_actions
    (id, device_id, name, display_name, description, parameters)
SELECT column1, column2, column3, column4, column5, column6
FROM (VALUES
-- 环境传感器
('cmd-env01-restart',    'device-env-01',  'restart',        '重启设备',     '远程重启传感器',                    '{}'),
('cmd-env01-calibrate',  'device-env-01',  'calibrate',      '校准传感器',   '执行温湿度校准',                    '{}'),
-- 冷链温度仪
('cmd-cold-alarm',       'device-cold-01', 'set_alarm',      '设置告警阈值', '设置温度告警上下限',                '{"high": 8, "low": -25}'),
('cmd-cold-report',      'device-cold-01', 'force_report',   '强制上报',     '立即上报当前温度数据',              '{}'),
-- 摄像头
('cmd-cam01-snapshot',   'device-cam-01',  'snapshot',       '拍照',         '抓拍一张高清照片',                  '{}'),
('cmd-cam01-reboot',     'device-cam-01',  'reboot',         '重启摄像头',   '远程重启摄像头',                    '{}'),
('cmd-cam01-ptz',        'device-cam-01',  'ptz_control',    '云台控制',     '控制云台转动方向',                  '{"direction": "left", "speed": 5}'),
-- 机器人
('cmd-robot-move',       'device-robot-01','move_to',        '移动到位置',   '移动到指定坐标',                    '{"x": 0, "y": 0, "z": 100}'),
('cmd-robot-stop',       'device-robot-01','emergency_stop', '紧急停止',     '立即停止所有运动',                  '{}'),
('cmd-robot-home',       'device-robot-01','go_home',        '回零',         '回到机械零点位置',                  '{}'),
('cmd-robot-speed',      'device-robot-01','set_speed',      '设置速度',     '设置运行速度百分比',                '{"speed": 50}'),
-- 网关
('cmd-gw-restart',       'device-gw-01',   'restart',        '重启网关',     '远程重启边缘网关',                  '{}'),
('cmd-gw-update',        'device-gw-01',   'firmware_update','固件升级',     '升级网关固件到指定版本',            '{"version": "1.1.0"}'),
-- 电力仪表
('cmd-power-reset',      'device-power-01','reset_energy',   '电能清零',     '重置累计电能计数器',                '{}'),
('cmd-power-report',     'device-power-01','force_report',   '强制上报',     '立即上报当前电力参数',              '{}')
)
WHERE EXISTS (SELECT 1 FROM devices d WHERE d.id = column2);
