-- ============================================
-- 测试数据种子脚本
-- 基于迁移文件的最终数据库结构
-- ============================================

-- 注意：基础数据已在之前的迁移文件中插入，这里只添加额外的测试数据

-- ============================================
-- 用户数据（密码：admin123）
-- ============================================

-- 添加额外测试用户
INSERT OR IGNORE INTO users (id, username, password_hash, email, display_name, is_enabled, created_at) VALUES
('user-test-001', 'test1', 'hashed_admin123', 'test1@example.com', '测试用户1', 1, datetime('now')),
('user-test-002', 'test2', 'hashed_admin123', 'test2@example.com', '测试用户2', 1, datetime('now')),
('user-operator-001', 'operator', 'hashed_admin123', 'operator@example.com', '操作员', 1, datetime('now'));

-- 分配角色
INSERT OR IGNORE INTO user_roles (id, user_id, role_id, created_at) VALUES
('user-role-test-001', 'user-test-001', 'role-viewer', datetime('now')),
('user-role-test-002', 'user-test-002', 'role-viewer', datetime('now')),
('user-role-operator-001', 'user-operator-001', 'role-operator', datetime('now'));

-- ============================================
-- 组织数据
-- ============================================

INSERT OR IGNORE INTO organizations (id, name, description, created_at) VALUES
('org-default', '默认组织', '系统默认组织', datetime('now')),
('org-factory-001', '生产工厂A', '第一生产工厂', datetime('now')),
('org-factory-002', '生产工厂B', '第二生产工厂', datetime('now'));

-- ============================================
-- 事件数据
-- ============================================

-- 插入历史事件
INSERT OR IGNORE INTO events (id, event_type, event_subtype, event_level, timestamp, source_type, source_id, title, content, user_id, device_id, created_at) VALUES
('event-001', 'device', '{"Device":"Connection"}', 2, '2026-01-26T09:00:00Z', 'device', 'device-001', '设备上线', '{"title":"设备上线","elements":[{"Text":{"content":"温度传感器重新连接","format":"Plain"}}],"metadata":{}}', NULL, 'device-001', '2026-01-26T09:00:00Z'),
('event-002', 'device', '{"Device":"PropertyChange"}', 2, '2026-01-26T10:00:00Z', 'device', 'device-001', '属性变化', '{"title":"属性变化","elements":[{"Text":{"content":"属性值发生变化","format":"Plain"}}],"metadata":{"property":"temperature","old_value":"24.5","new_value":"25.8"}}', NULL, 'device-001', '2026-01-26T10:00:00Z'),
('event-003', 'system', '{"System":"UserAuth"}', 2, '2026-01-26T11:00:00Z', 'user', 'admin-user-001', '用户登录', '{"title":"用户登录","elements":[{"Text":{"content":"用户成功登录系统","format":"Plain"}}],"metadata":{"username":"admin","ip":"192.168.1.100"}}', 'admin-user-001', NULL, '2026-01-26T11:00:00Z'),
('event-004', 'device', '{"Device":"CommandCompleted"}', 2, '2026-01-26T12:00:00Z', 'device', 'device-003', '执行命令', '{"title":"命令执行成功","elements":[{"Text":{"content":"命令执行完成","format":"Plain"}}],"metadata":{"command":"capture","result":"success"}}', 'admin-user-001', 'device-003', '2026-01-26T12:00:00Z'),
('event-005', 'device', '{"Device":"Connection"}', 4, '2026-01-26T13:00:00Z', 'device', 'device-002', '通信错误', '{"title":"通信错误","elements":[{"Text":{"content":"设备通信超时","format":"Plain"}}],"metadata":{"error":"timeout","retry_count":"3"}}', NULL, 'device-002', '2026-01-26T13:00:00Z');

