# TinyIoTHub IoT Catalog for A2UI

> Version: 1.0.0
> Extends: A2UI Basic Catalog v0.10
> Date: 2026-04-03

## Overview

The TinyIoTHub IoT Catalog extends the A2UI Basic Catalog with domain-specific components for IoT device management. It provides declarative UI components that bind to TinyIoTHub's device data model.

## Catalog Structure

```json
{
  "catalog": "iot",
  "version": "1.0.0",
  "extends": "basic",
  "components": [...]
}
```

---

## 1. DeviceCard Component

**Component ID:** `DeviceCard`

**Description:** Single device display card showing device status, key metrics, and quick actions. Suitable for dashboard grids and device lists.

### Properties (Inputs)

| Property | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|
| `deviceId` | `string` | Yes | - | Unique device identifier |
| `name` | `string` | Yes | - | Device display name |
| `status` | `enum` | Yes | - | Device status: `online`, `offline`, `warning`, `error` |
| `deviceType` | `string` | No | `"generic"` | Device type (sensor, actuator, gateway) |
| `protocol` | `string` | No | - | Protocol name (modbus, onvif, snmp, mqtt) |
| `lastSeen` | `string` | No | - | ISO 8601 timestamp of last heartbeat |
| `properties` | `array` | No | `[]` | Current property values `[{name, value, unit}]` |
| `showActions` | `boolean` | No | `true` | Show action buttons |
| `compact` | `boolean` | No | `false` | Compact card variant |

### Events (Outputs)

| Event | Payload | Description |
|-------|---------|-------------|
| `onClick` | `{ deviceId, name }` | Card clicked - expand details |
| `onCommand` | `{ deviceId, command }` | Quick command executed |
| `onRefresh` | `{ deviceId }` | Refresh device data |
| `onNavigate` | `{ deviceId, target }` | Navigate to device detail |

### Data Binding Format

```typescript
// Device card data binding
interface DeviceCardData {
  device: {
    id: string;
    name: string;
    displayName?: string;
    deviceType: string;
    protocolType: string;
    state: 0 | 1 | 2 | 3;  // offline=0, online=1, alarm=2, fault=3
    lastHeartbeat?: string;
    properties?: Array<{
      name: string;
      displayName?: string;
      currentValue: string;
      unit?: string;
      dataType: string;
    }>;
  };
  ui: {
    showActions?: boolean;
    compact?: boolean;
  };
}
```

### A2UI JSON Example

```json
{
  "type": "updateComponents",
  "payload": {
    "surfaceId": "device-dashboard",
    "components": [
      {
        "id": "card-001",
        "type": "DeviceCard",
        "props": {
          "deviceId": "dev-abc123",
          "name": "Temperature Sensor A1",
          "status": "online",
          "deviceType": "sensor",
          "protocol": "modbus",
          "lastSeen": "2026-04-03T10:30:00Z",
          "properties": [
            { "name": "temperature", "value": "25.6", "unit": "C" },
            { "name": "humidity", "value": "65", "unit": "%" }
          ],
          "showActions": true,
          "compact": false
        }
      }
    ]
  }
}
```

---

## 2. DeviceTable Component

**Component ID:** `DeviceTable`

**Description:** Multi-device list view with sortable columns, row selection, and pagination. Suitable for device management pages.

### Properties (Inputs)

| Property | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|
| `devices` | `array` | Yes | - | Array of device objects |
| `columns` | `array` | No | `default` | Column configuration |
| `sortColumn` | `string` | No | `"name"` | Default sort column |
| `sortOrder` | `enum` | No | `"asc"` | Sort direction: `asc`, `desc` |
| `page` | `number` | No | `1` | Current page |
| `pageSize` | `number` | No | `10` | Items per page |
| `total` | `number` | No | `0` | Total item count |
| `selectable` | `boolean` | No | `true` | Enable row selection |
| `selectionMode` | `enum` | No | `"single"` | `single`, `multi`, `none` |
| `selectedIds` | `array` | No | `[]` | Pre-selected device IDs |
| `emptyMessage` | `string` | No | - | Message when no data |

### Events (Outputs)

| Event | Payload | Description |
|-------|---------|-------------|
| `onSort` | `{ column, order }` | Sort changed |
| `onPage` | `{ page, pageSize }` | Pagination changed |
| `onSelect` | `{ selectedIds }` | Selection changed |
| `onRowClick` | `{ deviceId, device }` | Row clicked |
| `onAction` | `{ deviceId, action }` | Row action triggered |

