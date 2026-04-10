# Agent Management & Chat Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the Agent management page (model config, tool permissions, placeholder tabs) and Chat page (A2UI IoT components, onAction wiring, inline A2UI in messages) to production quality.

**Architecture:** Lit 3 Web Components with Light DOM. Extract tab rendering from agents.ts into separate files to prevent growth. Reuse existing `AgentConfig.alternativeModels` for model dropdown (no new API). A2UI IoT components use `onAction` callback to send device commands back through the chat system.

**Tech Stack:** Lit 3, TypeScript, Vite, CSS (no third-party chart library), existing Axum backend APIs.

---

## File Structure

| File | Responsibility |
|------|----------------|
| `web/src/ui/views/agents.ts` | Main shell: agent selector dropdown, tab routing, save bar |
| `web/src/ui/views/agents-model-tab.ts` | Model config tab: dropdown + temperature + system prompt + save |
| `web/src/ui/views/agents-tools-tab.ts` | Tools tab: search + batch ops + danger markers |
| `web/src/ui/views/agents-placeholder.ts` | Unified placeholder for skills/channels/cron/files tabs |
| `web/src/ui/controllers/agents.ts` | Existing — no changes needed (alternativeModels already in type) |
| `web/src/ui/views/chat.ts` | Wire onAction, inline A2UI into messages, session sidebar improvements |
| `web/src/ui/controllers/chat.ts` | Associate a2ui surface IDs with streaming messages |
| `web/src/ui/chat/grouped-render.ts` | renderSingleMessage: inline A2UI surface if message has one |
| `web/src/ui/chat/message-normalizer.ts` | Add `a2uiSurfaceId` field to NormalizedMessage |
| `web/src/ui/chat/a2ui/catalog/device-card.ts` | Rewrite: telemetry, sparkline, status colors, action buttons |
| `web/src/ui/chat/a2ui/catalog/device-table.ts` | Rewrite: sort, status colors, action columns |
| `web/src/ui/chat/a2ui/catalog/data-chart.ts` | Rewrite: pure SVG line chart |
| `web/src/ui/chat/a2ui/catalog/control-panel.ts` | Rewrite: slider/toggle/choice/button controls |
| `web/src/ui/chat/a2ui/catalog/sparkline.ts` | **New**: shared SVG sparkline utility |
| `web/src/styles/components.css` | New CSS for model form, tools improvements, A2UI IoT components |

---

## Phase 1A: Agent Model Config Tab

### Task 1: Extract model tab into separate file

**Files:**
- Create: `web/src/ui/views/agents-model-tab.ts`
- Modify: `web/src/ui/views/agents.ts` (import + call new file)

- [ ] **Step 1: Create agents-model-tab.ts**

```typescript
import { html, nothing, type TemplateResult } from "lit";
import type { AgentsState } from "../controllers/agents.js";

export function renderModelTab(
  state: AgentsState,
  onStateChange: (patch: Partial<AgentsState>) => void,
  onSave: () => void,
  onReload: () => void,
): TemplateResult {
  const config = state.config;
  if (state.configLoading) {
    return html`<div class="agent-panel-loading">加载中...</div>`;
  }
  if (!config) {
    return html`<div class="agent-panel-empty">未找到配置</div>`;
  }

  const models: string[] = config.alternativeModels?.length
    ? config.alternativeModels
    : [config.model || "default"];
  const currentModel = config.model || models[0];

  return html`
    <div class="agent-model-tab">
      <div class="agent-field">
        <label class="agent-field__label">模型</label>
        <select class="agent-model-dropdown"
                .value=${currentModel}
                @change=${(e: Event) => {
                  onStateChange({
                    config: { ...config, model: (e.target as HTMLSelectElement).value },
                    configDirty: true,
                  });
                }}>
          ${models.map((m) => html`<option value=${m} ?selected=${m === currentModel}>${m}</option>`)}
        </select>
      </div>

      <div class="agent-field">
        <label class="agent-field__label">Temperature</label>
        <div class="agent-slider-row">
          <input type="range" class="agent-slider" min="0" max="2" step="0.1"
                 .value=${String(config.temperature ?? 1.0)}
                 @input=${(e: Event) => {
                   onStateChange({
                     config: { ...config, temperature: parseFloat((e.target as HTMLInputElement).value) },
                     configDirty: true,
                   });
                 }} />
          <span class="agent-slider-value">${(config.temperature ?? 1.0).toFixed(1)}</span>
        </div>
      </div>

      <div class="agent-field">
        <label class="agent-field__label">System Prompt</label>
        <textarea class="agent-system-prompt" rows="6"
                  .value=${config.systemPrompt || ""}
                  @input=${(e: Event) => {
                    onStateChange({
                      config: { ...config, systemPrompt: (e.target as HTMLTextAreaElement).value },
                      configDirty: true,
                    });
                  }}></textarea>
      </div>

      <div class="agent-actions">
        <button class="btn btn-primary" ?disabled=${!state.configDirty}
                @click=${onSave}>
          保存${state.configDirty ? " *" : ""}
        </button>
        <button class="btn" @click=${onReload}>重新加载</button>
      </div>
    </div>
  `;
}
```

