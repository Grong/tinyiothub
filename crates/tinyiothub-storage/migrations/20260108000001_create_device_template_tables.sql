-- 创建设备模板系统相关表
-- 此迁移脚本创建设备模板和模板分类表，支持设备模板管理功能

-- ============================================================================
-- 创建模板分类表
-- ============================================================================

CREATE TABLE IF NOT EXISTS template_categories (
    name TEXT PRIMARY KEY,
    display_name TEXT NOT NULL, -- JSON格式的多语言显示名称
    description TEXT, -- JSON格式的多语言描述
    sort_order INTEGER DEFAULT 0,
    is_active INTEGER DEFAULT 1,
    created_at TEXT NOT NULL
);

-- ============================================================================
-- 创建设备模板表
-- ============================================================================

CREATE TABLE IF NOT EXISTS device_templates (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL, -- JSON格式的多语言显示名称
    description TEXT,  -- JSON格式的多语言描述
    version TEXT NOT NULL,
    author TEXT,
    category TEXT NOT NULL,
    manufacturer TEXT,
    device_type TEXT NOT NULL,
    protocol_type TEXT,
    driver_name TEXT,
    tags TEXT NOT NULL, -- JSON数组格式
    device_info TEXT NOT NULL, -- JSON格式的DeviceInfo
    properties TEXT NOT NULL, -- JSON数组格式的PropertyTemplate
    commands TEXT NOT NULL, -- JSON数组格式的CommandTemplate
    is_builtin INTEGER DEFAULT 0, -- 是否为内置模板
    is_active INTEGER DEFAULT 1, -- 是否激活
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (category) REFERENCES template_categories(name)
);

-- ============================================================================
-- 创建索引以提高查询性能
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_device_templates_category ON device_templates(category);
CREATE INDEX IF NOT EXISTS idx_device_templates_device_type ON device_templates(device_type);
CREATE INDEX IF NOT EXISTS idx_device_templates_manufacturer ON device_templates(manufacturer);
CREATE INDEX IF NOT EXISTS idx_device_templates_protocol_type ON device_templates(protocol_type);
CREATE INDEX IF NOT EXISTS idx_device_templates_driver_name ON device_templates(driver_name);
CREATE INDEX IF NOT EXISTS idx_device_templates_is_builtin ON device_templates(is_builtin);
CREATE INDEX IF NOT EXISTS idx_device_templates_is_active ON device_templates(is_active);
CREATE INDEX IF NOT EXISTS idx_device_templates_created_at ON device_templates(created_at);

CREATE INDEX IF NOT EXISTS idx_template_categories_sort_order ON template_categories(sort_order);
CREATE INDEX IF NOT EXISTS idx_template_categories_is_active ON template_categories(is_active);

-- ============================================================================
-- 插入默认模板分类
-- ============================================================================

INSERT OR IGNORE INTO template_categories (name, display_name, description, sort_order, is_active, created_at) VALUES
('sensors', '{"zh": "传感器", "en": "Sensors"}', '{"zh": "各类传感器设备模板", "en": "Various sensor device templates"}', 1, 1, datetime('now')),
('cameras', '{"zh": "摄像头", "en": "Cameras"}', '{"zh": "监控摄像头设备模板", "en": "Surveillance camera device templates"}', 2, 1, datetime('now')),
('controllers', '{"zh": "控制器", "en": "Controllers"}', '{"zh": "各类控制器设备模板", "en": "Various controller device templates"}', 3, 1, datetime('now')),
('robots', '{"zh": "机器人", "en": "Robots"}', '{"zh": "工业机器人设备模板", "en": "Industrial robot device templates"}', 4, 1, datetime('now')),
('gateways', '{"zh": "网关", "en": "Gateways"}', '{"zh": "通信网关设备模板", "en": "Communication gateway device templates"}', 5, 1, datetime('now')),
('meters', '{"zh": "仪表", "en": "Meters"}', '{"zh": "各类仪表设备模板", "en": "Various meter device templates"}', 6, 1, datetime('now'));

-- ============================================================================
-- 插入内置设备模板
-- ============================================================================