### Column Configuration

```typescript
interface TableColumn {
  id: string;          // Property key
  label: string;       // Display label
  sortable?: boolean;  // Enable sorting
  width?: string;      // CSS width (e.g., "120px", "20%")
  align?: "left" | "center" | "right";
  format?: "text" | "status" | "protocol" | "datetime" | "property";
}
```

### Data Binding Format

```typescript
interface DeviceTableData {
  devices: Array<{
    id: string;
    name: string;
    displayName?: string;
    deviceType: string;
    protocolType: string;
    state: 0 | 1 | 2 | 3;
    lastHeartbeat?: string;
    createdAt: string;
  }>;
  pagination: {
    page: number;
    pageSize: number;
    total: number;
    totalPages: number;
  };
  selection: {
    mode: "single" | "multi" | "none";
    selectedIds: string[];
  };
}
```

### A2UI JSON Example

```json
{
  "type": "updateComponents",
  "payload": {
    "surfaceId": "device-list",
    "components": [
      {
        "id": "table-001",
        "type": "DeviceTable",
        "props": {
          "devices": [
            {
              "id": "dev-001",
              "name": "Sensor-001",
              "displayName": "Temperature Sensor 1",
              "deviceType": "sensor",
              "protocolType": "modbus",
              "state": 1,
              "lastHeartbeat": "2026-04-03T10:30:00Z"
            }
          ],
          "columns": [
            { "id": "name", "label": "Name", "sortable": true },
            { "id": "status", "label": "Status", "sortable": true, "format": "status" },
            { "id": "deviceType", "label": "Type" },
            { "id": "protocolType", "label": "Protocol", "format": "protocol" },
            { "id": "lastHeartbeat", "label": "Last Seen", "sortable": true, "format": "datetime" }
          ],
          "sortColumn": "name",
          "sortOrder": "asc",
          "page": 1,
          "pageSize": 10,
          "total": 50,
          "selectable": true,
          "selectionMode": "multi"
        }
      }
    ]
  }
}
```

---

## 3. DataChart Component

**Component ID:** `DataChart`

**Description:** Time-series data visualization component for sensor readings. Supports multiple series, time range selection, and real-time updates.

### Properties (Inputs)

| Property | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|
| `deviceId` | `string` | Yes | - | Device ID |
| `propertyName` | `string` | Yes | - | Property to chart |
| `series` | `array` | No | `[]` | Additional series config |
| `timeRange` | `enum` | No | `"1h"` | Time range: `5m`, `15m`, `30m`, `1h`, `6h`, `24h`, `7d` |
| `startTime` | `string` | No | - | Custom start time (ISO 8601) |
| `endTime` | `string` | No | - | Custom end time (ISO 8601) |
| `refreshInterval` | `number` | No | `0` | Auto-refresh interval (ms), 0 = disabled |
| `showLegend` | `boolean` | No | `true` | Show legend |
| `showGrid` | `boolean` | No | `true` | Show grid lines |
| `showTooltip` | `boolean` | No | `true` | Show data point tooltip |
| `chartType` | `enum` | No | `"line"` | Chart type: `line`, `area`, `bar` |
| `unit` | `string` | No | - | Y-axis unit |
| `minValue` | `number` | No | - | Y-axis minimum |
| `maxValue` | `number` | No | - | Y-axis maximum |

### Events (Outputs)

| Event | Payload | Description |
|-------|---------|-------------|
| `onTimeRangeChange` | `{ timeRange, startTime, endTime }` | Time range changed |
| `onDataPointClick` | `{ deviceId, property, timestamp, value }` | Data point clicked |
| `onExport` | `{ format, data }` | Data export requested |
| `onRefresh` | `{ deviceId, property }` | Manual refresh |

### Series Configuration

```typescript
interface ChartSeries {
  id: string;
  deviceId?: string;       // Optional for multi-device
  propertyName: string;
  label?: string;           // Display name
  color?: string;           // Line color
  yAxis?: "left" | "right"; // Y-axis side
}
```

### Data Binding Format

```typescript
interface DataChartData {
  deviceId: string;
  propertyName: string;
  dataPoints: Array<{
    timestamp: string;      // ISO 8601
    value: number;
  }>;
  series: ChartSeries[];
  timeRange: {
    start: string;
    end: string;
    label: string;
  };
  statistics?: {
    min: number;
    max: number;
    avg: number;
    count: number;
  };
}
```

