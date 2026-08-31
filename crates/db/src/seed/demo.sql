-- seed_demo: demo-scenario seed rows (extracted from the pre-baseline
-- migration chain; see crates/db/src/seed.rs for the provenance table).
-- Requires seed_system to have run first (things reference the default
-- tenant/workspace; tags/alarm rules reference the admin user).
-- Every statement is idempotent (INSERT OR IGNORE / WHERE EXISTS guards).

-- ── demo things (20260106000002) ───────────────────────────────────────────
-- Baseline things table: product_id dropped by the 20260723000001 thing
-- rebuild; tenant_id/workspace_id point at the seed_system defaults (the old
-- chain backfilled those via UPDATE in 20260407000001).
INSERT OR IGNORE INTO things (id, name, display_name, category, address, description, driver_name, protocol_type, state, tenant_id, workspace_id, driver_options) VALUES
('device-env-01',    'env_sensor_workshop',   '车间环境传感器',     '环境传感器', '192.168.1.100:502',  'A栋生产车间温湿度监测',     'simulator', 'Modbus RTU', 1, 'tenant-default-001', 'ws-default-001', '{"interval":"2000","mode":"sine"}'),
('device-env-02',    'env_sensor_warehouse',  '仓库环境传感器',     '环境传感器', '192.168.1.101:502',  'B栋原料仓库环境监测',       'simulator', 'Modbus RTU', 1, 'tenant-default-001', 'ws-default-001', '{"interval":"3000","mode":"random"}'),
('device-cold-01',   'cold_chain_fridge',     '冷链冰箱温度仪',     '温度记录仪', '192.168.1.110:502',  '药品冷链存储温度监控',       'simulator', 'Modbus RTU', 1, 'tenant-default-001', 'ws-default-001', '{"interval":"5000","mode":"sine"}'),
('device-cam-01',    'camera_entrance',       '工厂入口摄像头',     '网络摄像头', '192.168.1.200',      '正门出入口高清监控',         'simulator', 'ONVIF',      1, 'tenant-default-001', 'ws-default-001', '{"interval":"1000"}'),
('device-cam-02',    'camera_workshop',       '车间监控摄像头',     '网络摄像头', '192.168.1.201',      'A栋车间生产线监控',         'simulator', 'ONVIF',      0, 'tenant-default-001', 'ws-default-001', '{"interval":"1000"}'),
('device-robot-01',  'robot_arm_assembly',    '装配机器人1号',      '协作机器人', '192.168.1.50:8080',  '3号产线产品装配作业',       'simulator', 'TCP/IP',     1, 'tenant-default-001', 'ws-default-001', '{"interval":"500","mode":"sine"}'),
('device-gw-01',     'gateway_floor1',        '一楼边缘网关',       '边缘网关',   '192.168.1.10:1883',  '一楼设备汇聚网关',           'simulator', 'MQTT',       1, 'tenant-default-001', 'ws-default-001', '{"interval":"3000"}'),
('device-power-01',  'power_meter_main',      '总配电电力仪表',     '电力仪表',   '192.168.1.220:502',  '厂区总配电柜电力参数监测',   'simulator', 'Modbus TCP', 1, 'tenant-default-001', 'ws-default-001', '{"interval":"2000","mode":"random"}');

-- ── per-thing properties + actions (20260818000001, verbatim) ───────────────
-- The restore migration re-inserted the January per-device seed rows into
-- thing_properties/thing_actions after the thing-ontology rebuild; the
-- WHERE EXISTS(things) guard keeps the rows FK-clean.
INSERT OR IGNORE INTO thing_properties
    (id, thing_id, name, display_name, description, data_type, unit, min_value, max_value)
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
WHERE EXISTS (SELECT 1 FROM things d WHERE d.id = column2);

INSERT OR IGNORE INTO thing_actions
    (id, thing_id, name, display_name, description, parameters)
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
WHERE EXISTS (SELECT 1 FROM things d WHERE d.id = column2);

-- ── demo tags + bindings (20260106000002) ───────────────────────────────────
INSERT OR IGNORE INTO tags (id, type, name, description, color, tenant_id, created_by) VALUES
('tag-device-001', 'thing', '温度传感器', '温度监测设备', '#FF6B6B', 'tenant-default-001', 'admin-user-001'),
('tag-device-002', 'thing', '湿度传感器', '湿度监测设备', '#4ECDC4', 'tenant-default-001', 'admin-user-001'),
('tag-device-003', 'thing', '摄像头', '视频监控设备', '#45B7D1', 'tenant-default-001', 'admin-user-001'),
('tag-device-004', 'thing', '机器人', '自动化设备', '#96CEB4', 'tenant-default-001', 'admin-user-001'),
('tag-device-005', 'thing', '在线设备', '当前在线的设备', '#FFEAA7', 'tenant-default-001', 'admin-user-001'),
('tag-device-006', 'thing', '离线设备', '当前离线的设备', '#DDA0DD', 'tenant-default-001', 'admin-user-001'),
('tag-device-007', 'thing', '生产设备', '生产相关设备', '#98D8C8', 'tenant-default-001', 'admin-user-001'),
('tag-device-008', 'thing', '监控设备', '监控相关设备', '#F7DC6F', 'tenant-default-001', 'admin-user-001'),
('tag-app-001', 'app', '生产环境', '生产环境应用', '#52C41A', 'tenant-default-001', 'admin-user-001'),
('tag-app-002', 'app', '测试环境', '测试环境应用', '#1890FF', 'tenant-default-001', 'admin-user-001'),
('tag-app-003', 'app', '开发环境', '开发环境应用', '#722ED1', 'tenant-default-001', 'admin-user-001');

