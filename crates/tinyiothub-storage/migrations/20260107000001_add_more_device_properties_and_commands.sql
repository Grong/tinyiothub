-- 为现有设备添加更多属性和命令，使数据更加完整和真实

-- ============================================================================
-- 为温度传感器 (device-001) 添加更多属性
-- ============================================================================

INSERT INTO device_properties (id, device_id, name, display_name, description, data_type, unit, min_value, max_value, is_read_only, created_at) VALUES
-- 温度传感器额外属性
('prop-006', 'device-001', 'alarm_high_temp', '高温报警阈值', '温度超过此值时触发报警', 'number', '°C', 0, 200, 0, datetime('now')),
('prop-007', 'device-001', 'alarm_low_temp', '低温报警阈值', '温度低于此值时触发报警', 'number', '°C', -50, 100, 0, datetime('now')),
('prop-008', 'device-001', 'sampling_interval', '采样间隔', '数据采样时间间隔', 'number', '秒', 1, 3600, 0, datetime('now')),
('prop-009', 'device-001', 'device_status', '设备状态', '设备运行状态', 'string', '', NULL, NULL, 1, datetime('now')),
('prop-010', 'device-001', 'last_calibration', '上次校准时间', '设备上次校准的时间', 'string', '', NULL, NULL, 1, datetime('now')),
('prop-011', 'device-001', 'firmware_version', '固件版本', '设备固件版本号', 'string', '', NULL, NULL, 1, datetime('now')),
('prop-012', 'device-001', 'power_status', '电源状态', '设备电源供电状态', 'boolean', '', NULL, NULL, 1, datetime('now'));

-- ============================================================================
-- 为湿度传感器 (device-002) 添加更多属性
-- ============================================================================

INSERT INTO device_properties (id, device_id, name, display_name, description, data_type, unit, min_value, max_value, is_read_only, created_at) VALUES
-- 湿度传感器额外属性
('prop-013', 'device-002', 'alarm_high_humidity', '高湿度报警阈值', '湿度超过此值时触发报警', 'number', '%', 0, 100, 0, datetime('now')),
('prop-014', 'device-002', 'alarm_low_humidity', '低湿度报警阈值', '湿度低于此值时触发报警', 'number', '%', 0, 100, 0, datetime('now')),
('prop-015', 'device-002', 'sampling_interval', '采样间隔', '数据采样时间间隔', 'number', '秒', 1, 3600, 0, datetime('now')),
('prop-016', 'device-002', 'device_status', '设备状态', '设备运行状态', 'string', '', NULL, NULL, 1, datetime('now')),
('prop-017', 'device-002', 'last_calibration', '上次校准时间', '设备上次校准的时间', 'string', '', NULL, NULL, 1, datetime('now')),
('prop-018', 'device-002', 'firmware_version', '固件版本', '设备固件版本号', 'string', '', NULL, NULL, 1, datetime('now')),
('prop-019', 'device-002', 'power_status', '电源状态', '设备电源供电状态', 'boolean', '', NULL, NULL, 1, datetime('now')),
('prop-020', 'device-002', 'sensor_accuracy', '传感器精度', '湿度传感器测量精度', 'number', '%', 0, 10, 1, datetime('now'));

-- ============================================================================
-- 为入口监控摄像头 (device-003) 添加更多属性
-- ============================================================================

