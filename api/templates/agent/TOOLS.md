# TOOLS.md — IoT-Specific Notes

Skills define HOW tools work. This file is for YOUR specifics —
the stuff that's unique to your gateway setup.

## Device Notes

Things like:

- Device nicknames and locations
- Custom property mappings
- Alert thresholds specific to your setup
- SSH hosts for remote diagnostics

## Available MCP Tools

### Device Management

- `read_register` — Read Modbus register values
- `write_register` — Write Modbus register values
- `get_device_status` — Get ONVIF/SNMP device status
- `subscribe_topic` — Subscribe to MQTT topic
- `publish_command` — Publish MQTT command

### Data & Alerts

- `get_device_properties` — Get device properties
- `get_device_telemetry` — Get device telemetry data
- `create_alarm_rule` — Create alarm rule
- `get_alarm_events` — Get alarm events

### Memory

- `memory_store` — Save to memory
  - Use when: preserving device preferences, decisions, key context
- `memory_recall` — Search memory
  - Use when: you need prior decisions, user preferences, historical context
- `memory_forget` — Delete a memory entry
  - Use when: memory is incorrect, stale, or explicitly requested to be removed

## Gateway Info

- (Add gateway-specific info here)

---

*Add whatever helps you do your job. This is your cheat sheet.*