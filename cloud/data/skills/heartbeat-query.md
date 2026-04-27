# Heartbeat Query Skill

You are responsible for querying and configuring gateway heartbeat monitoring.

## Get Heartbeat Status

Call `get_heartbeat_status` to get current gateway health status.

This tool requires no input parameters.

Returns:
- `gatewayId`: Gateway identifier
- `status`: Overall health status (healthy/warning/degraded)
- `timestamp`: Last heartbeat timestamp
- `cpuUsagePercent`: CPU usage percentage (0-100)
- `memoryUsagePercent`: Memory usage percentage (0-100)
- `diskUsagePercent`: Disk usage percentage (0-100)
- `networkStatus`: Network connectivity status
- `connectedDevices`: Number of connected devices
- `activeAlarms`: Number of active alarms
- `uptimeSeconds`: Gateway uptime in seconds
- `lastCloudSync`: Last cloud sync timestamp

Example:
```
get_heartbeat_status()
```

Example response:
```json
{
  "gatewayId": "GW-001",
  "status": "healthy",
  "timestamp": "2026-04-03T10:00:00Z",
  "cpuUsagePercent": 45.2,
  "memoryUsagePercent": 60.5,
  "diskUsagePercent": 30.1,
  "networkStatus": "connected",
  "connectedDevices": 12,
  "activeAlarms": 0,
  "uptimeSeconds": 86400,
  "lastCloudSync": "2026-04-03T09:59:00Z"
}
```

## Report Heartbeat

Call `report_heartbeat` to push gateway health status to cloud.

Input parameters (all optional):
- `gatewayId`: Gateway identifier
- `cpuUsagePercent`: CPU usage percentage (0-100)
- `memoryUsagePercent`: Memory usage percentage (0-100)
- `diskUsagePercent`: Disk usage percentage (0-100)
- `networkStatus`: Network connectivity status (connected/degraded/offline)
- `connectedDevices`: Number of connected devices
- `activeAlarms`: Number of active alarms
- `metadata`: Additional metadata as JSON object

Example:
```
report_heartbeat(
  gatewayId="GW-001",
  cpuUsagePercent=45.2,
  memoryUsagePercent=60.5,
  diskUsagePercent=30.1,
  networkStatus="connected",
  connectedDevices=12,
  activeAlarms=0
)
```

## Configure Heartbeat

Call `configure_heartbeat` to modify probe configuration.

Input parameters (all optional):
- `probeIntervalSecs`: Probe interval in seconds
- `cpuThresholdPercent`: CPU threshold percentage (0-100)
- `memoryThresholdPercent`: Memory threshold percentage (0-100)
- `diskThresholdPercent`: Disk threshold percentage (0-100)
- `cloudSyncEnabled`: Whether cloud sync is enabled (boolean)
- `cloudSyncIntervalSecs`: Cloud sync interval in seconds

Example:
```
configure_heartbeat(probeIntervalSecs=60, cpuThresholdPercent=80)
```

## Health Status Thresholds

- **CPU**: warning > 70%, critical > 90%
- **Memory**: warning > 75%, critical > 90%
- **Disk**: warning > 80%, critical > 95%
- **Network**: warning > 5s latency, critical > 10s
- **Status values**: healthy, warning, degraded

## Common User Questions

Users may ask:
- "Is the gateway heartbeat normal?"
- "View heartbeat status"
- "How is the system health?"
- "Change heartbeat interval to 10 minutes"
- "Disable system probe"
- "What are the current thresholds?"

## Response Formatting

When responding to heartbeat queries, present the information clearly:
- Show key metrics (CPU, memory, disk, network)
- Indicate overall health status with color coding if applicable
- List any warnings or issues
- Report uptime in human-readable format (e.g., "2 days, 5 hours")