INSERT INTO device_properties (id, device_id, name, display_name, description, data_type, unit, min_value, max_value, is_read_only, created_at) VALUES
-- 摄像头额外属性
('prop-021', 'device-003', 'resolution', '分辨率', '视频分辨率设置', 'string', '', NULL, NULL, 0, datetime('now')),
('prop-022', 'device-003', 'frame_rate', '帧率', '视频帧率设置', 'number', 'fps', 1, 60, 0, datetime('now')),
('prop-023', 'device-003', 'brightness', '亮度', '图像亮度调节', 'number', '', 0, 100, 0, datetime('now')),
('prop-024', 'device-003', 'contrast', '对比度', '图像对比度调节', 'number', '', 0, 100, 0, datetime('now')),
('prop-025', 'device-003', 'zoom_level', '变焦级别', '摄像头变焦倍数', 'number', 'x', 1, 10, 0, datetime('now')),
('prop-026', 'device-003', 'night_vision', '夜视模式', '是否启用夜视功能', 'boolean', '', NULL, NULL, 0, datetime('now')),
('prop-027', 'device-003', 'motion_detection', '运动检测', '是否启用运动检测', 'boolean', '', NULL, NULL, 0, datetime('now')),
('prop-028', 'device-003', 'recording_status', '录制状态', '当前录制状态', 'string', '', NULL, NULL, 1, datetime('now')),
('prop-029', 'device-003', 'storage_usage', '存储使用率', '存储空间使用百分比', 'number', '%', 0, 100, 1, datetime('now')),
('prop-030', 'device-003', 'network_status', '网络状态', '网络连接状态', 'string', '', NULL, NULL, 1, datetime('now'));

-- ============================================================================
-- 为装配机器人 (device-004) 添加更多属性
-- ============================================================================

INSERT INTO device_properties (id, device_id, name, display_name, description, data_type, unit, min_value, max_value, is_read_only, created_at) VALUES
-- 机器人额外属性
('prop-031', 'device-004', 'joint1_angle', '关节1角度', '机器人关节1当前角度', 'number', '度', -180, 180, 1, datetime('now')),
('prop-032', 'device-004', 'joint2_angle', '关节2角度', '机器人关节2当前角度', 'number', '度', -180, 180, 1, datetime('now')),
('prop-033', 'device-004', 'joint3_angle', '关节3角度', '机器人关节3当前角度', 'number', '度', -180, 180, 1, datetime('now')),
('prop-034', 'device-004', 'joint4_angle', '关节4角度', '机器人关节4当前角度', 'number', '度', -180, 180, 1, datetime('now')),
('prop-035', 'device-004', 'joint5_angle', '关节5角度', '机器人关节5当前角度', 'number', '度', -180, 180, 1, datetime('now')),
('prop-036', 'device-004', 'joint6_angle', '关节6角度', '机器人关节6当前角度', 'number', '度', -180, 180, 1, datetime('now')),
('prop-037', 'device-004', 'speed', '运动速度', '机器人运动速度设置', 'number', '%', 1, 100, 0, datetime('now')),
('prop-038', 'device-004', 'payload_weight', '负载重量', '当前负载重量', 'number', 'kg', 0, 50, 1, datetime('now')),
('prop-039', 'device-004', 'operation_mode', '操作模式', '机器人当前操作模式', 'string', '', NULL, NULL, 0, datetime('now')),
('prop-040', 'device-004', 'safety_status', '安全状态', '机器人安全系统状态', 'string', '', NULL, NULL, 1, datetime('now')),
('prop-041', 'device-004', 'error_code', '错误代码', '当前错误代码', 'string', '', NULL, NULL, 1, datetime('now')),
('prop-042', 'device-004', 'cycle_count', '循环计数', '完成的工作循环次数', 'number', '次', 0, NULL, 1, datetime('now'));

-- ============================================================================
-- 为车间监控摄像头 (device-005) 添加更多属性
-- ============================================================================