### A2UI JSON Example

```json
{
  "type": "updateComponents",
  "payload": {
    "surfaceId": "device-detail",
    "components": [
      {
        "id": "chart-001",
        "type": "DataChart",
        "props": {
          "deviceId": "dev-abc123",
          "propertyName": "temperature",
          "series": [
            {
              "id": "temp-series",
              "propertyName": "temperature",
              "label": "Temperature",
              "color": "#3b82f6",
              "yAxis": "left"
            }
          ],
          "timeRange": "1h",
          "refreshInterval": 5000,
          "showLegend": true,
          "showGrid": true,
          "chartType": "line",
          "unit": "C",
          "minValue": -20,
          "maxValue": 100
        }
      }
    ]
  }
}
```

---

## 4. ControlPanel Component

**Component ID:** `ControlPanel`

**Description:** Device control widget for actuators. Provides ON/OFF toggles, value sliders, and command buttons with loading states.

### Properties (Inputs)

| Property | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|
| `deviceId` | `string` | Yes | - | Device ID |
| `commands` | `array` | Yes | - | Available commands |
| `properties` | `array` | No | `[]` | Writable properties |
| `loading` | `boolean` | No | `false` | Command executing |
| `loadingCommand` | `string` | No | - | Currently executing command ID |
| `disabled` | `boolean` | No | `false` | Disable all controls |
| `layout` | `enum` | No | `"vertical"` | `vertical`, `horizontal` |

### Events (Outputs)

| Event | Payload | Description |
|-------|---------|-------------|
| `onCommand` | `{ deviceId, command, parameters? }` | Command executed |
| `onPropertyChange` | `{ deviceId, property, value }` | Property value changed |
| `onPropertyCommit` | `{ deviceId, property, value }` | Property change confirmed |
| `onCancel` | `{ deviceId, command }` | Cancel ongoing command |

### Command Configuration

```typescript
interface ControlCommand {
  id: string;
  name: string;
  displayName?: string;
  description?: string;
  type: "button" | "toggle" | "slider" | "input";
  parameters?: Array<{
    name: string;
    type: "string" | "number" | "boolean";
    default?: any;
    min?: number;
    max?: number;
    options?: string[];  // For choice
  }>;
  confirmRequired?: boolean;
  icon?: string;
}
```

### Property Configuration

```typescript
interface ControlProperty {
  name: string;
  displayName?: string;
  dataType: "boolean" | "number" | "string";
  currentValue?: any;
  unit?: string;
  min?: number;
  max?: number;
  step?: number;
  options?: Array<{ value: string; label: string }>;
}
```

### Data Binding Format

```typescript
interface ControlPanelData {
  device: {
    id: string;
    name: string;
    isOnline: boolean;
  };
  commands: ControlCommand[];
  properties: ControlProperty[];
  execution: {
    loading: boolean;
    currentCommand?: string;
    lastResult?: {
      success: boolean;
      message: string;
    };
  };
}
```

### A2UI JSON Example

```json
{
  "type": "updateComponents",
  "payload": {
    "surfaceId": "device-control",
    "components": [
      {
        "id": "control-001",
        "type": "ControlPanel",
        "props": {
          "deviceId": "dev-actuator-01",
          "commands": [
            {
              "id": "turn_on",
              "name": "turn_on",
              "displayName": "Turn On",
              "type": "button",
              "icon": "power"
            },
            {
              "id": "turn_off",
              "name": "turn_off",
              "displayName": "Turn Off",
              "type": "button",
              "icon": "power"
            },
            {
              "id": "set_speed",
              "name": "set_speed",
              "displayName": "Fan Speed",
              "type": "slider",
              "parameters": [
                { "name": "speed", "type": "number", "min": 0, "max": 100, "default": 50 }
              ]
            }
          ],
          "properties": [
            {
              "name": "speed",
              "displayName": "Fan Speed",
              "dataType": "number",
              "currentValue": 50,
              "unit": "%",
              "min": 0,
              "max": 100,
              "step": 5
            }
          ],
          "loading": false,
          "layout": "vertical"
        }
      }
    ]
  }
}
```

---

## 5. ConfirmationDialog Component

**Component ID:** `ConfirmationDialog`

**Description:** Action confirmation dialog with danger level styling, auto-timeout option, and customizable buttons.

### Properties (Inputs)