- [ ] **Step 2: Run build to verify no type errors**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npx tsc --noEmit 2>&1 | head -20`
Expected: No errors related to agents-model-tab.ts

- [ ] **Step 3: Update agents.ts to import and use the new tab**

Replace `renderOverview()` method body with a call to `renderModelTab`. Remove the hardcoded models array and inline model chip rendering.

```typescript
// In agents.ts, add import:
import { renderModelTab } from "./agents-model-tab.js";

// Replace renderOverview() call in render():
// was: this.renderOverview()
// now: renderModelTab(this.state, this._patchState.bind(this), this.onSaveConfig.bind(this), ...)

// Add helper method:
private _patchState(patch: Partial<AgentsState>): void {
  this.state = { ...this.state, ...patch };
}
```

- [ ] **Step 4: Run build to verify**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npx tsc --noEmit 2>&1 | head -20`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/views/agents-model-tab.ts web/src/ui/views/agents.ts
git commit -m "feat(agents): extract model config tab with dropdown + temperature + system prompt"
```

### Task 2: Add CSS for model tab

**Files:**
- Modify: `web/src/styles/components.css`

- [ ] **Step 1: Add model tab styles**

Append after existing `.agent-model-select` block (line ~2626):

```css
.agent-model-tab {
  display: grid;
  gap: 20px;
  max-width: 560px;
}

.agent-field {
  display: grid;
  gap: 6px;
}

.agent-field__label {
  font-size: 12px;
  font-weight: 600;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.agent-model-dropdown {
  padding: 8px 12px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--card);
  color: var(--text);
  font-size: 14px;
  cursor: pointer;
}

.agent-slider-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.agent-slider {
  flex: 1;
  accent-color: var(--accent, #3b82f6);
}

.agent-slider-value {
  font-size: 14px;
  font-weight: 600;
  min-width: 36px;
  text-align: right;
}

.agent-system-prompt {
  padding: 10px 12px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--card);
  color: var(--text);
  font-family: var(--mono, monospace);
  font-size: 13px;
  line-height: 1.5;
  resize: vertical;
}

.agent-actions {
  display: flex;
  gap: 8px;
  padding-top: 8px;
}
```

- [ ] **Step 2: Commit**

```bash
git add web/src/styles/components.css
git commit -m "style(agents): add model config tab styles"
```

---

## Phase 1B: Tools Tab Improvements

### Task 3: Extract tools tab with search + batch + danger markers

**Files:**
- Create: `web/src/ui/views/agents-tools-tab.ts`
- Modify: `web/src/ui/views/agents.ts` (import + call)

- [ ] **Step 1: Create agents-tools-tab.ts**

```typescript
import { html, nothing, type TemplateResult } from "lit";
import type { AgentsState } from "../controllers/agents.js";

const DANGEROUS_TOOLS = new Set(["device_delete", "workspace_delete", "agent_delete", "batch_delete"]);

export function renderToolsTab(
  state: AgentsState,
  searchFilter: string,
  onSearchChange: (v: string) => void,
  onToggleTool: (name: string, enabled: boolean) => void,
): TemplateResult {
  if (state.toolsCatalogLoading) {
    return html`<div class="agent-panel-loading">加载工具目录...</div>`;
  }
  if (!state.toolsCatalog?.length) {
    return html`<div class="agent-panel-empty">暂无可用工具</div>`;
  }

  const filter = searchFilter.toLowerCase();

  return html`
    <div class="agent-tools-tab">
      <div class="agent-tools-toolbar">
        <input type="text" class="agent-tools-search" placeholder="搜索工具..."
               .value=${searchFilter}
               @input=${(e: Event) => onSearchChange((e.target as HTMLInputElement).value)} />
        <button class="btn btn-sm" @click=${() => {
          for (const g of state.toolsCatalog || []) {
            for (const t of (g.tools || []) as Record<string, unknown>[]) {
              onToggleTool(t.name as string, true);
            }
          }
        }}>全部启用</button>
        <button class="btn btn-sm" @click=${() => {
          for (const g of state.toolsCatalog || []) {
            for (const t of (g.tools || []) as Record<string, unknown>[]) {
              onToggleTool(t.name as string, false);
            }
          }
        }}>全部禁用</button>
      </div>

      ${state.toolsCatalog.map((group) => {
        const tools = ((group.tools || []) as Record<string, unknown>[]).filter((t) =>
          !filter || (t.name as string).toLowerCase().includes(filter) || ((t.description as string) || "").toLowerCase().includes(filter)
        );
        if (!tools.length) return nothing;

        return html`
          <div class="agent-tool-group">
            <h4 class="agent-tool-group__title">${group.label || group.name}</h4>
            <div class="agent-tool-list">
              ${tools.map((tool) => html`
                <div class="agent-tool-item ${DANGEROUS_TOOLS.has(tool.name as string) ? 'agent-tool-item--danger' : ''}">
                  <div class="agent-tool-info">
                    <span class="agent-tool-name">${tool.name as string}</span>
                    <span class="agent-tool-desc">${(tool.description as string) || ""}</span>
                  </div>
                  <label class="agent-toggle">
                    <input type="checkbox"
                           ?checked=${tool.enabled as boolean}
                           @change=${(e: Event) => onToggleTool(tool.name as string, (e.target as HTMLInputElement).checked)} />
                    <span class="agent-toggle__slider"></span>
                  </label>
                </div>
              `)}
            </div>
          </div>
        `;
      })}
    </div>
  `;
}
```

- [ ] **Step 2: Update agents.ts to use renderToolsTab**

Replace `renderTools()` call with `renderToolsTab(...)`. Pass `this.searchFilter` and a setter.

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/views/agents-tools-tab.ts web/src/ui/views/agents.ts
git commit -m "feat(agents): add tools tab with search, batch ops, danger markers"
```