INSERT OR IGNORE INTO tag_bindings (id, tag_id, target_id, target_type, tenant_id, created_by) VALUES
('binding-001', 'tag-device-001', 'device-env-01', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-002', 'tag-device-005', 'device-env-01', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-003', 'tag-device-007', 'device-env-01', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-004', 'tag-device-002', 'device-env-02', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-005', 'tag-device-005', 'device-env-02', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-006', 'tag-device-007', 'device-env-02', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-007', 'tag-device-003', 'device-cam-01', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-008', 'tag-device-005', 'device-cam-01', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-009', 'tag-device-008', 'device-cam-01', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-010', 'tag-device-003', 'device-cam-02', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-011', 'tag-device-006', 'device-cam-02', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-012', 'tag-device-008', 'device-cam-02', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-013', 'tag-device-004', 'device-robot-01', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-014', 'tag-device-005', 'device-robot-01', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-015', 'tag-device-007', 'device-robot-01', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-016', 'tag-device-005', 'device-gw-01', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-017', 'tag-device-007', 'device-gw-01', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-018', 'tag-device-005', 'device-power-01', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-019', 'tag-device-008', 'device-power-01', 'device', 'tenant-default-001', 'admin-user-001');

-- ── demo alarm rules + sample alarms (20260106000002) ───────────────────────
INSERT OR IGNORE INTO thing_alarm_rules (id, thing_id, property_id, rule_name, rule_type, condition_config, alarm_level, created_by) VALUES
('alarm-rule-001', 'device-env-01', 'prop-env01-temp', '车间高温告警',   'threshold', '{"operator": "gt", "value": 45}',  'warning',  'admin-user-001'),
('alarm-rule-002', 'device-env-01', 'prop-env01-temp', '车间超高温告警', 'threshold', '{"operator": "gt", "value": 55}',  'critical', 'admin-user-001'),
('alarm-rule-003', 'device-env-02', 'prop-env02-humid','仓库高湿度告警', 'threshold', '{"operator": "gt", "value": 85}',  'warning',  'admin-user-001'),
('alarm-rule-004', 'device-cold-01','prop-cold-temp',  '冷链超温告警',   'threshold', '{"operator": "gt", "value": 8}',   'critical', 'admin-user-001'),
('alarm-rule-005', 'device-cold-01','prop-cold-temp',  '冷链低温告警',   'threshold', '{"operator": "lt", "value": -25}', 'warning',  'admin-user-001'),
('alarm-rule-006', 'device-power-01','prop-pow-voltage','电压过高告警',  'threshold', '{"operator": "gt", "value": 420}',  'warning',  'admin-user-001'),
('alarm-rule-007', 'device-power-01','prop-pow-pf',    '功率因数过低',   'threshold', '{"operator": "lt", "value": 0.85}', 'warning',  'admin-user-001');

INSERT OR IGNORE INTO thing_alarms (id, thing_id, property_id, rule_id, alarm_level, alarm_message, alarm_value, threshold_value, alarm_time) VALUES
('alarm-001', 'device-env-01',  'prop-env01-temp',  'alarm-rule-001', 'warning',  '车间温度超过警告阈值',     '47.2',  '45',  datetime('now', '-2 hours')),
('alarm-002', 'device-env-02',  'prop-env02-humid', 'alarm-rule-003', 'warning',  '仓库湿度超过警告阈值',     '88.5',  '85',  datetime('now', '-1 hour')),
('alarm-003', 'device-cold-01', 'prop-cold-temp',   'alarm-rule-004', 'critical', '冷链冰箱温度异常偏高',     '10.3',  '8',   datetime('now', '-30 minutes')),
('alarm-004', 'device-power-01','prop-pow-pf',      'alarm-rule-007', 'warning',  '总配电功率因数偏低',       '0.82',  '0.85', datetime('now', '-45 minutes'));

-- ── sample jobs (20260312000001) ────────────────────────────────────────────
INSERT OR IGNORE INTO jobs (id, name, description, job_type, cron_expression, config, is_enabled, tags) VALUES
    ('job-001', '设备状态同步', '每5分钟同步一次设备在线状态', 'http', '*/5 * * * *',
     '{"url": "/api/things/sync-status", "method": "POST", "headers": {}}',
     true, '["系统", "设备"]'),
    ('job-002', '数据清理', '每天凌晨3点清理过期数据', 'script', '0 3 * * *',
     '{"script": "cleanup.sh", "working_dir": "/app/scripts"}',
     true, '["维护", "清理"]'),
    ('job-003', '健康检查', '每分钟检查系统健康状态', 'http', '*/1 * * * *',
     '{"url": "/api/health", "method": "GET"}',
     true, '["系统", "监控"]');

-- ── example notification channels (20260312000002) ──────────────────────────
INSERT OR IGNORE INTO notification_channels (id, name, channel_type, config, description) VALUES
    ('channel-sms-default', '系统短信', 'sms',
     '{"provider": "aliyun", "sign_name": "TinyIoT", "template_id": ""}',
     '系统默认短信渠道'),
    ('channel-email-default', '系统邮件', 'email',
     '{"provider": "smtp", "smtp_host": "", "smtp_port": 465, "from": "TinyIoT <noreply@tinyiot.com>"}',
     '系统默认邮件渠道'),
    ('channel-webhook-default', '钉钉 webhook', 'webhook',
     '{"url": "", "method": "POST", "headers": {"Content-Type": "application/json"}}',
     '系统默认钉钉 webhook 渠道');