-- 温度传感器模板
INSERT OR IGNORE INTO device_templates (
    id, name, display_name, description, version, author, category, manufacturer,
    device_type, protocol_type, driver_name, tags, device_info, properties, commands,
    is_builtin, is_active, created_at, updated_at
) VALUES (
    'builtin-temperature-sensor',
    'temperature_sensor',
    '{"zh": "温度传感器", "en": "Temperature Sensor"}',
    '{"zh": "标准温度传感器设备模板，支持温度监测和报警配置", "en": "Standard temperature sensor device template with temperature monitoring and alarm configuration"}',
    '1.0.0',
    'System',
    'sensors',
    NULL,
    'sensor',
    'modbus',
    'modbus_rtu',
    '["sensor", "temperature", "monitoring"]',
    '{"default_name_pattern": "temp_sensor_{index}", "default_display_name_pattern": "温度传感器 {index}", "default_description": {"zh": "温度监测传感器", "en": "Temperature monitoring sensor"}, "required_fields": ["name", "address"]}',
    '[
        {
            "name": "temperature",
            "display_name": {"zh": "温度", "en": "Temperature"},
            "description": {"zh": "当前环境温度", "en": "Current ambient temperature"},
            "data_type": "number",
            "unit": "°C",
            "min_value": -50.0,
            "max_value": 200.0,
            "default_value": "25.0",
            "is_read_only": true,
            "is_required": true
        },
        {
            "name": "alarm_high_temp",
            "display_name": {"zh": "高温报警阈值", "en": "High Temperature Alarm Threshold"},
            "description": {"zh": "温度超过此值时触发报警", "en": "Trigger alarm when temperature exceeds this value"},
            "data_type": "number",
            "unit": "°C",
            "min_value": 0.0,
            "max_value": 200.0,
            "default_value": "80.0",
            "is_read_only": false,
            "is_required": false
        },
        {
            "name": "alarm_low_temp",
            "display_name": {"zh": "低温报警阈值", "en": "Low Temperature Alarm Threshold"},
            "description": {"zh": "温度低于此值时触发报警", "en": "Trigger alarm when temperature below this value"},
            "data_type": "number",
            "unit": "°C",
            "min_value": -50.0,
            "max_value": 100.0,
            "default_value": "10.0",
            "is_read_only": false,
            "is_required": false
        },
        {
            "name": "sampling_interval",
            "display_name": {"zh": "采样间隔", "en": "Sampling Interval"},
            "description": {"zh": "数据采样时间间隔", "en": "Data sampling time interval"},
            "data_type": "number",
            "unit": "秒",
            "min_value": 1.0,
            "max_value": 3600.0,
            "default_value": "60.0",
            "is_read_only": false,
            "is_required": false
        }
    ]',
    '[
        {
            "name": "read_temperature",
            "display_name": {"zh": "读取温度", "en": "Read Temperature"},
            "description": {"zh": "读取当前温度值", "en": "Read current temperature value"},
            "parameters": "{}",
            "is_required": true
        },
        {
            "name": "set_alarm_thresholds",
            "display_name": {"zh": "设置报警阈值", "en": "Set Alarm Thresholds"},
            "description": {"zh": "设置高低温报警阈值", "en": "Set high and low temperature alarm thresholds"},
            "parameters": "{\"high_temp\": 80, \"low_temp\": 10}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"high_temp\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 200}, \"low_temp\": {\"type\": \"number\", \"minimum\": -50, \"maximum\": 100}}, \"required\": [\"high_temp\", \"low_temp\"]}",
            "is_required": false
        },
        {
            "name": "calibrate_sensor",
            "display_name": {"zh": "校准传感器", "en": "Calibrate Sensor"},
            "description": {"zh": "执行传感器校准程序", "en": "Execute sensor calibration procedure"},
            "parameters": "{\"reference_temp\": 25}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"reference_temp\": {\"type\": \"number\", \"minimum\": -50, \"maximum\": 200}}, \"required\": [\"reference_temp\"]}",
            "is_required": false
        }
    ]',
    1,
    1,
    datetime('now'),
    datetime('now')
);

