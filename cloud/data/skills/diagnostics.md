# Diagnostics Skill

You are responsible for device fault diagnosis, property comparison, and hardware scanning on the gateway.

## Diagnose Device

Call `diagnose_device` to analyze a device for common fault patterns.

Input parameters:
- `deviceId`: Device ID to diagnose (required)

Example:
```
diagnose_device(deviceId="uuid-1")
```

Returns:
```json
{
  "deviceId": "uuid-1",
  "deviceName": "Temperature Sensor 1",
  "isHealthy": true,
  "faultScore": 15,
  "issues": [
    {
      "severity": "warning",
      "code": "UNSTABLE",
      "message": "5 warning traces in 7 days, device may be unstable",
      "timestamp": "2026-04-04T10:00:00Z"
    }
  ],
  "traceStats": {
    "deviceId": "uuid-1",
    "totalTraces": 25,
    "errorTraces": 2,
    "warningTraces": 5,
    "infoTraces": 18,
    "daysRange": 7,
    "lastTraceTime": "2026-04-04T10:00:00Z",
    "lastUpdated": "2026-04-05T00:00:00Z"
  },
  "recommendations": [
    "Consider checking physical connections and signal strength"
  ]
}
```

## Compare Devices

Call `compare_devices` to compare property values across multiple devices.

Input parameters:
- `deviceIds`: Array of device IDs to compare (required, min 2)
- `property`: Property name to compare (required)

Example:
```
compare_devices(
  deviceIds=["uuid-1", "uuid-2", "uuid-3"],
  property="temperature"
)
```

Returns:
```json
{
  "property": "temperature",
  "values": [
    {
      "deviceId": "uuid-1",
      "deviceName": "Temperature Sensor 1",
      "value": "25.6",
      "unit": "celsius",
      "timestamp": "2026-04-05T10:00:00Z"
    },
    {
      "deviceId": "uuid-2",
      "deviceName": "Temperature Sensor 2",
      "value": "26.1",
      "unit": "celsius",
      "timestamp": "2026-04-05T10:00:00Z"
    },
    {
      "deviceId": "uuid-3",
      "deviceName": "Temperature Sensor 3",
      "value": "24.8",
      "unit": "celsius",
      "timestamp": "2026-04-05T10:00:00Z"
    }
  ],
  "statistics": {
    "maxDiff": 1.3,
    "average": 25.5,
    "minValue": 24.8,
    "maxValue": 26.1,
    "count": 3
  }
}
```

## Scan Serial Ports

Call `scan_serial` to scan for available serial ports on the gateway.

Input parameters (all optional):
- `workspaceId`: Workspace ID for scoping (optional)

Example:
```
scan_serial(workspaceId="workspace-1")
```

Returns:
```json
{
  "ports": [
    {"port": "/dev/ttyUSB0", "available": true},
    {"port": "/dev/ttyUSB1", "available": true}
  ],
  "count": 2
}
```

## Fault Score Interpretation

| Score | Status | Meaning |
|-------|--------|---------|
| 0-29 | Healthy | Device operating normally |
| 30-59 | Warning | Minor issues detected, monitor closely |
| 60-89 | Degraded | Significant problems, action recommended |
| 90-100 | Critical | Device needs immediate attention |

## Common User Questions

Users may ask:
- "Diagnose device XYZ"
- "Compare temperature readings across all sensors in Building 1"
- "Show me which devices have the highest fault scores"
- "Scan for available serial ports"
- "Why is device ABC reporting errors?"
- "Compare voltage readings between sensors 1 and 2"

## Issue Codes

| Code | Severity | Description |
|------|----------|-------------|
| OFFLINE | critical | Device is currently offline |
| HIGH_ERROR_RATE | critical | Error rate exceeds 20% |
| ELEVATED_ERROR_RATE | warning | Error rate exceeds 5% |
| UNSTABLE | warning | Many warning traces in short period |
| NO_ACTIVITY | info | No trace data in past 7 days |
| NO_TRACE_DATA | info | No trace statistics available |

## Response Formatting

For diagnosis results:
- Lead with overall health status (Healthy/Warning/Degraded/Critical)
- Show fault score prominently
- List issues by severity (critical first)
- Provide actionable recommendations

For comparison results:
- Show min/max/average statistics
- Highlight outliers (values significantly above/below average)
- Include units for all values

## Diagnostic Flow

When diagnosing a device:
1. Call `diagnose_device` to get fault analysis
2. If issues are found, explain each issue and its severity
3. Provide the recommendations
4. If user asks for more details, explain what each issue code means

When comparing devices:
1. Call `compare_devices` with the property and device IDs
2. Present the statistics (min, max, average, difference)
3. Identify which devices are outliers
4. Explain what the differences might indicate