| Property | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|
| `id` | `string` | Yes | - | Dialog ID |
| `title` | `string` | Yes | - | Dialog title |
| `message` | `string` | Yes | - | Confirmation message |
| `dangerLevel` | `enum` | No | `"normal"` | `normal`, `warning`, `destructive` |
| `confirmText` | `string` | No | `"Confirm"` | Confirm button text |
| `cancelText` | `string` | No | `"Cancel"` | Cancel button text |
| `timeout` | `number` | No | `0` | Auto-close timeout (ms), 0 = no timeout |
| `showIcon` | `boolean` | No | `true` | Show danger/warning icon |
| `loading` | `boolean` | No | `false` | Confirm button loading state |

### Events (Outputs)

| Event | Payload | Description |
|-------|---------|-------------|
| `onConfirm` | `{ dialogId }` | Confirm button clicked |
| `onCancel` | `{ dialogId }` | Cancel button clicked |
| `onTimeout` | `{ dialogId }` | Dialog timed out |
| `onClose` | `{ dialogId }` | Dialog closed (any reason) |

### A2UI JSON Example

```json
{
  "type": "updateComponents",
  "payload": {
    "surfaceId": "modal-layer",
    "components": [
      {
        "id": "confirm-delete",
        "type": "ConfirmationDialog",
        "props": {
          "id": "confirm-delete",
          "title": "Delete Device",
          "message": "Are you sure you want to delete this device? This action cannot be undone.",
          "dangerLevel": "destructive",
          "confirmText": "Delete",
          "cancelText": "Keep",
          "timeout": 0,
          "showIcon": true
        }
      }
    ]
  }
}
```

---

## 6. ProgressIndicator Component

**Component ID:** `ProgressIndicator`

**Description:** Batch operation progress display with percentage, status text, and cancel option.

### Properties (Inputs)

| Property | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|
| `id` | `string` | Yes | - | Progress ID |
| `label` | `string` | Yes | - | Operation label |
| `progress` | `number` | Yes | - | Progress percentage (0-100) |
| `status` | `string` | No | - | Status text |
| `current` | `number` | No | `0` | Current item count |
| `total` | `number` | No | `0` | Total item count |
| `showPercentage` | `boolean` | No | `true` | Show percentage |
| `showCancel` | `boolean` | No | `true` | Show cancel button |
| `indeterminate` | `boolean` | No | `false` | Indeterminate progress |
| `variant` | `enum` | No | `"default"` | `default`, `success`, `error` |

### Events (Outputs)

| Event | Payload | Description |
|-------|---------|-------------|
| `onCancel` | `{ progressId }` | Cancel button clicked |
| `onComplete` | `{ progressId }` | Progress completed |
| `onRetry` | `{ progressId }` | Retry button clicked |

### A2UI JSON Example

```json
{
  "type": "updateComponents",
  "payload": {
    "surfaceId": "modal-layer",
    "components": [
      {
        "id": "batch-progress",
        "type": "ProgressIndicator",
        "props": {
          "id": "batch-update",
          "label": "Updating Firmware",
          "progress": 65,
          "status": "Updating device 13 of 20",
          "current": 13,
          "total": 20,
          "showPercentage": true,
          "showCancel": true,
          "indeterminate": false,
          "variant": "default"
        }
      }
    ]
  }
}
```

---

## 7. RealTimeToggle Component

**Component ID:** `RealTimeToggle`

**Description:** Live data subscription toggle with connection status indicator and auto-reconnect behavior.

### Properties (Inputs)

| Property | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|
| `enabled` | `boolean` | Yes | - | Toggle state |
| `connectionStatus` | `enum` | Yes | - | `connected`, `reconnecting`, `disconnected` |
| `deviceIds` | `array` | No | `[]` | Subscribed device IDs |
| `label` | `string` | No | `"Live Data"` | Toggle label |
| `showStatus` | `boolean` | No | `true` | Show connection status |
| `autoReconnect` | `boolean` | No | `true` | Auto-reconnect on disconnect |
| `reconnectInterval` | `number` | No | `3000` | Reconnect interval (ms) |
| `maxReconnectAttempts` | `number` | No | `5` | Max reconnection attempts |

### Events (Outputs)

| Event | Payload | Description |
|-------|---------|-------------|
| `onToggle` | `{ enabled }` | Toggle changed |
| `onConnect` | `{}` | Connection established |
| `onDisconnect` | `{ reason }` | Connection lost |
| `onReconnect` | `{ attempt }` | Reconnection attempted |
| `onSubscribe` | `{ deviceIds }` | Device subscription changed |
| `onError` | `{ error }` | Connection error |