-- ONVIF摄像头模板
INSERT OR IGNORE INTO device_templates (
    id, name, display_name, description, version, author, category, manufacturer,
    device_type, protocol_type, driver_name, tags, device_info, properties, commands,
    is_builtin, is_active, created_at, updated_at
) VALUES (
    'builtin-onvif-camera',
    'onvif_camera',
    '{"zh": "ONVIF摄像头", "en": "ONVIF Camera"}',
    '{"zh": "标准ONVIF协议摄像头设备模板，支持视频流和PTZ控制", "en": "Standard ONVIF protocol camera device template with video streaming and PTZ control"}',
    '1.0.0',
    'System',
    'cameras',
    NULL,
    'camera',
    'onvif',
    'onvif',
    '["camera", "onvif", "surveillance", "ptz"]',
    '{"default_name_pattern": "camera_{index}", "default_display_name_pattern": "摄像头 {index}", "default_description": {"zh": "ONVIF网络摄像头", "en": "ONVIF Network Camera"}, "required_fields": ["name", "address"]}',
    '[
        {
            "name": "resolution",
            "display_name": {"zh": "分辨率", "en": "Resolution"},
            "description": {"zh": "视频分辨率设置", "en": "Video resolution setting"},
            "data_type": "string",
            "default_value": "1920x1080",
            "is_read_only": false,
            "is_required": true
        },
        {
            "name": "frame_rate",
            "display_name": {"zh": "帧率", "en": "Frame Rate"},
            "description": {"zh": "视频帧率设置", "en": "Video frame rate setting"},
            "data_type": "number",
            "unit": "fps",
            "min_value": 1.0,
            "max_value": 60.0,
            "default_value": "30.0",
            "is_read_only": false,
            "is_required": true
        },
        {
            "name": "pan_angle",
            "display_name": {"zh": "水平角度", "en": "Pan Angle"},
            "description": {"zh": "摄像头水平旋转角度", "en": "Camera horizontal rotation angle"},
            "data_type": "number",
            "unit": "度",
            "min_value": -180.0,
            "max_value": 180.0,
            "default_value": "0.0",
            "is_read_only": false,
            "is_required": false
        },
        {
            "name": "tilt_angle",
            "display_name": {"zh": "垂直角度", "en": "Tilt Angle"},
            "description": {"zh": "摄像头垂直旋转角度", "en": "Camera vertical rotation angle"},
            "data_type": "number",
            "unit": "度",
            "min_value": -90.0,
            "max_value": 90.0,
            "default_value": "0.0",
            "is_read_only": false,
            "is_required": false
        },
        {
            "name": "zoom_level",
            "display_name": {"zh": "变焦级别", "en": "Zoom Level"},
            "description": {"zh": "摄像头变焦倍数", "en": "Camera zoom magnification"},
            "data_type": "number",
            "unit": "x",
            "min_value": 1.0,
            "max_value": 20.0,
            "default_value": "1.0",
            "is_read_only": false,
            "is_required": false
        }
    ]',
    '[
        {
            "name": "get_snapshot",
            "display_name": {"zh": "获取快照", "en": "Get Snapshot"},
            "description": {"zh": "获取当前视频快照", "en": "Get current video snapshot"},
            "parameters": "{}",
            "is_required": true
        },
        {
            "name": "pan_tilt",
            "display_name": {"zh": "云台控制", "en": "Pan Tilt Control"},
            "description": {"zh": "控制摄像头水平和垂直旋转", "en": "Control camera horizontal and vertical rotation"},
            "parameters": "{\"pan_angle\": 0, \"tilt_angle\": 0}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"pan_angle\": {\"type\": \"number\", \"minimum\": -180, \"maximum\": 180}, \"tilt_angle\": {\"type\": \"number\", \"minimum\": -90, \"maximum\": 90}}, \"required\": [\"pan_angle\", \"tilt_angle\"]}",
            "is_required": false
        },
        {
            "name": "set_zoom",
            "display_name": {"zh": "设置变焦", "en": "Set Zoom"},
            "description": {"zh": "调整摄像头变焦级别", "en": "Adjust camera zoom level"},
            "parameters": "{\"zoom_level\": 1}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"zoom_level\": {\"type\": \"number\", \"minimum\": 1, \"maximum\": 20}}, \"required\": [\"zoom_level\"]}",
            "is_required": false
        },
        {
            "name": "start_recording",
            "display_name": {"zh": "开始录制", "en": "Start Recording"},
            "description": {"zh": "开始视频录制", "en": "Start video recording"},
            "parameters": "{\"duration_minutes\": 60}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"duration_minutes\": {\"type\": \"number\", \"minimum\": 1, \"maximum\": 1440}}, \"required\": [\"duration_minutes\"]}",
            "is_required": false
        },
        {
            "name": "stop_recording",
            "display_name": {"zh": "停止录制", "en": "Stop Recording"},
            "description": {"zh": "停止视频录制", "en": "Stop video recording"},
            "parameters": "{}",
            "is_required": false
        }
    ]',
    1,
    1,
    datetime('now'),
    datetime('now')
);