INSERT INTO device_properties (id, device_id, name, display_name, description, data_type, unit, min_value, max_value, is_read_only, created_at) VALUES
-- 车间摄像头额外属性
('prop-043', 'device-005', 'resolution', '分辨率', '视频分辨率设置', 'string', '', NULL, NULL, 0, datetime('now')),
('prop-044', 'device-005', 'frame_rate', '帧率', '视频帧率设置', 'number', 'fps', 1, 60, 0, datetime('now')),
('prop-045', 'device-005', 'brightness', '亮度', '图像亮度调节', 'number', '', 0, 100, 0, datetime('now')),
('prop-046', 'device-005', 'contrast', '对比度', '图像对比度调节', 'number', '', 0, 100, 0, datetime('now')),
('prop-047', 'device-005', 'pan_angle', '水平角度', '摄像头水平旋转角度', 'number', '度', -180, 180, 0, datetime('now')),
('prop-048', 'device-005', 'tilt_angle', '垂直角度', '摄像头垂直旋转角度', 'number', '度', -90, 90, 0, datetime('now')),
('prop-049', 'device-005', 'zoom_level', '变焦级别', '摄像头变焦倍数', 'number', 'x', 1, 20, 0, datetime('now')),
('prop-050', 'device-005', 'night_vision', '夜视模式', '是否启用夜视功能', 'boolean', '', NULL, NULL, 0, datetime('now')),
('prop-051', 'device-005', 'motion_detection', '运动检测', '是否启用运动检测', 'boolean', '', NULL, NULL, 0, datetime('now')),
('prop-052', 'device-005', 'recording_status', '录制状态', '当前录制状态', 'string', '', NULL, NULL, 1, datetime('now')),
('prop-053', 'device-005', 'storage_usage', '存储使用率', '存储空间使用百分比', 'number', '%', 0, 100, 1, datetime('now')),
('prop-054', 'device-005', 'network_status', '网络状态', '网络连接状态', 'string', '', NULL, NULL, 1, datetime('now'));

-- ============================================================================
-- 为温度传感器 (device-001) 添加更多命令
-- ============================================================================

INSERT INTO device_commands (id, device_id, name, display_name, description, parameters, created_at) VALUES
-- 温度传感器命令
('cmd-004', 'device-001', 'set_alarm_thresholds', '设置报警阈值', '设置高低温报警阈值', '{"high_temp": 80, "low_temp": 10}', datetime('now')),
('cmd-005', 'device-001', 'calibrate_sensor', '校准传感器', '执行传感器校准程序', '{"reference_temp": 25}', datetime('now')),
('cmd-006', 'device-001', 'set_sampling_rate', '设置采样频率', '调整数据采样间隔', '{"interval_seconds": 60}', datetime('now')),
('cmd-007', 'device-001', 'reset_device', '重启设备', '重启温度传感器设备', '{}', datetime('now')),
('cmd-008', 'device-001', 'get_diagnostics', '获取诊断信息', '获取设备诊断和状态信息', '{}', datetime('now'));

-- ============================================================================
-- 为湿度传感器 (device-002) 添加更多命令
-- ============================================================================

INSERT INTO device_commands (id, device_id, name, display_name, description, parameters, created_at) VALUES
-- 湿度传感器命令
('cmd-009', 'device-002', 'set_alarm_thresholds', '设置报警阈值', '设置高低湿度报警阈值', '{"high_humidity": 90, "low_humidity": 20}', datetime('now')),
('cmd-010', 'device-002', 'calibrate_sensor', '校准传感器', '执行传感器校准程序', '{"reference_humidity": 50}', datetime('now')),
('cmd-011', 'device-002', 'set_sampling_rate', '设置采样频率', '调整数据采样间隔', '{"interval_seconds": 60}', datetime('now')),
('cmd-012', 'device-002', 'reset_device', '重启设备', '重启湿度传感器设备', '{}', datetime('now')),
('cmd-013', 'device-002', 'get_diagnostics', '获取诊断信息', '获取设备诊断和状态信息', '{}', datetime('now'));

-- ============================================================================
-- 为入口监控摄像头 (device-003) 添加更多命令
-- ============================================================================