### Task 4: Add tools tab CSS

**Files:**
- Modify: `web/src/styles/components.css`

- [ ] **Step 1: Add tools toolbar + danger styles**

```css
.agent-tools-toolbar {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-bottom: 12px;
}

.agent-tools-search {
  flex: 1;
  padding: 6px 12px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--card);
  color: var(--text);
  font-size: 13px;
}

.agent-tool-item--danger {
  border-left: 3px solid #e74c3c;
}

.agent-tool-item--danger .agent-tool-name {
  color: #e74c3c;
}
```

- [ ] **Step 2: Commit**

```bash
git add web/src/styles/components.css
git commit -m "style(agents): add tools tab search and danger marker styles"
```

---

## Phase 1C: Placeholder Tabs + Agent Selector

### Task 5: Unified placeholder + agent dropdown selector

**Files:**
- Create: `web/src/ui/views/agents-placeholder.ts`
- Modify: `web/src/ui/views/agents.ts`

- [ ] **Step 1: Create agents-placeholder.ts**

```typescript
import { html, type TemplateResult } from "lit";

const placeholderData: Record<string, { icon: string; desc: string }> = {
  files: { icon: "📁", desc: "Agent 可以访问和管理文件资源。此功能即将推出。" },
  skills: { icon: "🔧", desc: "Agent 可以扩展自定义技能来处理特定任务。此功能即将推出。" },
  channels: { icon: "📡", desc: "Agent 可以通过多种渠道（邮件、Webhook 等）发送通知。此功能即将推出。" },
  cron: { icon: "⏰", desc: "Agent 可以配置定时任务，按计划自动执行操作。此功能即将推出。" },
};

export function renderPlaceholder(panel: string): TemplateResult {
  const data = placeholderData[panel] || { icon: "⚡", desc: "此功能即将推出。" };
  return html`
    <div class="agent-placeholder">
      <div class="agent-placeholder__icon">${data.icon}</div>
      <p class="agent-placeholder__desc">${data.desc}</p>
    </div>
  `;
}
```

- [ ] **Step 2: Replace agent pill selector with dropdown**

In `agents.ts`, replace the `agents.map(...)` pill buttons with a `<select>` dropdown:

```typescript
// In render(), replace agents-selector div:
<div class="agents-selector">
  <select class="agent-dropdown"
          @change=${(e: Event) => this.onAgentSelected((e.target as HTMLSelectElement).value)}>
    ${agents.map((a) => html`
      <option value=${a.id} ?selected=${a.id === this.state.selectedAgentId}>
        ${a.name || a.id}
      </option>
    `)}
  </select>
</div>
```

- [ ] **Step 3: Add placeholder + dropdown CSS**

```css
.agent-dropdown {
  padding: 6px 12px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--card);
  color: var(--text);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  min-width: 200px;
}

.agent-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 48px 24px;
  text-align: center;
  color: var(--muted);
}

.agent-placeholder__icon {
  font-size: 32px;
  margin-bottom: 12px;
}

.agent-placeholder__desc {
  font-size: 14px;
  max-width: 320px;
  line-height: 1.5;
}
```

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/views/agents-placeholder.ts web/src/ui/views/agents.ts web/src/styles/components.css
git commit -m "feat(agents): add placeholder tabs, switch agent selector to dropdown"
```

---

## Phase 2A: A2UI IoT Components

### Task 6: Create sparkline SVG utility

**Files:**
- Create: `web/src/ui/chat/a2ui/catalog/sparkline.ts`

- [ ] **Step 1: Create sparkline.ts**

```typescript
export function renderSparkline(
  data: number[],
  width: number = 80,
  height: number = 24,
  color: string = "#3b82f6",
): string {
  if (!data.length) return "";
  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1;
  const step = width / (data.length - 1 || 1);

  const points = data
    .map((v, i) => `${(i * step).toFixed(1)},${(height - ((v - min) / range) * height).toFixed(1)}`)
    .join(" ");

  return `<svg width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" xmlns="http://www.w3.org/2000/svg">
    <polyline points="${points}" fill="none" stroke="${color}" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
  </svg>`;
}
```

- [ ] **Step 2: Commit**

```bash
git add web/src/ui/chat/a2ui/catalog/sparkline.ts
git commit -m "feat(a2ui): add shared sparkline SVG utility"
```

### Task 7: Rewrite DeviceCard

**Files:**
- Modify: `web/src/ui/chat/a2ui/catalog/device-card.ts`

- [ ] **Step 1: Rewrite device-card.ts**

```typescript
import { html, nothing, type TemplateResult } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { renderSparkline } from "./sparkline.js";

const STATUS_COLORS: Record<string, string> = {
  online: "#2ecc71",
  offline: "#95a5a6",
  warning: "#f39c12",
  error: "#e74c3c",
};

const STATUS_LABELS: Record<string, string> = {
  online: "在线",
  offline: "离线",
  warning: "告警",
  error: "故障",
};

