# 告警管理

告警系统实时监控设备状态和数据，当触发预设条件时及时产生告警通知，帮助用户及时发现和处理异常情况。

## 告警类型

| 类型 | 说明 | 触发条件 |
|------|------|----------|
| 阈值告警 | 数据超过或低于设定阈值 | temperature > 30 |
| 离线告警 | 设备断开连接超过设定时间 | 30秒无心跳 |
| 故障告警 | 设备主动上报故障状态 | 设备 error 状态 |
| 状态变化告警 | 设备状态发生特定变化 |开关从 ON→OFF |
| 自定义告警 | 根据自定义规则触发 | 用户定义条件 |

## 告警级别

| 级别 | 标识 | 说明 | 典型场景 |
|------|------|------|----------|
| 紧急 | 🔴 | 需要立即处理 | 设备失控、危险状态 |
| 重要 | 🟠 | 需要尽快处理 | 核心设备离线 |
| 一般 | 🟡 | 需要关注 | 数据轻微异常 |
| 信息 | 🔵 | 提示信息 | 设备重启通知 |

## 告警规则

### 创建告警规则

1. 进入「告警管理」→「告警规则」页面
2. 点击「添加规则」
3. 选择告警类型
4. 配置触发条件
5. 设置通知方式和收告警人
6. 保存规则

### 阈值告警配置

```json
{
  "device_id": "device_001",
  "data_point": "temperature",
  "condition": ">",
  "value": 30,
  "duration": 60
}
```

**条件运算符说明：**

| 运算符 | 说明 | 示例 |
|--------|------|------|
| > | 大于 | temperature > 30 |
| < | 小于 | humidity < 40 |
| >= | 大于等于 | temperature >= 0 |
| <= | 小于等于 | temperature <= 50 |
| == | 等于 | status == 1 |
| != | 不等于 | state != 0 |

### 离线告警配置

```json
{
  "device_id": "device_001",
  "offline_duration": 60,
  "check_interval": 30
}
```

### 多条件组合

支持 AND/OR 逻辑组合多个条件：

```json
{
  "conditions": [
    { "property": "temperature", "operator": ">", "value": 30 },
    { "property": "humidity", "operator": ">", "value": 70 }
  ],
  "logic": "OR"
}
```

## 通知方式

### 渠道配置

在「系统管理」→「通知渠道」中配置通知发送渠道：

| 渠道 | 说明 | 配置项 |
|------|------|--------|
| SMS | 短信通知 | 提供商、签名、模板 |
| Email | 邮件通知 | SMTP 服务器、收件人 |
| Webhook | HTTP 回调 | URL、认证信息 |
| MQTT | MQTT 推送 | 主题、QoS |

### 告警规则关联通知

在告警规则中关联通知渠道：

```json
{
  "name": "高温告警",
  "alarm_rule_id": "rule_001",
  "channel_ids": ["channel_sms_001", "channel_email_001"],
  "conditions": {
    "severity": ["critical", "error", "warning"]
  }
}
```

## 告警处理

### 确认告警

收到告警后，点击「确认」标记已处理：

```http
POST /api/v1/alarms/{id}/acknowledge
Content-Type: application/json

{
  "acknowledged_by": "admin",
  "note": "已现场处理，温度传感器已校准"
}
```

### 批量确认

```http
POST /api/v1/alarms/batch-acknowledge
Content-Type: application/json

{
  "alarmIds": ["alarm_001", "alarm_002", "alarm_003"],
  "acknowledged_by": "admin",
  "note": "批量处理"
}
```

### 解决告警

对于已处理的告警，点击「解决」关闭告警：

```http
POST /api/v1/alarms/{id}/resolve
```

### 告警历史

所有告警记录可查询和导出：

| 字段 | 说明 |
|------|------|
| 告警时间 | 告警触发时间 |
| 告警设备 | 触发告警的设备 |
| 告警内容 | 告警的具体信息 |
| 级别 | 紧急/重要/一般/信息 |
| 状态 | 活跃/已确认/已解决 |
| 处理人 | 确认和处理的人员 |

## 告警统计

### 获取告警统计

```http
GET /api/v1/alarms/statistics
```

**响应：**

```json
{
  "success": true,
  "result": {
    "total_count": 150,
    "active_count": 25,
    "acknowledged_count": 100,
    "resolved_count": 25,
    "by_level": {
      "critical": 5,
      "error": 20,
      "warning": 80,
      "info": 45
    }
  }
}
```

## 自动化告警

可将告警规则与自动化动作结合：

```json
{
  "name": "温度过高自动关设备",
  "trigger_type": "threshold",
  "event_device_id": "temp_001",
  "event_property": "temperature",
  "event_condition": ">",
  "event_value": 35,
  "actions": [
    { "type": "device_command", "device_id": "ac_001", "command": "off" },
    { "type": "notification", "channel_id": "channel_email_001" }
  ]
}
```

## 常见问题

**Q：告警没有收到通知？**
- 检查通知渠道配置是否正确
- 确认告警规则已启用
- 检查告警级别是否符合通知条件

**Q：设备离线没有产生告警？**
- 确认设备已配置离线告警规则
- 检查离线持续时间设置（默认 60 秒）

**Q：如何避免告警风暴？**
- 设置合理的冷却时间（cooldown）
- 使用告警聚合功能将同类告警合并
- 设置告警升级机制
