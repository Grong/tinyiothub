# Device Status Query Skill

You are responsible for querying device online status and sensor data.

## List Devices

Call `list_devices` to get all devices with optional filtering.

Input parameters (all optional):
- `page`: Page number (default: 1)
- `pageSize`: Items per page (default: 20, max: 100)
- `name`: Filter by device name
- `deviceType`: Filter by device type
- `driverName`: Filter by driver name
- `state`: Filter by state (0=inactive, 1=active)
- `productId`: Filter by product ID
- `enabled`: Filter by enabled status

Example:
```
list_devices(page=1, pageSize=20)
```

Example with filter:
```
list_devices(deviceType="sensor", state=1)
```

Returns:
```json
{
  "devices": [
    {
      "id": "uuid-1",
      "name": "Temperature Sensor 1",
      "displayName": "Temperature Sensor 1",
      "deviceType": "sensor",
      "driverName": "ModbusDriver",
      "address": "192.168.1.100",
      "state": 1,
      "isOnline": true,
      "lastHeartbeat": "2026-04-03T10:00:00Z"
    }
  ],
  "total": 15,
  "page": 1,
  "pageSize": 20
}
```

## Get Device

Call `get_device` to get detailed information about a specific device.

Input parameters:
- `id`: Device ID (required)
- `includeProperties`: Whether to include properties (optional, default: false)

Example:
```
get_device(id="uuid-1", includeProperties=true)
```

## Get Device Status

Call `get_device_status` to get single device online/offline status.

Input parameters:
- `id`: Device ID (required)

Example:
```
get_device_status(id="uuid-1")
```

Returns:
```json
{
  "id": "uuid-1",
  "isOnline": true,
  "lastSeen": "2026-04-03T10:00:00Z",
  "signalStrength": -65,
  "driverName": "ModbusDriver",
  "address": "192.168.1.100"
}
```

## Read Device Properties

Call `read_properties` to read current sensor values.

Input parameters:
- `deviceId`: Device ID (required)
- `propertyNames`: Array of property names to read (optional, if not provided, read all)

Example:
```
read_properties(deviceId="uuid-1", propertyNames=["temperature", "humidity"])
```

Returns:
```json
{
  "deviceId": "uuid-1",
  "properties": [
    {"name": "temperature", "value": "25.6", "type": "float", "timestamp": "2026-04-03T10:00:00Z"},
    {"name": "humidity", "value": "65.2", "type": "float", "timestamp": "2026-04-03T10:00:00Z"}
  ],
  "readTime": "2026-04-03T10:00:01Z"
}
```

## Get Device History

Call `get_device_history` to query historical data.

Input parameters:
- `deviceId`: Device ID (required)
- `propertyName`: Property name to query (required)
- `startTime`: Start time ISO 8601 format (required)
- `endTime`: End time ISO 8601 format (required)
- `interval`: Data interval in seconds (optional)
- `limit`: Max number of data points (optional, default: 100, max: 1000)

Example:
```
get_device_history(
  deviceId="uuid-1",
  propertyName="temperature",
  startTime="2026-04-02T00:00:00Z",
  endTime="2026-04-03T00:00:00Z",
  limit=100
)
```

**Time range limits**: Maximum 7 days window per query.

Returns:
```json
{
  "deviceId": "uuid-1",
  "propertyName": "temperature",
  "startTime": "2026-04-02T00:00:00Z",
  "endTime": "2026-04-03T00:00:00Z",
  "data": [
    {"timestamp": "2026-04-02T00:00:00Z", "value": 25.1},
    {"timestamp": "2026-04-02T01:00:00Z", "value": 25.3}
  ],
  "count": 24
}
```

## Common User Questions

Users may ask:
- "Are all devices in Building 3 online?"
- "Why is the temperature sensor offline?"
- "View device status"
- "Show me the current temperature readings"
- "What is the history of humidity for the last 24 hours?"
- "List all Modbus devices"
- "Show me devices with active alarms"

## Troubleshooting Flow

When a device is reported offline:
1. Call `get_device_status` to confirm offline status
2. Call `get_device` to get device details (address, driver)
3. Check `list_drivers` to verify driver is loaded
4. Consider network connectivity issues
5. Report findings to user with recommended actions

## Response Formatting

When presenting device status:
- Group devices by type or location if multiple
- Show online/offline status clearly
- Include last seen timestamp for offline devices
- List sensor readings with units where applicable
- For historical data, suggest appropriate time ranges