INSERT INTO device_commands (id, device_id, name, display_name, description, parameters, created_at) VALUES
-- 入口摄像头命令
('cmd-014', 'device-003', 'set_resolution', '设置分辨率', '调整视频分辨率', '{"width": 1920, "height": 1080}', datetime('now')),
('cmd-015', 'device-003', 'set_frame_rate', '设置帧率', '调整视频帧率', '{"fps": 30}', datetime('now')),
('cmd-016', 'device-003', 'adjust_brightness', '调整亮度', '调整图像亮度', '{"brightness": 50}', datetime('now')),
('cmd-017', 'device-003', 'adjust_contrast', '调整对比度', '调整图像对比度', '{"contrast": 50}', datetime('now')),
('cmd-018', 'device-003', 'set_zoom', '设置变焦', '调整摄像头变焦级别', '{"zoom_level": 1}', datetime('now')),
('cmd-019', 'device-003', 'toggle_night_vision', '切换夜视模式', '开启或关闭夜视功能', '{"enabled": true}', datetime('now')),
('cmd-020', 'device-003', 'toggle_motion_detection', '切换运动检测', '开启或关闭运动检测', '{"enabled": true}', datetime('now')),
('cmd-021', 'device-003', 'start_recording', '开始录制', '开始视频录制', '{"duration_minutes": 60}', datetime('now')),
('cmd-022', 'device-003', 'stop_recording', '停止录制', '停止视频录制', '{}', datetime('now')),
('cmd-023', 'device-003', 'reboot_camera', '重启摄像头', '重启摄像头设备', '{}', datetime('now'));

-- ============================================================================
-- 为装配机器人 (device-004) 添加更多命令
-- ============================================================================

INSERT INTO device_commands (id, device_id, name, display_name, description, parameters, created_at) VALUES
-- 机器人命令
('cmd-024', 'device-004', 'set_joint_angles', '设置关节角度', '设置所有关节的目标角度', '{"joint1": 0, "joint2": 0, "joint3": 0, "joint4": 0, "joint5": 0, "joint6": 0}', datetime('now')),
('cmd-025', 'device-004', 'set_speed', '设置运动速度', '调整机器人运动速度', '{"speed_percent": 50}', datetime('now')),
('cmd-026', 'device-004', 'home_position', '回到原点', '机器人回到初始位置', '{}', datetime('now')),
('cmd-027', 'device-004', 'emergency_stop', '紧急停止', '立即停止所有运动', '{}', datetime('now')),
('cmd-028', 'device-004', 'set_operation_mode', '设置操作模式', '切换机器人操作模式', '{"mode": "auto"}', datetime('now')),
('cmd-029', 'device-004', 'start_program', '启动程序', '执行预设的工作程序', '{"program_id": 1}', datetime('now')),
('cmd-030', 'device-004', 'pause_program', '暂停程序', '暂停当前执行的程序', '{}', datetime('now')),
('cmd-031', 'device-004', 'resume_program', '恢复程序', '恢复暂停的程序执行', '{}', datetime('now')),
('cmd-032', 'device-004', 'reset_error', '清除错误', '清除错误状态', '{}', datetime('now')),
('cmd-033', 'device-004', 'get_status', '获取状态', '获取机器人详细状态信息', '{}', datetime('now'));

-- ============================================================================
-- 为车间监控摄像头 (device-005) 添加更多命令
-- ============================================================================

