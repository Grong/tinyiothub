# Full-Screen Device Creation Wizard

## Problem

Current device creation wizard is a small modal (720px max, 90vh max) that doesn't scale for many templates and doesn't show template context during device configuration.

## Solution

Replace the modal with a full-screen, 2-step wizard:
- **Step 1**: Full-screen template selection with search and category grid
- **Step 2**: Split-panel — left: device form, right: template overview (properties, commands, stats)

## Layout

### Step 1: Template Selection

Full-screen overlay (`position: fixed; inset: 0; z-index: 1000`).

```
Header: ← 返回设备列表 | 选择设备模板 | step dots (● ○)
Search: [centered search bar, max-width 640px]
Body:   Templates grouped by category, 5-6 column CSS grid
Cards:  Larger than current — icon, name, manufacturer, protocol, version
```

Click a card → auto-fill defaults from template → advance to Step 2.

### Step 2: Device Info + Overview (Split Panel)

CSS Grid layout: `grid-template-columns: 1fr 420px`.

**Left panel (form)**:
- Header: ← 返回模板选择 | 填写设备信息
- Form fields: name (required 2-50), description, address (conditionally required), position, driver select
- Driver config section: dynamic fields from `driverApi.getDriverConfig()`
- Footer (sticky): 取消 | 创建设备

**Right panel (overview)**:
- Template summary card: icon, name, version, manufacturer, protocol, category, builtin badge
- Stats grid (2x2): 属性数, 命令数, 只读属性, 可写属性
- Property list: name, data type, unit (scrollable, max ~200px)
- Command list: name, description (scrollable)
- Independent scroll from left panel

## Data Source

All overview data comes from the `Template` object already loaded in Step 1:
- `template.properties[]` → property list + readonly/writable counts
- `template.commands[]` → command list + total count
- No additional API calls needed

## CSS

New block in `components.css`:
- `.wizard-fullscreen`: `position: fixed; inset: 0; z-index: 1000; background: var(--bg); display: flex; flex-direction: column;`
- `.wizard-fullscreen__header`: top bar with back button, title, step indicator
- `.wizard-fullscreen__body`: `flex: 1; overflow-y: auto; padding: 24px 32px;`
- `.wizard-split`: `display: grid; grid-template-columns: 1fr 420px; height: 100%;`
- `.wizard-split__form`: left panel, `overflow-y: auto; padding: 24px 32px;`
- `.wizard-split__overview`: right panel, `overflow-y: auto; padding: 24px; border-left: 1px solid var(--border); background: var(--card);`

## Files Modified

1. `web/src/ui/views/devices.ts` — replace `renderWizard()` and sub-methods
2. `web/src/styles/components.css` — add `.wizard-fullscreen` CSS block

## Validation

Same as current:
- Device name: 2-50 chars, required
- Address: conditionally required via `deviceInfo.requiredFields`
- Driver config: required fields checked against both user value and `defaultValue`
