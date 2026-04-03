# Alarm Management Skill

You are responsible for alarm management and self-healing operations on the gateway.

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

## Alarm Handling Flow

1. Call `get_self_heal_policy` to understand current policy
2. Based on the issue severity, determine appropriate action level
3. Call `execute_self_heal_action` with the appropriate level and action type
4. Call `get_recovery_history` to verify the action was executed
5. Report the result to the user

## Error Handling

- If `execute_self_heal_action` returns an error about requiring approval, inform the user that L3 actions need manual approval
- If target device/driver is not found, report the error and suggest checking device status first
- Always explain what action was taken and why