INSERT INTO device_commands (id, device_id, name, display_name, description, parameters, created_at) VALUES
-- 车间摄像头命令
('cmd-034', 'device-005', 'set_resolution', '设置分辨率', '调整视频分辨率', '{"width": 1920, "height": 1080}', datetime('now')),
('cmd-035', 'device-005', 'set_frame_rate', '设置帧率', '调整视频帧率', '{"fps": 30}', datetime('now')),
('cmd-036', 'device-005', 'adjust_brightness', '调整亮度', '调整图像亮度', '{"brightness": 50}', datetime('now')),
('cmd-037', 'device-005', 'adjust_contrast', '调整对比度', '调整图像对比度', '{"contrast": 50}', datetime('now')),
('cmd-038', 'device-005', 'pan_tilt', '云台控制', '控制摄像头水平和垂直旋转', '{"pan_angle": 0, "tilt_angle": 0}', datetime('now')),
('cmd-039', 'device-005', 'set_zoom', '设置变焦', '调整摄像头变焦级别', '{"zoom_level": 1}', datetime('now')),
('cmd-040', 'device-005', 'toggle_night_vision', '切换夜视模式', '开启或关闭夜视功能', '{"enabled": true}', datetime('now')),
('cmd-041', 'device-005', 'toggle_motion_detection', '切换运动检测', '开启或关闭运动检测', '{"enabled": true}', datetime('now')),
('cmd-042', 'device-005', 'start_recording', '开始录制', '开始视频录制', '{"duration_minutes": 60}', datetime('now')),
('cmd-043', 'device-005', 'stop_recording', '停止录制', '停止视频录制', '{}', datetime('now')),
('cmd-044', 'device-005', 'preset_position', '预设位置', '移动到预设的监控位置', '{"preset_id": 1}', datetime('now')),
('cmd-045', 'device-005', 'patrol_mode', '巡航模式', '启动自动巡航监控', '{"patrol_points": [1, 2, 3], "interval_seconds": 30}', datetime('now')),
('cmd-046', 'device-005', 'reboot_camera', '重启摄像头', '重启摄像头设备', '{}', datetime('now'));

-- ============================================================================
-- 添加更多告警规则
-- ============================================================================

INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, created_by, created_at) VALUES
-- 温度传感器告警规则
('alarm-rule-004', 'device-001', 'prop-006', '自定义高温告警', 'threshold', '{"operator": "gt", "property": "alarm_high_temp"}', 'warning', 'admin-user-001', datetime('now')),
('alarm-rule-005', 'device-001', 'prop-007', '自定义低温告警', 'threshold', '{"operator": "lt", "property": "alarm_low_temp"}', 'warning', 'admin-user-001', datetime('now')),

-- 湿度传感器告警规则
('alarm-rule-006', 'device-002', 'prop-013', '自定义高湿度告警', 'threshold', '{"operator": "gt", "property": "alarm_high_humidity"}', 'warning', 'admin-user-001', datetime('now')),
('alarm-rule-007', 'device-002', 'prop-014', '自定义低湿度告警', 'threshold', '{"operator": "lt", "property": "alarm_low_humidity"}', 'warning', 'admin-user-001', datetime('now')),

-- 摄像头存储告警
('alarm-rule-008', 'device-003', 'prop-029', '存储空间不足', 'threshold', '{"operator": "gt", "value": 85}', 'warning', 'admin-user-001', datetime('now')),
('alarm-rule-009', 'device-005', 'prop-053', '存储空间不足', 'threshold', '{"operator": "gt", "value": 85}', 'warning', 'admin-user-001', datetime('now')),

-- 机器人安全告警
('alarm-rule-010', 'device-004', 'prop-040', '机器人安全状态异常', 'change', '{"from": "safe", "to": "warning"}', 'error', 'admin-user-001', datetime('now'));

-- ============================================================================
-- 添加一些示例告警实例
-- ============================================================================

INSERT INTO device_alarms (id, device_id, property_id, rule_id, alarm_level, alarm_message, alarm_value, threshold_value, alarm_time, created_at) VALUES
-- 历史告警记录
('alarm-003', 'device-003', 'prop-029', 'alarm-rule-008', 'warning', '入口摄像头存储空间使用率过高', '88', '85', datetime('now', '-3 hours'), datetime('now', '-3 hours')),
('alarm-004', 'device-004', 'prop-040', 'alarm-rule-010', 'error', '装配机器人安全状态异常', 'warning', 'safe', datetime('now', '-1 hour'), datetime('now', '-1 hour')),
('alarm-005', 'device-002', 'prop-002', 'alarm-rule-003', 'warning', '车间湿度传感器湿度过高', '92', '90', datetime('now', '-30 minutes'), datetime('now', '-30 minutes'));