export function renderDeviceCard(
  data: Record<string, unknown>,
  onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const deviceId = String(data.deviceId || "");
  const deviceName = String(data.name || data.deviceName || deviceId);
  const status = String(data.status || "unknown");
  const telemetry = (data.telemetry as Array<{ key: string; value: string; unit: string }>) || [];
  const sparkline = data.sparkline as number[] | undefined;
  const lastSeen = data.lastSeen as string | undefined;
  const actions = (data.actions as Array<{ label: string; functionId: string }>) || [];

  const statusColor = STATUS_COLORS[status] || "#95a5a6";
  const statusLabel = STATUS_LABELS[status] || status;

  return html`
    <div class="a2ui-device-card">
      <div class="a2ui-device-card__header">
        <span class="a2ui-device-card__status" style="background: ${statusColor}"></span>
        <span class="a2ui-device-card__name">${deviceName}</span>
        <span class="a2ui-device-card__badge" style="color: ${statusColor}">${statusLabel}</span>
      </div>

      ${telemetry.length ? html`
        <div class="a2ui-device-card__telemetry">
          ${telemetry.map((t) => html`
            <div class="a2ui-device-card__metric">
              <span class="a2ui-device-card__metric-key">${t.key}</span>
              <span class="a2ui-device-card__metric-value">${t.value}${t.unit ? ` ${t.unit}` : ""}</span>
            </div>
          `)}
        </div>
      ` : nothing}

      ${sparkline?.length ? html`
        <div class="a2ui-device-card__sparkline">
          ${unsafeHTML(renderSparkline(sparkline, 120, 28, statusColor))}
        </div>
      ` : nothing}

      ${lastSeen ? html`
        <div class="a2ui-device-card__last-seen">
          最后活跃: ${new Date(lastSeen).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}
        </div>
      ` : nothing}

      ${actions.length ? html`
        <div class="a2ui-device-card__actions">
          ${actions.map((a) => html`
            <button class="a2ui-btn a2ui-btn--secondary a2ui-btn--sm"
                    @click=${() => { if (onAction) onAction(a.functionId, { deviceId }); }}>
              ${a.label}
            </button>
          `)}
        </div>
      ` : nothing}

      <div class="a2ui-device-card__id">${deviceId}</div>
    </div>
  `;
}
```

- [ ] **Step 2: Add DeviceCard CSS**

Append to components.css after existing `.a2ui-device-card__id`:

```css
.a2ui-device-card__badge {
  margin-left: auto;
  font-size: 11px;
  font-weight: 600;
}

.a2ui-device-card__telemetry {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--border);
}

.a2ui-device-card__metric {
  display: flex;
  justify-content: space-between;
  font-size: 13px;
}

.a2ui-device-card__metric-key {
  color: var(--muted);
}

.a2ui-device-card__metric-value {
  font-weight: 600;
}

.a2ui-device-card__sparkline {
  margin-top: 8px;
}

.a2ui-device-card__last-seen {
  margin-top: 6px;
  font-size: 11px;
  color: var(--muted);
}

.a2ui-device-card__actions {
  display: flex;
  gap: 6px;
  margin-top: 10px;
}

.a2ui-btn--sm {
  padding: 3px 10px;
  font-size: 12px;
}
```

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/chat/a2ui/catalog/device-card.ts web/src/styles/components.css
git commit -m "feat(a2ui): complete DeviceCard with telemetry, sparkline, status, actions"
```

### Task 8: Rewrite DeviceTable

**Files:**
- Modify: `web/src/ui/chat/a2ui/catalog/device-table.ts`

- [ ] **Step 1: Rewrite device-table.ts**

```typescript
import { html, nothing, type TemplateResult } from "lit";

const STATUS_COLORS: Record<string, string> = {
  online: "#2ecc71",
  offline: "#95a5a6",
  warning: "#f39c12",
  error: "#e74c3c",
};

export function renderDeviceTable(
  data: Record<string, unknown>,
  onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const title = String(data.title || "");
  const columns = (data.columns as string[]) || ["设备名称", "状态", "最新数据", "操作"];
  const devices = (data.devices as Array<Record<string, unknown>>) || [];

  return html`
    <div class="a2ui-device-table">
      ${title ? html`<div class="a2ui-device-table__title">${title}</div>` : nothing}
      <table class="a2ui-device-table__table">
        <thead>
          <tr>${columns.map((c) => html`<th>${c}</th>`)}</tr>
        </thead>
        <tbody>
          ${devices.map((d) => {
            const status = String(d.status || "unknown");
            const statusColor = STATUS_COLORS[status] || "#95a5a6";
            const actions = (d.actions as Array<{ label: string; functionId: string }>) || [];

            return html`
              <tr>
                <td>${String(d.name || d.id || "")}</td>
                <td>
                  <span class="a2ui-device-table__status" style="color: ${statusColor}">
                    ● ${status}
                  </span>
                </td>
                <td>${String(d.latestData || d.id || "")}</td>
                <td>
                  <div class="a2ui-device-table__actions">
                    ${actions.map((a) => html`
                      <button class="a2ui-btn a2ui-btn--secondary a2ui-btn--sm"
                              @click=${() => { if (onAction) onAction(a.functionId, { deviceId: d.id }); }}>
                        ${a.label}
                      </button>
                    `)}
                  </div>
                </td>
              </tr>
            `;
          })}
        </tbody>
      </table>
      ${devices.length === 0 ? html`<div class="a2ui-caption" style="padding: 12px">暂无设备</div>` : nothing}
    </div>
  `;
}
```