-- Modbus RTU设备模板
INSERT OR IGNORE INTO device_templates (
    id, name, display_name, description, version, author, category, manufacturer,
    device_type, protocol_type, driver_name, tags, device_info, properties, commands,
    is_builtin, is_active, created_at, updated_at
) VALUES (
    'builtin-modbus-rtu-device',
    'modbus_rtu_device',
    '{"zh": "Modbus RTU设备", "en": "Modbus RTU Device"}',
    '{"zh": "标准Modbus RTU协议设备模板，支持寄存器读写操作", "en": "Standard Modbus RTU protocol device template with register read/write operations"}',
    '1.0.0',
    'System',
    'controllers',
    NULL,
    'controller',
    'modbus',
    'modbus_rtu',
    '["modbus", "rtu", "controller", "industrial"]',
    '{"default_name_pattern": "modbus_device_{index}", "default_display_name_pattern": "Modbus设备 {index}", "default_description": {"zh": "Modbus RTU工业设备", "en": "Modbus RTU Industrial Device"}, "required_fields": ["name", "address"]}',
    '[
        {
            "name": "holding_register_1",
            "display_name": {"zh": "保持寄存器1", "en": "Holding Register 1"},
            "description": {"zh": "保持寄存器地址1的值", "en": "Value of holding register address 1"},
            "data_type": "number",
            "min_value": 0.0,
            "max_value": 65535.0,
            "default_value": "0",
            "is_read_only": false,
            "is_required": false
        },
        {
            "name": "input_register_1",
            "display_name": {"zh": "输入寄存器1", "en": "Input Register 1"},
            "description": {"zh": "输入寄存器地址1的值", "en": "Value of input register address 1"},
            "data_type": "number",
            "min_value": 0.0,
            "max_value": 65535.0,
            "default_value": "0",
            "is_read_only": true,
            "is_required": false
        },
        {
            "name": "coil_1",
            "display_name": {"zh": "线圈1", "en": "Coil 1"},
            "description": {"zh": "线圈地址1的状态", "en": "Status of coil address 1"},
            "data_type": "boolean",
            "default_value": "false",
            "is_read_only": false,
            "is_required": false
        },
        {
            "name": "discrete_input_1",
            "display_name": {"zh": "离散输入1", "en": "Discrete Input 1"},
            "description": {"zh": "离散输入地址1的状态", "en": "Status of discrete input address 1"},
            "data_type": "boolean",
            "default_value": "false",
            "is_read_only": true,
            "is_required": false
        }
    ]',
    '[
        {
            "name": "read_holding_registers",
            "display_name": {"zh": "读取保持寄存器", "en": "Read Holding Registers"},
            "description": {"zh": "读取指定地址的保持寄存器", "en": "Read holding registers at specified address"},
            "parameters": "{\"address\": 1, \"count\": 1}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"address\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 65535}, \"count\": {\"type\": \"number\", \"minimum\": 1, \"maximum\": 125}}, \"required\": [\"address\", \"count\"]}",
            "is_required": true
        },
        {
            "name": "write_single_register",
            "display_name": {"zh": "写入单个寄存器", "en": "Write Single Register"},
            "description": {"zh": "写入单个保持寄存器的值", "en": "Write value to single holding register"},
            "parameters": "{\"address\": 1, \"value\": 0}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"address\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 65535}, \"value\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 65535}}, \"required\": [\"address\", \"value\"]}",
            "is_required": false
        },
        {
            "name": "read_coils",
            "display_name": {"zh": "读取线圈", "en": "Read Coils"},
            "description": {"zh": "读取指定地址的线圈状态", "en": "Read coil status at specified address"},
            "parameters": "{\"address\": 1, \"count\": 1}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"address\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 65535}, \"count\": {\"type\": \"number\", \"minimum\": 1, \"maximum\": 2000}}, \"required\": [\"address\", \"count\"]}",
            "is_required": false
        },
        {
            "name": "write_single_coil",
            "display_name": {"zh": "写入单个线圈", "en": "Write Single Coil"},
            "description": {"zh": "写入单个线圈的状态", "en": "Write status to single coil"},
            "parameters": "{\"address\": 1, \"value\": false}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"address\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 65535}, \"value\": {\"type\": \"boolean\"}}, \"required\": [\"address\", \"value\"]}",
            "is_required": false
        }
    ]',
    1,
    1,
    datetime('now'),
    datetime('now')
);

