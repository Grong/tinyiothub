# Alarm Management Skill

You are responsible for alarm management, self-healing operations, and alarm rules on the gateway.

## List Alarms

Call `alarm_list` to get active alarms with optional filtering.

Input parameters (all optional):
- `page`: Page number (default: 1)
- `pageSize`: Items per page (default: 20, max: 100)
- `deviceId`: Filter by device ID
- `level`: Filter by severity level (info, warning, error, critical)
- `acknowledged`: Filter by acknowledged status (boolean)
- `ruleType`: Filter by rule type (threshold, range, change, duration, composite)

Example:
```
alarm_list(page=1, pageSize=20, level="error")
```

Returns:
```json
{
  "alarms": [
    {
      "id": "uuid-1",
      "deviceId": "uuid-device",
      "deviceName": "Temperature Sensor 1",
      "ruleId": "uuid-rule",
      "ruleName": "High Temperature Alert",
      "ruleType": "threshold",
      "level": "error",
      "message": "Temperature exceeded 80 celsius",
      "acknowledged": false,
      "acknowledgedBy": null,
      "acknowledgedAt": null,
      "triggeredAt": "2026-04-05T10:00:00Z"
    }
  ],
  "total": 15,
  "page": 1,
  "pageSize": 20
}
```

## Get Alarm Statistics

Call `alarm_statistics` to get alarm statistics overview.

Input parameters:
- `startTime`: Start time ISO 8601 format (required)
- `endTime`: End time ISO 8601 format (required)
- `workspaceId`: Workspace ID to scope statistics (optional)

Example:
```
alarm_statistics(startTime="2026-04-01T00:00:00Z", endTime="2026-04-05T00:00:00Z")
```

Returns:
```json
{
  "totalAlarms": 45,
  "acknowledgedCount": 30,
  "unacknowledgedCount": 15,
  "byLevel": {
    "critical": 5,
    "error": 15,
    "warning": 20,
    "info": 5
  },
  "byRuleType": {
    "threshold": 20,
    "range": 10,
    "change": 8,
    "duration": 5,
    "composite": 2
  },
  "topDevices": [
    {"deviceId": "uuid-1", "count": 12},
    {"deviceId": "uuid-2", "count": 8}
  ],
  "startTime": "2026-04-01T00:00:00Z",
  "endTime": "2026-04-05T00:00:00Z"
}
```

## Acknowledge Alarm

Call `alarm_acknowledge` to acknowledge an active alarm.

Input parameters:
- `alarmId`: Alarm ID to acknowledge (required)
- `comment`: Optional comment explaining why (optional)

Example:
```
alarm_acknowledge(alarmId="uuid-1", comment="Being investigated")
```

Returns:
```json
{
  "acknowledged": true,
  "alarmId": "uuid-1",
  "acknowledgedBy": "user@example.com",
  "acknowledgedAt": "2026-04-05T10:30:00Z"
}
```

## Add Alarm Rule

Call `alarm_rule_add` to create a new alarm rule.

Input parameters:
- `name`: Rule name (required)
- `ruleType`: Rule type: threshold, range, change, duration, composite (required)
- `deviceId`: Target device ID (required)
- `property`: Property name to monitor (required)
- `condition`: Rule condition in JSON format (required)
- `level`: Severity level: info, warning, error, critical (required)
- `enabled`: Whether the rule is enabled (optional, default: true)
- `workspaceId`: Workspace ID to scope this rule (optional)

Condition formats by rule type:

**Threshold** (value crosses a boundary):
```json
{"operator": "gt", "value": 80}
```
Operators: gt, lt, gte, lte, eq, neq

**Range** (value outside or inside range):
```json
{"operator": "outside", "min": 20, "max": 80}
```
Operators: outside, inside

**Change** (value changes by amount):
```json
{"changeType": "delta", "threshold": 10}
```
Change types: delta, rate

**Duration** (condition persists for duration):
```json
{"condition": {"operator": "gt", "value": 80}, "durationSecs": 300}
```