- [ ] **Step 2: Add DeviceTable CSS**

```css
.a2ui-device-table__title {
  font-weight: 600;
  font-size: 14px;
  margin-bottom: 8px;
}

.a2ui-device-table__table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.a2ui-device-table__table th {
  text-align: left;
  padding: 8px 10px;
  font-size: 11px;
  font-weight: 600;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  border-bottom: 1px solid var(--border);
}

.a2ui-device-table__table td {
  padding: 8px 10px;
  border-bottom: 1px solid var(--border);
}

.a2ui-device-table__status {
  font-weight: 600;
  font-size: 12px;
}

.a2ui-device-table__actions {
  display: flex;
  gap: 4px;
}
```

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/chat/a2ui/catalog/device-table.ts web/src/styles/components.css
git commit -m "feat(a2ui): complete DeviceTable with status colors, action columns"
```

### Task 9: Rewrite DataChart (pure SVG)

**Files:**
- Modify: `web/src/ui/chat/a2ui/catalog/data-chart.ts`

- [ ] **Step 1: Rewrite data-chart.ts**

```typescript
import { html, nothing, type TemplateResult } from "lit";

export function renderDataChart(
  data: Record<string, unknown>,
  _onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const title = String(data.title || "图表");
  const unit = String(data.unit || "");
  const timeRange = String(data.timeRange || "1h");
  const series = (data.series as Array<{
    name: string;
    color: string;
    data: Array<{ time: string; value: number }>;
  }>) || [];
  const thresholds = (data.thresholds as Array<{ label: string; value: number; color: string }>) || [];

  const width = 360;
  const height = 160;
  const padLeft = 36;
  const padRight = 12;
  const padTop = 12;
  const padBottom = 24;
  const chartW = width - padLeft - padRight;
  const chartH = height - padTop - padBottom;

  // Collect all values for Y range
  const allValues = series.flatMap((s) => s.data.map((d) => d.value));
  const allThresholdValues = thresholds.map((t) => t.value);
  const yMin = Math.min(0, ...allValues, ...allThresholdValues);
  const yMax = Math.max(1, ...allValues, ...allThresholdValues) * 1.1;
  const yRange = yMax - yMin || 1;

  function toX(i: number, total: number): number {
    return padLeft + (i / (total - 1 || 1)) * chartW;
  }
  function toY(v: number): number {
    return padTop + chartH - ((v - yMin) / yRange) * chartH;
  }

  // Build polyline points for each series
  const seriesPolylines = series.map((s) => {
    const points = s.data
      .map((d, i) => `${toX(i, s.data.length).toFixed(1)},${toY(d.value).toFixed(1)}`)
      .join(" ");
    return { name: s.name, color: s.color, points };
  });

  // Y axis labels (4 ticks)
  const yTicks = [0, 0.25, 0.5, 0.75, 1].map((p) => ({
    value: yMin + p * yRange,
    y: toY(yMin + p * yRange),
  }));

  // X axis labels (first, middle, last)
  const firstSeries = series[0];
  const xLabels: Array<{ label: string; x: number }> = [];
  if (firstSeries?.data.length) {
    const len = firstSeries.data.length;
    const indices = [0, Math.floor(len / 2), len - 1];
    for (const i of indices) {
      const t = firstSeries.data[i]?.time;
      if (t) {
        xLabels.push({
          label: new Date(t).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }),
          x: toX(i, len),
        });
      }
    }
  }

  return html`
    <div class="a2ui-data-chart">
      <div class="a2ui-data-chart__header">
        <span class="a2ui-data-chart__title">${title}</span>
        <span class="a2ui-data-chart__range">${timeRange}</span>
      </div>

      <svg width="100%" viewBox="0 0 ${width} ${height}" xmlns="http://www.w3.org/2000/svg"
           class="a2ui-data-chart__svg">
        {/* Grid lines */}
        ${yTicks.map((t) => html`
          <line x1=${padLeft} y1=${t.y.toFixed(1)} x2=${width - padRight} y2=${t.y.toFixed(1)}
                stroke="var(--border)" stroke-width="0.5" />
        `)}

        {/* Y axis labels */}
        ${yTicks.map((t) => html`
          <text x=${padLeft - 4} y=${(t.y + 3).toFixed(1)} text-anchor="end"
                fill="var(--muted)" font-size="10">
            ${t.value.toFixed(0)}${unit ? ` ${unit}` : ""}
          </text>
        `)}

        {/* Threshold lines */}
        ${thresholds.map((th) => html`
          <line x1=${padLeft} y1=${toY(th.value).toFixed(1)}
                x2=${width - padRight} y2=${toY(th.value).toFixed(1)}
                stroke=${th.color} stroke-width="1" stroke-dasharray="4 2" />
          <text x=${width - padRight - 2} y=${(toY(th.value) - 4).toFixed(1)}
                text-anchor="end" fill=${th.color} font-size="9">${th.label}</text>
        `)}

        {/* Series polylines */}
        ${seriesPolylines.map((s) => html`
          <polyline points=${s.points} fill="none" stroke=${s.color}
                    stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
        `)}

        {/* X axis labels */}
        ${xLabels.map((l) => html`
          <text x=${l.x.toFixed(1)} y=${height - 4} text-anchor="middle"
                fill="var(--muted)" font-size="10">${l.label}</text>
        `)}
      </svg>

      ${series.length > 1 ? html`
        <div class="a2ui-data-chart__legend">
          ${series.map((s) => html`
            <span class="a2ui-data-chart__legend-item">
              <span class="a2ui-data-chart__legend-dot" style="background: ${s.color}"></span>
              ${s.name}
            </span>
          `)}
        </div>
      ` : nothing}
    </div>
  `;
}
```

- [ ] **Step 2: Add DataChart CSS**

```css
.a2ui-data-chart {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px;
  background: var(--card);
}