-- 插入实时事件
INSERT OR IGNORE INTO real_time_events (id, event_type, event_subtype, event_level, source_type, source_id, device_id, title, content, first_occurrence, last_update, occurrence_count, acknowledged) VALUES
('rt-event-001', 'device', '{"Device":"Connection"}', 3, 'device', 'device-003', 'device-003', '设备离线', '{"title":"设备离线","elements":[{"Text":{"content":"摄像头连接中断","format":"Plain"}}],"metadata":{}}', '2026-01-26T12:00:00Z', '2026-01-26T12:00:00Z', 1, 0),
('rt-event-002', 'device', '{"Device":"Connection"}', 3, 'device', 'device-005', 'device-005', '设备离线', '{"title":"设备离线","elements":[{"Text":{"content":"摄像头连接中断","format":"Plain"}}],"metadata":{}}', '2026-01-26T11:00:00Z', '2026-01-26T11:00:00Z', 1, 0),
('rt-event-003', 'device', '{"Device":"DeviceAlarm"}', 3, 'device', 'device-001', 'device-001', '温度告警', '{"title":"温度告警","elements":[{"Text":{"content":"温度超过阈值","format":"Plain"}}],"metadata":{"temperature":"85","threshold":"80"}}', '2026-01-26T12:30:00Z', '2026-01-26T12:30:00Z', 1, 0);

-- ============================================
-- 设备追踪记录
-- ============================================

INSERT OR IGNORE INTO device_traces (id, device_id, trace_type, level, category, title, message, details, source, created_at) VALUES
('trace-006', 'device-001', 'operation', 'info', 'user', '设置采样频率', '用户调整了采样间隔', '{"old_interval": 60, "new_interval": 30}', 'api', datetime('now', '-3 hours')),
('trace-007', 'device-003', 'operation', 'info', 'user', '拍照命令', '用户执行了拍照命令', '{"command": "capture", "result": "success"}', 'api', datetime('now', '-2 hours')),
('trace-008', 'device-004', 'status_change', 'info', 'system', '程序启动', '机器人开始执行工作程序', '{"program_id": 1, "cycle_count": 0}', 'system', datetime('now', '-1 hour')),
('trace-009', 'device-002', 'error', 'error', 'device', '传感器故障', '湿度传感器读数异常', '{"error_code": "E001", "description": "sensor malfunction"}', 'device', datetime('now', '-45 minutes')),
('trace-010', 'device-005', 'warning', 'warn', 'network', '网络延迟', '摄像头网络响应时间过长', '{"latency_ms": 1500, "threshold_ms": 1000}', 'system', datetime('now', '-20 minutes'));

-- ============================================
-- 审计日志
-- ============================================

INSERT OR IGNORE INTO event_audit_logs (id, log_type, user_id, event_id, event_type, event_level, action, result, ip_address, created_at) VALUES
('audit-001', 'access', 'admin-user-001', 'event-001', 'device', 2, 'view_event', 'allowed', '192.168.1.100', datetime('now', '-2 hours')),
('audit-002', 'modification', 'admin-user-001', 'rt-event-001', 'device', 3, 'acknowledge_event', 'success', '192.168.1.100', datetime('now', '-1 hour')),
('audit-003', 'access', 'user-operator-001', 'event-002', 'device', 2, 'view_event', 'allowed', '192.168.1.101', datetime('now', '-30 minutes'));

-- ============================================
-- 通知历史
-- ============================================

INSERT OR IGNORE INTO notification_history (id, event_id, rule_id, notification_method, recipient, status, sent_at, created_at) VALUES
('notif-001', 'event-005', 'default-error-events', 'websocket', 'admin', 'sent', datetime('now', '-10 minutes'), datetime('now', '-10 minutes')),
('notif-002', 'event-004', 'device-connection-events', 'websocket', 'admin', 'sent', datetime('now', '-15 minutes'), datetime('now', '-15 minutes')),
('notif-003', 'event-004', 'device-connection-events', 'websocket', 'operator', 'sent', datetime('now', '-15 minutes'), datetime('now', '-15 minutes'));

-- ============================================
-- 更新统计信息
-- ============================================

ANALYZE;

-- ============================================
-- 测试数据插入完成
-- 
-- 默认用户账号：
-- - admin / admin123 (管理员)
-- - operator / admin123 (操作员)
-- - test1 / admin123 (查看者)
-- - test2 / admin123 (查看者)
-- 
-- 已创建：
-- - 5个设备（包含完整属性和命令）
-- - 3个产品
-- - 3个组织
-- - 多个事件和追踪记录
-- - 告警规则和实例
-- - 标签和绑定
-- ============================================