-- MQTT设备模板
INSERT OR IGNORE INTO device_templates (
    id, name, display_name, description, version, author, category, manufacturer,
    device_type, protocol_type, driver_name, tags, device_info, properties, commands,
    is_builtin, is_active, created_at, updated_at
) VALUES (
    'builtin-mqtt-device',
    'mqtt_device',
    '{"zh": "MQTT设备", "en": "MQTT Device"}',
    '{"zh": "标准MQTT协议设备模板，支持发布和订阅消息", "en": "Standard MQTT protocol device template with publish and subscribe capabilities"}',
    '1.0.0',
    'System',
    'gateways',
    NULL,
    'gateway',
    'mqtt',
    'mqtt',
    '["mqtt", "iot", "gateway", "messaging"]',
    '{"default_name_pattern": "mqtt_device_{index}", "default_display_name_pattern": "MQTT设备 {index}", "default_description": {"zh": "MQTT物联网设备", "en": "MQTT IoT Device"}, "required_fields": ["name", "address"]}',
    '[
        {
            "name": "connection_status",
            "display_name": {"zh": "连接状态", "en": "Connection Status"},
            "description": {"zh": "MQTT连接状态", "en": "MQTT connection status"},
            "data_type": "string",
            "default_value": "disconnected",
            "is_read_only": true,
            "is_required": true
        },
        {
            "name": "last_message_time",
            "display_name": {"zh": "最后消息时间", "en": "Last Message Time"},
            "description": {"zh": "最后收到消息的时间", "en": "Time of last received message"},
            "data_type": "string",
            "is_read_only": true,
            "is_required": false
        },
        {
            "name": "message_count",
            "display_name": {"zh": "消息计数", "en": "Message Count"},
            "description": {"zh": "收到的消息总数", "en": "Total number of received messages"},
            "data_type": "number",
            "min_value": 0.0,
            "default_value": "0",
            "is_read_only": true,
            "is_required": false
        },
        {
            "name": "qos_level",
            "display_name": {"zh": "QoS级别", "en": "QoS Level"},
            "description": {"zh": "消息质量服务级别", "en": "Message Quality of Service level"},
            "data_type": "number",
            "min_value": 0.0,
            "max_value": 2.0,
            "default_value": "1",
            "is_read_only": false,
            "is_required": false
        }
    ]',
    '[
        {
            "name": "publish_message",
            "display_name": {"zh": "发布消息", "en": "Publish Message"},
            "description": {"zh": "向指定主题发布消息", "en": "Publish message to specified topic"},
            "parameters": "{\"topic\": \"device/data\", \"payload\": \"{}\", \"qos\": 1}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"topic\": {\"type\": \"string\", \"minLength\": 1}, \"payload\": {\"type\": \"string\"}, \"qos\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 2}}, \"required\": [\"topic\", \"payload\"]}",
            "is_required": true
        },
        {
            "name": "subscribe_topic",
            "display_name": {"zh": "订阅主题", "en": "Subscribe Topic"},
            "description": {"zh": "订阅指定的MQTT主题", "en": "Subscribe to specified MQTT topic"},
            "parameters": "{\"topic\": \"device/command\", \"qos\": 1}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"topic\": {\"type\": \"string\", \"minLength\": 1}, \"qos\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 2}}, \"required\": [\"topic\"]}",
            "is_required": false
        },
        {
            "name": "unsubscribe_topic",
            "display_name": {"zh": "取消订阅", "en": "Unsubscribe Topic"},
            "description": {"zh": "取消订阅指定的MQTT主题", "en": "Unsubscribe from specified MQTT topic"},
            "parameters": "{\"topic\": \"device/command\"}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"topic\": {\"type\": \"string\", \"minLength\": 1}}, \"required\": [\"topic\"]}",
            "is_required": false
        },
        {
            "name": "get_connection_info",
            "display_name": {"zh": "获取连接信息", "en": "Get Connection Info"},
            "description": {"zh": "获取MQTT连接详细信息", "en": "Get detailed MQTT connection information"},
            "parameters": "{}",
            "is_required": false
        }
    ]',
    1,
    1,
    datetime('now'),
    datetime('now')
);