.a2ui-data-chart__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
}

.a2ui-data-chart__title {
  font-weight: 600;
  font-size: 14px;
}

.a2ui-data-chart__range {
  font-size: 11px;
  color: var(--muted);
}

.a2ui-data-chart__svg {
  display: block;
}

.a2ui-data-chart__legend {
  display: flex;
  gap: 12px;
  margin-top: 8px;
  font-size: 12px;
}

.a2ui-data-chart__legend-item {
  display: flex;
  align-items: center;
  gap: 4px;
}

.a2ui-data-chart__legend-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
```

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/chat/a2ui/catalog/data-chart.ts web/src/styles/components.css
git commit -m "feat(a2ui): implement DataChart with pure SVG line chart"
```

### Task 10: Rewrite ControlPanel

**Files:**
- Modify: `web/src/ui/chat/a2ui/catalog/control-panel.ts`

- [ ] **Step 1: Rewrite control-panel.ts**

```typescript
import { html, nothing, type TemplateResult } from "lit";

export function renderControlPanel(
  data: Record<string, unknown>,
  onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const deviceName = String(data.deviceName || data.deviceId || "");
  const controls = (data.controls as Array<Record<string, unknown>>) || [];

  return html`
    <div class="a2ui-control-panel">
      <div class="a2ui-control-panel__header">控制面板: ${deviceName}</div>

      ${controls.map((ctrl) => {
        const type = String(ctrl.type || "button");
        const label = String(ctrl.label || ctrl.id || "");
        const id = String(ctrl.id || "");

        if (type === "slider") {
          const min = Number(ctrl.min ?? 0);
          const max = Number(ctrl.max ?? 100);
          const step = Number(ctrl.step ?? 1);
          const value = Number(ctrl.value ?? min);
          const unit = String(ctrl.unit || "");

          return html`
            <div class="a2ui-control-panel__field">
              <label class="a2ui-control-panel__label">${label}</label>
              <div class="a2ui-control-panel__slider-row">
                <span class="a2ui-control-panel__range-label">${min}${unit}</span>
                <input type="range" class="a2ui-control-panel__slider"
                       min=${min} max=${max} step=${step} value=${value}
                       @change=${(e: Event) => {
                         if (onAction) onAction(id, { value: parseFloat((e.target as HTMLInputElement).value) });
                       }} />
                <span class="a2ui-control-panel__range-label">${max}${unit}</span>
              </div>
            </div>
          `;
        }

        if (type === "choice") {
          const options = (ctrl.options as Array<{ label: string; value: string }>) || [];
          const selected = String(ctrl.selected || "");

          return html`
            <div class="a2ui-control-panel__field">
              <label class="a2ui-control-panel__label">${label}</label>
              <div class="a2ui-control-panel__choices">
                ${options.map((opt) => html`
                  <label class="a2ui-control-panel__choice">
                    <input type="radio" name=${id} value=${opt.value}
                           ?checked=${opt.value === selected}
                           @change=${() => { if (onAction) onAction(id, { value: opt.value }); }} />
                    <span>${opt.label}</span>
                  </label>
                `)}
              </div>
            </div>
          `;
        }

        if (type === "button") {
          const variant = String(ctrl.variant || "primary");
          const confirmMsg = String(ctrl.confirmMessage || "");

          return html`
            <div class="a2ui-control-panel__field">
              <button class="a2ui-btn a2ui-btn--${variant}"
                      @click=${() => {
                        if (confirmMsg && !confirm(confirmMsg)) return;
                        if (onAction) onAction(id, {});
                      }}>
                ${label}
              </button>
            </div>
          `;
        }

        // Default: render as button
        return html`
          <div class="a2ui-control-panel__field">
            <button class="a2ui-btn a2ui-btn--primary"
                    @click=${() => { if (onAction) onAction(id, {}); }}>
              ${label}
            </button>
          </div>
        `;
      })}
    </div>
  `;
}
```

- [ ] **Step 2: Add ControlPanel CSS**

