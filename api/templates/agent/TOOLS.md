# TOOLS.md — What I Can Do

These are my capabilities — use them naturally as you work.

## Device Management

**Onboarding new devices**
- Scan available serial ports
- Match device brand/model to supported drivers (Modbus, ONVIF, SNMP, MQTT)
- Configure connection parameters and test communication
- Register device and report its online status

**Reading & Writing**
- Read current sensor values and properties
- Write control values to actuators
- Batch read multiple devices for comparison
- Subscribe to MQTT topics for real-time updates

## Alarm & Self-Healing

**Alarm Management**
- View active alarms filtered by device, level, or time
- Acknowledge and close alarms
- Create alarm rules with threshold/range/change/duration conditions
- View alarm statistics and trends

**Self-Healing**
- Check self-heal policy (L0-L3 levels)
- Trigger manual recovery actions (restart driver, reconnect device)
- View recovery action history

## Diagnostics

**Device Health**
- Run fault diagnosis on a single device
- Compare property values across multiple devices
- Scan serial ports for available connections
- View device trace logs (errors, warnings, info)

**System Health**
- Check gateway system status (CPU, memory, disk)
- View driver loading states
- Analyze event logs

## Data & History

**Telemetry**
- Query historical data for any property
- Get min/max/average over time ranges (max 7 days per query)
- Identify outliers and anomalies

## UI Rendering (A2UI)

Use the `canvas` tool to push rich UI components:

```
canvas(toolCallId, {
  action: "a2ui_push",
  jsonl: JSON.stringify({createSurface:{id:"s1",surfaceKind:"inline"}})+"\n"+
         JSON.stringify({updateComponents:{components:[...]}})
})
```

**When to use A2UI:**
- Device lists → `DeviceTable` or `DeviceCard`
- KPIs/metrics → `StatCard` with trend
- Time-series data → `DataChart`
- Alarms → `AlarmTable` or `AlarmCard`
- Control interfaces → `ControlPanel`

## Memory

Use memory tools to persist information across sessions:
- `memory_store` — remember device preferences, decisions, context
- `memory_recall` — retrieve prior context when needed
- `memory_forget` — remove stale/incorrect memories

---

*This file describes what I can do. See skills/ for specialized workflows.*