-- 智能开关模板
INSERT OR IGNORE INTO device_templates (
    id, name, display_name, description, version, author, category, manufacturer,
    device_type, protocol_type, driver_name, tags, device_info, properties, commands,
    is_builtin, is_active, created_at, updated_at
) VALUES (
    'builtin-smart-switch',
    'smart_switch',
    '{"zh": "智能开关", "en": "Smart Switch"}',
    '{"zh": "智能开关设备模板，支持开关状态控制和电流监测", "en": "Smart switch device template with switch control and current monitoring"}',
    '1.0.0',
    'System',
    'controllers',
    NULL,
    'switch',
    'wifi',
    'wifi_switch',
    '["switch", "smart", "control", "power"]',
    '{"default_name_pattern": "switch_{index}", "default_display_name_pattern": "智能开关 {index}", "default_description": {"zh": "智能电源开关", "en": "Smart Power Switch"}, "required_fields": ["name", "address"]}',
    '[
        {
            "name": "switch_state",
            "display_name": {"zh": "开关状态", "en": "Switch State"},
            "description": {"zh": "开关的当前状态", "en": "Current state of the switch"},
            "data_type": "boolean",
            "default_value": "false",
            "is_read_only": false,
            "is_required": true
        },
        {
            "name": "current",
            "display_name": {"zh": "电流", "en": "Current"},
            "description": {"zh": "当前电流值", "en": "Current electrical current"},
            "data_type": "number",
            "unit": "A",
            "min_value": 0.0,
            "max_value": 16.0,
            "default_value": "0.0",
            "is_read_only": true,
            "is_required": false
        },
        {
            "name": "power",
            "display_name": {"zh": "功率", "en": "Power"},
            "description": {"zh": "当前功率消耗", "en": "Current power consumption"},
            "data_type": "number",
            "unit": "W",
            "min_value": 0.0,
            "max_value": 3680.0,
            "default_value": "0.0",
            "is_read_only": true,
            "is_required": false
        },
        {
            "name": "energy_today",
            "display_name": {"zh": "今日用电量", "en": "Energy Today"},
            "description": {"zh": "今日累计用电量", "en": "Total energy consumption today"},
            "data_type": "number",
            "unit": "kWh",
            "min_value": 0.0,
            "default_value": "0.0",
            "is_read_only": true,
            "is_required": false
        }
    ]',
    '[
        {
            "name": "turn_on",
            "display_name": {"zh": "打开开关", "en": "Turn On"},
            "description": {"zh": "打开智能开关", "en": "Turn on the smart switch"},
            "parameters": "{}",
            "is_required": true
        },
        {
            "name": "turn_off",
            "display_name": {"zh": "关闭开关", "en": "Turn Off"},
            "description": {"zh": "关闭智能开关", "en": "Turn off the smart switch"},
            "parameters": "{}",
            "is_required": true
        },
        {
            "name": "toggle",
            "display_name": {"zh": "切换状态", "en": "Toggle"},
            "description": {"zh": "切换开关状态", "en": "Toggle switch state"},
            "parameters": "{}",
            "is_required": false
        },
        {
            "name": "get_power_stats",
            "display_name": {"zh": "获取电力统计", "en": "Get Power Statistics"},
            "description": {"zh": "获取详细的电力使用统计", "en": "Get detailed power usage statistics"},
            "parameters": "{}",
            "is_required": false
        },
        {
            "name": "reset_energy_counter",
            "display_name": {"zh": "重置电量计数", "en": "Reset Energy Counter"},
            "description": {"zh": "重置累计电量计数器", "en": "Reset cumulative energy counter"},
            "parameters": "{}",
            "is_required": false
        }
    ]',
    1,
    1,
    datetime('now'),
    datetime('now')
);

-- ============================================================================
-- 创建完成提示
-- ============================================================================

-- 设备模板系统表创建完成
-- 包含以下表：
-- - template_categories: 模板分类表
-- - device_templates: 设备模板表
-- 包含以下内置模板：
-- - 温度传感器模板
-- - ONVIF摄像头模板  
-- - Modbus RTU设备模板
-- - MQTT设备模板
-- - 智能开关模板