```css
.a2ui-control-panel {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px;
  background: var(--card);
}

.a2ui-control-panel__header {
  font-weight: 600;
  font-size: 14px;
  margin-bottom: 12px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border);
}

.a2ui-control-panel__field {
  margin-bottom: 12px;
}

.a2ui-control-panel__label {
  display: block;
  font-size: 12px;
  font-weight: 600;
  color: var(--muted);
  margin-bottom: 4px;
}

.a2ui-control-panel__slider-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.a2ui-control-panel__slider {
  flex: 1;
  accent-color: var(--accent, #3b82f6);
}

.a2ui-control-panel__range-label {
  font-size: 11px;
  color: var(--muted);
  min-width: 24px;
}

.a2ui-control-panel__choices {
  display: flex;
  gap: 12px;
}

.a2ui-control-panel__choice {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 13px;
  cursor: pointer;
}
```

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/chat/a2ui/catalog/control-panel.ts web/src/styles/components.css
git commit -m "feat(a2ui): implement ControlPanel with slider/choice/button controls"
```

---

## Phase 2B: onAction Wiring

### Task 11: Wire onAction into A2uiRendererEngine

**Files:**
- Modify: `web/src/ui/views/chat.ts`
- Modify: `web/src/ui/controllers/chat.ts`

- [ ] **Step 1: Pass onAction callback in chat.ts**

In `chat.ts`, change line 28 from:
```typescript
private a2uiRenderer = new A2uiRendererEngine();
```
to:
```typescript
private a2uiRenderer = new A2uiRendererEngine((functionId: string, data: Record<string, unknown>) => {
  this._handleA2uiAction(functionId, data);
});
```

- [ ] **Step 2: Add _handleA2uiAction method to ChatView**

```typescript
private _handleA2uiAction(functionId: string, data: Record<string, unknown>): void {
  const actionMsg = `[操作] ${functionId}: ${JSON.stringify(data)}`;
  sendChatMessage(this.chatState, actionMsg);
  this._startStreamPolling();
}
```

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/views/chat.ts
git commit -m "feat(chat): wire onAction callback for A2UI IoT component interactions"
```

---

## Phase 3: Chat Page Integration

### Task 12: Inline A2UI surfaces into message bubbles

**Files:**
- Modify: `web/src/ui/chat/message-normalizer.ts` (add a2uiSurfaceId)
- Modify: `web/src/ui/controllers/chat.ts` (associate surface with final message)
- Modify: `web/src/ui/chat/grouped-render.ts` (render A2UI inline)
- Modify: `web/src/ui/views/chat.ts` (pass a2uiRenderer to render functions)

- [ ] **Step 1: Add a2uiSurfaceId to NormalizedMessage**

In `message-normalizer.ts`, add field:
```typescript
export type NormalizedMessage = {
  role: string;
  content: NormalizedContentItem[];
  timestamp: number;
  id?: string;
  senderLabel?: string | null;
  a2uiSurfaceId?: string;  // NEW: associated A2UI surface
};
```

No change to `normalizeMessage()` — the field is set post-normalization.

- [ ] **Step 2: Associate A2UI surfaces with messages in chat.ts**

In `handleChatEvent` (chat.ts), when `payload.a2ui` arrives, track the latest surface ID. When the `final` event comes for the same run, attach it to the newly appended message.

Add to `ChatState`:
```typescript
lastA2uiSurfaceId?: string;
```

In `handleChatEvent`, after `state.onA2ui(payload.a2ui)` (line ~221):
```typescript
// Track the latest surface ID created during this run
// The a2ui callback in chat.ts view handles this via the renderer
```

In the chat view's `_bindA2uiCallback`, after `handleA2uiMessage`, track the last surface:
```typescript
this.chatState.onA2ui = (jsonl: string) => {
  this.a2uiRenderer.handleA2uiMessage(jsonl);
  // Track last surface ID for message association
  const surfaceIds = this.a2uiRenderer.getSurfaceIds();
  if (surfaceIds.length) {
    this.chatState.lastA2uiSurfaceId = surfaceIds[surfaceIds.length - 1];
  }
  this.requestUpdate();
};
```

Add `getSurfaceIds()` to `A2uiRendererEngine`:
```typescript
getSurfaceIds(): string[] {
  return Array.from(this.surfaces.keys());
}
```

- [ ] **Step 3: Update grouped-render.ts to render A2UI inline**

In `renderSingleMessage`, after rendering text content, check for `msg.a2uiSurfaceId`:

```typescript
// In renderSingleMessage function signature, add a2uiRenderer param:
function renderSingleMessage(
  msg: NormalizedMessage,
  isTool: boolean,
  a2uiRenderer?: A2uiRendererEngine,
): TemplateResult {
  // ... existing text rendering ...

  // After text content, before timestamp:
  ${msg.a2uiSurfaceId && a2uiRenderer
    ? a2uiRenderer.renderSurface(msg.a2uiSurfaceId)
    : nothing}
}
```

Update `renderMessageGroup` to accept and pass `a2uiRenderer`:
```typescript
export function renderMessageGroup(
  group: MessageGroup,
  a2uiRenderer?: A2uiRendererEngine,
): TemplateResult {
  // ... pass to renderSingleMessage ...
}
```

- [ ] **Step 4: Update chat.ts to pass a2uiRenderer to renderMessageGroup**

In `chat.ts` render method, change:
```typescript
${groups.map((g) => renderMessageGroup(g, this.a2uiRenderer))}
```

Also remove the standalone `${this.a2uiRenderer.renderAllSurfaces()}` at line 165.

- [ ] **Step 5: Associate surface ID on final message**

In chat.ts, after streaming ends and the final message is appended, attach the surface ID. Track this via a helper in the view:

```typescript
// In _startStreamPolling or after sendChatMessage resolves:
// When stream ends, check if lastA2uiSurfaceId exists and attach to last assistant message
private _attachLastSurfaceToMessage(): void {
  const surfaceId = this.chatState.lastA2uiSurfaceId;
  if (!surfaceId) return;
  const msgs = this.chatState.chatMessages;
  for (let i = msgs.length - 1; i >= 0; i--) {
    if (msgs[i].role === "assistant") {
      (msgs[i] as any).a2uiSurfaceId = surfaceId;
      break;
    }
  }
  this.chatState.lastA2uiSurfaceId = undefined;
}
```