**Composite** (multiple conditions):
```json
{"conditions": [{"property": "temperature", "operator": "gt", "value": 80}], "logic": "and"}

Example:
```
alarm_rule_add(
  name="High Temperature Alert",
  ruleType="threshold",
  deviceId="uuid-1",
  property="temperature",
  condition={"operator": "gt", "value": 80},
  level="error",
  enabled=true
)
```

Returns:
```json
{
  "id": "uuid-new",
  "name": "High Temperature Alert",
  "ruleType": "threshold",
  "deviceId": "uuid-1",
  "property": "temperature",
  "level": "error",
  "enabled": true,
  "createdAt": "2026-04-05T10:00:00Z"
}
```

## Get Self-Heal Policy

Call `get_self_heal_policy` to retrieve the current self-healing policy configuration.

This tool requires no input parameters.

Returns:
```json
{
  "enabled": true,
  "levels": {
    "L0": {
      "actions": ["log_only"],
      "conditions": [
        {"type": "signal_weak", "threshold": -110},
        {"type": "single_timeout", "count": 1}
      ],
      "cooldownSecs": 300,
      "requireApproval": false
    },
    "L1": {
      "actions": ["restart_driver", "rejoin_lora", "reconnect_device"],
      "conditions": [
        {"type": "process_dead"},
        {"type": "device_timeout", "count": 3}
      ],
      "cooldownSecs": 600,
      "requireApproval": false
    },
    "L2": {
      "actions": ["report_cloud", "clean_logs"],
      "conditions": [
        {"type": "devices_offline_ratio", "threshold": 0.2},
        {"type": "disk_usage", "threshold": 85}
      ],
      "cooldownSecs": 1800,
      "requireApproval": false
    },
    "L3": {
      "actions": ["report_cloud", "create_ticket"],
      "conditions": [
        {"type": "bus_short_circuit"},
        {"type": "core_service_crash"}
      ],
      "cooldownSecs": 3600,
      "requireApproval": true
    }
  }
}
```

## Execute Self-Heal Action

Call `execute_self_heal_action` to manually trigger a recovery action.

**Note**: Actions requiring approval (L3) cannot be executed directly via MCP.

Input parameters:
- `level`: Severity level: L0, L1, L2, L3 (required)
- `actionType`: Recovery action type (required)
- `target`: Target device or process ID (optional)

Valid action types:
- `log_only`: Log the event only (L0)
- `restart_driver`: Restart a driver process
- `rejoin_lora`: Rejoin LoRa network
- `reconnect_device`: Reconnect to a device
- `clean_logs`: Clean up old log files
- `report_cloud`: Report to cloud platform
- `create_ticket`: Create a ticket for manual intervention

Example:
```
execute_self_heal_action(level="L1", actionType="restart_driver", target="ModbusDriver")
```

Returns:
```json
{
  "executionId": "uuid",
  "executed": true,
  "result": "success",
  "logs": [
    "action started",
    "driver stopped",
    "waiting for cooldown",
    "driver started"
  ]
}
```

## Get Recovery History

Call `get_recovery_history` to view historical recovery events.

Input parameters (all optional):
- `limit`: Number of records to return (default: 20, max: 100)
- `offset`: Pagination offset (default: 0)

Example:
```
get_recovery_history(limit=20, offset=0)
```

Returns:
```json
{
  "executions": [
    {
      "id": "uuid-1",
      "timestamp": "2026-04-03T09:00:00Z",
      "level": "L1",
      "actionType": "restart_driver",
      "target": "ModbusDriver",
      "result": "success",
      "logs": ["action started", "driver stopped", "driver started"]
    }
  ],
  "limit": 20,
  "offset": 0,
  "total": 5
}
```

## Severity Levels

| Level | Description | Actions | Requires Approval |
|-------|-------------|---------|-------------------|
| L0 | Minor issue, signal weak, single timeout | log_only | No |
| L1 | Local self-healing, device timeout, process dead | restart_driver, rejoin_lora, reconnect_device | No |
| L2 | Moderate issue, 20%+ devices offline, disk usage high | report_cloud, clean_logs | No |
| L3 | Critical issue, bus short circuit, core service crash | report_cloud, create_ticket | Yes |

## Common User Questions

Users may ask:
- "Show me the self-healing policy"
- "What recovery actions have been taken?"
- "Restart the Modbus driver"
- "Reconnect the temperature sensor"
- "View recovery history"
- "What's the current alarm status?"
- "Execute L1 recovery for device 3"
- "List all active alarms"
- "Show me alarm statistics for the last 7 days"
- "Acknowledge alarm XYZ"
- "Create a high temperature alarm rule for device ABC"
- "Show me all error-level alarms"
- "Which device has the most alarms?"

## Alarm Handling Flow

1. For alarm queries: call `alarm_list` or `alarm_statistics`
2. For acknowledging: call `alarm_acknowledge` with the alarm ID
3. For creating rules: call `alarm_rule_add` with the rule configuration
4. For self-healing: call `get_self_heal_policy` to understand current policy
5. Based on the issue severity, determine appropriate action level
6. Call `execute_self_heal_action` with the appropriate level and action type
7. Call `get_recovery_history` to verify the action was executed
8. Report the result to the user

## Error Handling

- If `execute_self_heal_action` returns an error about requiring approval, inform the user that L3 actions need manual approval
- If target device/driver is not found, report the error and suggest checking device status first
- Always explain what action was taken and why
