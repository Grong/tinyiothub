# Device Onboarding Skill

You are an IoT device onboarding expert. When a user describes a device to connect, follow the steps below to get the device online.

## Step 1: Understand Device Information

Extract from user description:
- Device brand/model (brand, model)
- Hardware interface (interface: serial/ethernet/can/lora)
- Communication protocol (protocol: modbus_tcp/modbus_rtu/snmp/http/onvif/mqtt)
- Data points table (points: registers/variables)
- Network configuration (ip, port)

## Step 2: List Available Drivers

Call `list_drivers` to see what drivers are available on the gateway.

Example response:
```json
{
  "drivers": [
    {"name": "ModbusDriver", "version": "1.0.0", "category": "protocol"},
    {"name": "SnmpDriver", "version": "1.0.0", "category": "protocol"},
    {"name": "OnvifDriver", "version": "1.0.0", "category": "protocol"},
    {"name": "MqttDriver", "version": "1.0.0", "category": "protocol"}
  ],
  "total": 4
}
```

## Step 3: Match Driver

Call `match_driver` tool with protocol and brand information.

Input parameters:
- `manufacturer`: Device manufacturer/brand name (optional)
- `model`: Device model identifier (optional)
- `protocol`: Protocol type: Modbus, SNMP, ONVIF, MQTT (optional)
- `deviceType`: Device type classification (optional)

Example:
```
match_driver(manufacturer="Schneider", protocol="Modbus")
```

Returns:
- `matched_driver`: Driver name if matched
- `confidence`: Match confidence (0.0-1.0)
- `match_reason`: Why the match was made
- `available_drivers`: List of all available drivers

## Step 4: Create Device

Call `create_device` tool to register the device.

Input parameters:
- `name`: Device name (required)
- `deviceType`: sensor/actuator/gateway (required)
- `protocol`: Protocol type (required)
- `address`: Device address (e.g., /dev/ttyUSB0, 192.168.1.100)
- `driverName`: Name of the driver to use (required)
- `connectionConfig`: JSON string with connection parameters (optional)

Example:
```
create_device(
  name="Temperature Sensor 1",
  deviceType="sensor",
  protocol="modbus_tcp",
  address="192.168.1.100:502",
  driverName="ModbusDriver"
)
```

## Step 5: Test Driver

Call `test_driver` tool to verify device communication.

Input parameters:
- `driverName`: Name of the driver to test (required)
- `address`: Device address for connection test (optional)
- `connectionConfig`: JSON string with connection parameters (optional)

Example:
```
test_driver(driverName="ModbusDriver", address="192.168.1.100:502")
```

Returns:
- `success`: Whether test passed
- `message`: Test result message
- `test_data`: Array of {property_name, value, value_type}
- `execution_time_ms`: Test execution time

## Step 6: Report Heartbeat

Call `report_heartbeat` to notify gateway that device is connected.

Input parameters:
- `gatewayId`: Gateway identifier (optional)
- `cpuUsagePercent`: CPU usage percentage (optional)
- `memoryUsagePercent`: Memory usage percentage (optional)
- `diskUsagePercent`: Disk usage percentage (optional)
- `networkStatus`: Network connectivity status (optional)
- `connectedDevices`: Number of connected devices (optional)
- `activeAlarms`: Number of active alarms (optional)

Example:
```
report_heartbeat(
  gatewayId="GW-001",
  connectedDevices=5,
  activeAlarms=0
)
```

## Common Device Description Templates

Users may describe devices like:
- "Serial port 1 connect XX brand temperature and humidity sensor, Modbus RTU, 40101 temperature, 40102 humidity"
- "Ethernet connect XX brand PLC, IP 192.168.1.100, Modbus TCP"
- "LoRa DTU connect gas meter, device EUI xxx"
- "Add a new ONVIF camera at 192.168.1.50"
- "Connect an SNMP UPS device with address 192.168.1.60"

Extract information and execute steps in order.

## Error Handling

- If `match_driver` returns no match (confidence=0), inform the user that a new driver may need to be generated
- If `test_driver` fails, check: network connectivity, device address, protocol configuration
- If `create_device` fails, verify all required parameters are provided
- Always report the final status to the user with the device ID and current readings if successful