### Connection Status Display

| Status | Indicator Color | Text |
|--------|----------------|------|
| `connected` | Green (#22c55e) | Connected |
| `reconnecting` | Yellow (#eab308) | Reconnecting... |
| `disconnected` | Red (#ef4444) | Disconnected |

### A2UI JSON Example

```json
{
  "type": "updateComponents",
  "payload": {
    "surfaceId": "dashboard-header",
    "components": [
      {
        "id": "realtime-toggle",
        "type": "RealTimeToggle",
        "props": {
          "enabled": true,
          "connectionStatus": "connected",
          "deviceIds": ["dev-001", "dev-002", "dev-003"],
          "label": "Live Data",
          "showStatus": true,
          "autoReconnect": true,
          "reconnectInterval": 3000,
          "maxReconnectAttempts": 5
        }
      }
    ]
  }
}
```

---

## TinyIoTHub Device Data Model Reference

### Device Entity

```typescript
interface Device {
  id: string;
  name: string;
  displayName?: string;
  deviceType?: string;      // sensor, actuator, gateway
  address?: string;
  description?: string;
  position?: string;
  driverName?: string;       // modbus, onvif, snmp, mqtt
  deviceModel?: string;
  protocolType?: string;
  factoryName?: string;
  state: 0 | 1 | 2 | 3;      // 0=offline, 1=online, 2=alarm, 3=fault
  parentId?: string;
  productId?: string;
  tenantId?: string;
  createdAt?: string;
  updatedAt?: string;
  isOnline: boolean;         // Runtime field
  lastHeartbeat?: string;    // Runtime field
  properties?: DeviceProperty[];
  commands?: DeviceCommand[];
}
```

### DeviceProperty Entity

```typescript
interface DeviceProperty {
  id: string;
  deviceId: string;
  name: string;
  displayName?: string;
  description?: string;
  dataType?: string;         // string, int, float, bool
  unit?: string;
  minValue?: number;
  maxValue?: number;
  defaultValue?: string;
  isReadOnly: 0 | 1;
  currentValue?: string;     // Runtime field
  alarmStatus?: number;      // Runtime field
}
```

### DeviceCommand Entity

```typescript
interface DeviceCommand {
  id: string;
  deviceId: string;
  name: string;
  displayName?: string;
  description?: string;
  parameters?: string;       // JSON string
  createdAt: string;
}
```

---

## Integration with Basic Catalog

The IoT Catalog components can be composed with Basic Catalog components:

```json
{
  "type": "updateComponents",
  "payload": {
    "surfaceId": "dashboard",
    "components": [
      {
        "id": "header-row",
        "type": "Row",
        "children": [
          { "id": "title", "type": "Text", "props": { "content": "Device Dashboard" } },
          { "id": "realtime", "type": "RealTimeToggle", "props": { "enabled": true, "connectionStatus": "connected" } }
        ]
      },
      {
        "id": "stats-card",
        "type": "Card",
        "children": [
          { "id": "chart", "type": "DataChart", "props": { "deviceId": "dev-001", "propertyName": "temperature" } }
        ]
      },
      {
        "id": "device-list",
        "type": "DeviceTable",
        "props": { "devices": [], "selectable": true }
      }
    ]
  }
}
```

---

## Appendix: A2UI Message Types Reference

| Message Type | Description |
|--------------|-------------|
| `createSurface` | Create a new surface container |
| `updateComponents` | Add/update components on a surface |
| `updateDataModel` | Update component data bindings |
| `deleteSurface` | Remove a surface |
| `callFunction` | Agent calls a function |
| `actionResponse` | Client responds to an action |

## Appendix: Basic Catalog Components (Extended)

The IoT Catalog extends these Basic Catalog components:

- **Text** - Display text content
- **Image** - Display images/icons
- **Icon** - Protocol/type icons
- **Row** - Horizontal layout container
- **Column** - Vertical layout container
- **Card** - Container with shadow/border
- **List** - Scrollable list
- **Tabs** - Tab navigation
- **Modal** - Modal overlay container
- **Button** - Interactive button
- **TextField** - Text input
- **CheckBox** - Boolean toggle
- **ChoicePicker** - Single/multi select
- **Slider** - Range input
- **DateTimeInput** - Date/time picker
- **Divider** - Horizontal separator
- **Video** - Video player
- **AudioPlayer** - Audio player