Call this in `_stopStreamPolling` when `chatSending` becomes false.

- [ ] **Step 6: Build check**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npx tsc --noEmit 2>&1 | head -30`
Expected: No errors

- [ ] **Step 7: Commit**

```bash
git add web/src/ui/chat/message-normalizer.ts web/src/ui/controllers/chat.ts web/src/ui/chat/grouped-render.ts web/src/ui/views/chat.ts web/src/ui/chat/a2ui/a2ui-renderer.ts
git commit -m "feat(chat): inline A2UI surfaces into assistant message bubbles"
```

### Task 13: Session sidebar improvements

**Files:**
- Modify: `web/src/ui/views/chat.ts`

- [ ] **Step 1: Add auto-title from first user message**

In `handleSend()`, after sending, update the session label:
```typescript
private handleSend(): void {
  const msg = this.draft.trim();
  if (!msg) return;
  this.draft = "";
  sendChatMessage(this.chatState, msg);
  this.requestUpdate();
  this._startStreamPolling();

  // Auto-title: use first 20 chars of first user message
  const sessionIdx = this.sessionsList.findIndex((s) => s.key === this.sessionKey);
  if (sessionIdx >= 0 && this.sessionsList[sessionIdx].label === "新会话") {
    const title = msg.length > 20 ? msg.slice(0, 20) + "..." : msg;
    const updated = [...this.sessionsList];
    updated[sessionIdx] = { ...updated[sessionIdx], label: title };
    this.sessionsList = updated;
  }
}
```

- [ ] **Step 2: Add sidebar collapse toggle**

Add a collapse button at the top of the sidebar:
```typescript
<button class="chat-sidebar-toggle"
        @click=${() => { this.sidebarCollapsed = !this.sidebarCollapsed; }}>
  ${this.sidebarCollapsed ? "▶" : "◀"}
</button>
```

And when collapsed, hide the session list:
```typescript
<div class="chat-sidebar ${this.sidebarCollapsed ? "collapsed" : ""}">
  ${!this.sidebarCollapsed ? html`
    <button class="chat-new-session-btn" @click=${this.handleNewSession}>新建会话</button>
    ${this.sessionsList.map(/* ... */)}
  ` : nothing}
</div>
```

- [ ] **Step 3: Add sidebar CSS**

```css
.chat-sidebar.collapsed {
  min-width: 0;
  width: 0;
  padding: 0;
  overflow: hidden;
  border-right: none;
}

.chat-sidebar-toggle {
  border: none;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
  padding: 4px 8px;
  font-size: 12px;
}
```

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/views/chat.ts web/src/styles/components.css
git commit -m "feat(chat): session auto-title, sidebar collapse toggle"
```

### Task 14: Input bar improvements

**Files:**
- Modify: `web/src/ui/views/chat.ts`

- [ ] **Step 1: Auto-resize textarea**

Add an input handler that auto-adjusts height:
```typescript
@input=${(e: Event) => {
  const ta = e.target as HTMLTextAreaElement;
  this.draft = ta.value;
  ta.style.height = "auto";
  ta.style.height = Math.min(ta.scrollHeight, 120) + "px";
}}
```

Add CSS:
```css
.chat-input {
  min-height: 36px;
  max-height: 120px;
  resize: none;
  overflow-y: auto;
}
```

- [ ] **Step 2: Commit**

```bash
git add web/src/ui/views/chat.ts web/src/styles/components.css
git commit -m "feat(chat): auto-resize input textarea"
```

---

## Self-Review

**Spec coverage check:**

| Spec Section | Task Coverage |
|-------------|---------------|
| 2.1 Tab layout | Task 5 (selector + tabs) |
| 2.2 Model config | Tasks 1-2 (dropdown + temperature + prompt + save) |
| 2.3 Tools tab | Tasks 3-4 (search + batch + danger) |
| 2.4 Placeholder tabs | Task 5 (unified placeholder) |
| 2.5 Agent selector | Task 5 (dropdown) |
| 3.1 Chat layout | Existing (no change) |
| 3.2 DeviceCard | Task 7 |
| 3.2 DeviceTable | Task 8 |
| 3.2 DataChart | Task 9 |
| 3.2 ControlPanel | Task 10 |
| 3.3 onAction wiring | Task 11 |
| 3.4 Inline A2UI | Task 12 |
| 3.5 Session sidebar | Task 13 |
| 3.6 Input bar | Task 14 |

**Placeholder scan:** No "TBD", "TODO", or "implement later" found in tasks.

**Type consistency:** `AgentConfig` already has `alternativeModels?: string[]` — used in Task 1. `temperature` and `systemPrompt` use `[key: string]: unknown` catch-all — works without type changes. `NormalizedMessage` gets `a2uiSurfaceId` added in Task 12.

**No new backend APIs needed.** All existing endpoints (`GET /agents/:id/config`, `PUT /agents/:id/config`, `GET /tools/catalog`) are sufficient.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-04-08-agent-chat-redesign.md`.

**Two execution options:**

**1. Subagent-Driven (recommended)** — Dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
