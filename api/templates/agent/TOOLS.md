# TOOLS.md — What I Can Do

These are my capabilities — use them naturally as you work.

## Device Management (5 Tools)

**device_list**
- Paginated listing of all registered IoT devices
- Filter by name, device type, driver, state
- Shows online/offline status and last heartbeat

**device_template_list**
- List all available device templates
- Filter by category, manufacturer, device type
- Returns template_id needed for `device_create`

**device_profile**
- Full device details including all property definitions
- Current real-time values for each property
- Online/offline status and metadata

**device_property_get**
- Lookup a single property definition on a device
- Shows: name, display name, data type, unit, min/max, read-only flag
- Includes current value if available

**device_create**
- Create a new device from a device template
- Requires: `template_id` (from device_template_list) and `name`
- Optionally set property values and enable specific commands at creation

**device_command**
- Send a control command to a device
- Requires: `device_id` and `command_name`
- Returns execution result (success/failure with message)

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
- View device trace logs (errors, warnings, info)

**System Health**
- Check gateway system status (CPU, memory, disk)
- View driver loading states
- Analyze event logs

## UI Rendering (A2UI)

Use the `canvas` tool to push rich UI components after getting data from tools.

### Canvas Tool Call

```
canvas(toolCallId, {
  action: "a2ui_push",
  jsonl: JSON.stringify({createSurface:{id:"s1",surfaceKind:"inline"}})+"\n"+
         JSON.stringify({updateComponents:{components:[{id:"card1",componentKind:"DeviceCard",dataModel:{}}]}})
})
```

### Device Card Example

After `device_profile`, map the result to a `DeviceCard`:

```
const deviceData = await tool_results_of_device_profile;
const state = deviceData.state === 1 ? "online" : deviceData.state === 2 ? "warning" : "offline";
const surfaceId = "device-" + Date.now();
canvas(tool_call_id, {
  action: "a2ui_push",
  jsonl:
    JSON.stringify({createSurface:{id:surfaceId,surfaceKind:"inline"}})+"\n"+
    JSON.stringify({updateComponents:{components:[{
      id:"card1",
      componentKind:"DeviceCard",
      dataModel:{
        deviceId: deviceData.id,
        name: deviceData.display_name || deviceData.name,
        status: state,
        properties: deviceData.properties?.map(p => ({
          key: p.display_name || p.name,
          value: p.current_value || p.default_value || "-",
          unit: p.unit || ""
        })) || []
      }
    }]}})
});
```

### Component Reference

| Component | Use For |
|-----------|---------|
| `DeviceCard` | Single device overview with status and properties |
| `DeviceTable` | Multiple devices with columns |
| `StatCard` | KPIs and single metrics |
| `DataChart` | Time-series data visualization |
| `AlarmCard` | Single alarm details |
| `AlarmTable` | Alarm list with filters |
| `ControlPanel` | Device control interface |
| `Text`, `Column`, `Row`, `Divider` | Layout primitives |

## Memory

Use memory tools to persist information across sessions:
- `memory_store` — remember device preferences, decisions, context
- `memory_recall` — retrieve prior context when needed
- `memory_forget` — remove stale/incorrect memories

---

*This file describes what I can do. See skills/ for specialized workflows.*
