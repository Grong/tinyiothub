# web-lit Device Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement complete device management for web-lit: device list grid view, template-based create wizard, device detail enhancements, and monitoring tab.

**Architecture:** Lit web components with Shadow DOM. Service layer for API calls. Follow existing patterns in devices-page.ts and device-detail-page.ts.

**Tech Stack:** Lit 3.x, TypeScript, uPlot (charts), Shadow DOM

---

## File Structure

```
web-lit/src/
├── components/
│   ├── device-card.ts                    # Device card grid item
│   ├── tag-filter.ts                     # Tag selection dropdown
│   ├── create-device-wizard.ts          # Full-screen wizard modal
│   ├── template-card.ts                  # Template selection card
│   ├── template-preview.ts                # Template preview panel
│   ├── device-info-form.ts              # Device info form
│   ├── property-chart-dialog.ts          # Property history chart
│   ├── command-execute-dialog.ts         # Command execution dialog
│   └── monitoring/
│       ├── device-status-card.ts
│       ├── performance-metrics-card.ts
│       ├── performance-chart.ts
│       ├── performance-alerts.ts
│       └── trace-records.ts
├── services/
│   ├── tags.ts                          # Tag service
│   └── devices.ts                       # Already exists (modified)
├── pages/
│   ├── devices-page.ts                  # Enhanced list page
│   └── device-detail-page.ts            # Enhanced detail page
```

---

## Task 1: Add Maintenance Status Tab

**Files:**
- Modify: `web-lit/src/pages/devices-page.ts`

- [ ] **Step 1: Add maintenance to status filter**

Find the status filter options in devices-page.ts (around line 785) and add maintenance:

```typescript
// Current (line 792-797):
<select class="filter-select" .value=${this.status} @change=${this.handleStatusChange}>
  <option value="">全部状态</option>
  <option value="online">在线</option>
  <option value="offline">离线</option>
  <option value="error">错误</option>
</select>

// Change to:
<select class="filter-select" .value=${this.status} @change=${this.handleStatusChange}>
  <option value="">全部状态</option>
  <option value="online">在线</option>
  <option value="offline">离线</option>
  <option value="error">错误</option>
  <option value="maintenance">维护</option>
</select>
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/pages/devices-page.ts
git commit -m "feat(web-lit): add maintenance status filter option"
```

---

## Task 2: Create Tag Service

**Files:**
- Create: `web-lit/src/services/tags.ts`
- Test: None (no test framework)

- [ ] **Step 1: Create tag service**

```typescript
// web-lit/src/services/tags.ts
import { apiGet } from '../lib/api-client'

export interface Tag {
  id: string
  name: string
  color: string
}

export const tagApi = {
  getTags: (type: 'device' | 'alarm' = 'device') =>
    apiGet<Tag[]>(`tags?type=${type}`),
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/services/tags.ts
git commit -m "feat(web-lit): add tag service for device filtering"
```

---

## Task 3: Create Tag Filter Component

**Files:**
- Create: `web-lit/src/components/tag-filter.ts`
- Modify: `web-lit/src/pages/devices-page.ts` (add state and rendering)

- [ ] **Step 1: Create tag-filter component**

```typescript
// web-lit/src/components/tag-filter.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { tagApi, type Tag } from '../services/tags'

@customElement('tag-filter')
export class TagFilter extends LitElement {
  static styles = css`
    :host { display: inline-flex; }
    .filter-container {
      position: relative;
    }
    .filter-btn {
      display: flex;
      align-items: center;
      gap: 6px;
      padding: 8px 12px;
      background: var(--card);
      border: none;
      border-radius: var(--radius-md);
      color: var(--text);
      font-size: 13px;
      cursor: pointer;
      box-shadow: var(--glass-shadow-sm);
    }
    .filter-btn:hover { background: var(--bg-hover); }
    .filter-btn.active { background: var(--accent-subtle); color: var(--accent); }
    .dropdown {
      position: absolute;
      top: 100%;
      left: 0;
      margin-top: 4px;
      min-width: 200px;
      background: var(--card);
      border-radius: var(--radius-md);
      box-shadow: var(--shadow-lg);
      z-index: 100;
      padding: 8px;
    }
    .tag-item {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px;
      border-radius: var(--radius-sm);
      cursor: pointer;
    }
    .tag-item:hover { background: var(--bg-hover); }
    .tag-item.selected { background: var(--accent-subtle); }
    .tag-color {
      width: 8px;
      height: 8px;
      border-radius: 50%;
    }
    .tag-name { font-size: 13px; color: var(--text); }
    .tag-count {
      margin-left: auto;
      font-size: 11px;
      color: var(--muted);
    }
  `

  @property({ type: String }) value = ''
  @property({ type: String }) placeholder = '选择标签'
  @state() tags: Tag[] = []
  @state() open = false
  @state() loading = true

  async connectedCallback() {
    super.connectedCallback()
    await this.loadTags()
  }

  async loadTags() {
    try {
      const response = await tagApi.getTags('device')
      if (response.result) {
        this.tags = response.result
      }
    } catch {
      this.tags = []
    } finally {
      this.loading = false
    }
  }

  toggleDropdown() { this.open = !this.open }

  selectTag(tag: Tag) {
    this.value = tag.id
    this.open = false
    this.dispatchEvent(new CustomEvent('change', { detail: tag.id }))
  }

  render() {
    const selectedTag = this.tags.find(t => t.id === this.value)
    return html`
      <div class="filter-container">
        <button class="filter-btn ${this.value ? 'active' : ''}" @click=${this.toggleDropdown}>
          <span>${selectedTag?.name || this.placeholder}</span>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M6 9l6 6 6-6"/>
          </svg>
        </button>
        ${this.open ? html`
          <div class="dropdown">
            ${this.tags.map(tag => html`
              <div class="tag-item ${tag.id === this.value ? 'selected' : ''}" @click=${() => this.selectTag(tag)}>
                <span class="tag-color" style="background: ${tag.color}"></span>
                <span class="tag-name">${tag.name}</span>
              </div>
            `)}
          </div>
        ` : ''}
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'tag-filter': TagFilter }
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/components/tag-filter.ts
git commit -m "feat(web-lit): add tag filter component"
```

---

## Task 4: Create Device Card Component

**Files:**
- Create: `web-lit/src/components/device-card.ts`

- [ ] **Step 1: Create device card component**

```typescript
// web-lit/src/components/device-card.ts
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { Device } from '../services/devices'

@customElement('device-card')
export class DeviceCard extends LitElement {
  static styles = css`
    :host { display: block; }
    .card {
      background: var(--card);
      border-radius: var(--radius-lg);
      box-shadow: var(--shadow-sm);
      overflow: hidden;
      cursor: pointer;
      transition: transform 0.15s ease, box-shadow 0.15s ease;
    }
    .card:hover {
      transform: translateY(-2px);
      box-shadow: var(--shadow-md);
    }
    .card-left-bar {
      position: absolute;
      left: 0;
      top: 0;
      bottom: 0;
      width: 4px;
    }
    .card-content { padding: 16px; position: relative; }
    .card-header {
      display: flex;
      justify-content: space-between;
      align-items: flex-start;
      margin-bottom: 8px;
    }
    .device-name {
      font-size: 14px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .protocol-badge {
      font-size: 10px;
      padding: 2px 6px;
      border-radius: var(--radius-sm);
      background: var(--bg-muted);
      color: var(--muted);
      text-transform: uppercase;
    }
    .device-address {
      font-size: 12px;
      color: var(--muted);
      font-family: var(--mono);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      margin-bottom: 12px;
    }
    .card-footer {
      display: flex;
      justify-content: space-between;
      align-items: center;
    }
    .status {
      display: flex;
      align-items: center;
      gap: 6px;
      font-size: 12px;
    }
    .status-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
    }
    .status-dot.online { background: var(--ok); }
    .status-dot.offline { background: var(--muted); }
    .status-dot.error { background: var(--danger); }
    .status-dot.maintenance { background: var(--warn); }
    .actions { display: flex; gap: 4px; }
    .action-btn {
      width: 28px;
      height: 28px;
      display: flex;
      align-items: center;
      justify-content: center;
      border: none;
      border-radius: var(--radius-sm);
      background: transparent;
      color: var(--muted);
      cursor: pointer;
    }
    .action-btn:hover { background: var(--bg-hover); color: var(--text); }
    .action-btn.danger:hover { background: var(--danger-subtle); color: var(--danger); }
  `

  @property({ type: Object }) device!: Device
  @property({ type: Function }) onEdit!: (d: Device) => void
  @property({ type: Function }) onDelete!: (d: Device) => void

  private get deviceTypeColor(): string {
    const type = this.device.protocol?.toLowerCase() || ''
    if (type.includes('modbus')) return 'var(--accent)'
    if (type.includes('onvif')) return 'var(--ok)'
    if (type.includes('snmp')) return 'var(--warn)'
    if (type.includes('mqtt')) return 'var(--info)'
    return 'var(--muted)'
  }

  private handleEdit(e: Event) {
    e.stopPropagation()
    this.onEdit(this.device)
  }

  private handleDelete(e: Event) {
    e.stopPropagation()
    this.onDelete(this.device)
  }

  private handleClick() {
    window.history.pushState({}, '', `/device-detail?id=${this.device.id}`)
    window.dispatchEvent(new PopStateEvent('popstate'))
  }

  render() {
    const status = this.device.status || 'offline'
    return html`
      <div class="card" @click=${this.handleClick}>
        <div class="card-left-bar" style="background: ${this.deviceTypeColor}"></div>
        <div class="card-content">
          <div class="card-header">
            <h3 class="device-name">${this.device.name}</h3>
            ${this.device.protocol ? html`<span class="protocol-badge">${this.device.protocol}</span>` : ''}
          </div>
          ${this.device.address ? html`<div class="device-address">${this.device.address}</div>` : ''}
          <div class="card-footer">
            <div class="status">
              <span class="status-dot ${status}"></span>
              <span>${status === 'online' ? '在线' : status === 'offline' ? '离线' : status === 'maintenance' ? '维护' : '错误'}</span>
            </div>
            <div class="actions">
              <button class="action-btn" title="编辑" @click=${this.handleEdit}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931z"/>
                </svg>
              </button>
              <button class="action-btn danger" title="删除" @click=${this.handleDelete}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0"/>
                </svg>
              </button>
            </div>
          </div>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'device-card': DeviceCard }
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/components/device-card.ts
git commit -m "feat(web-lit): add device card component for grid view"
```

---

## Task 5: Create Skeleton Component

**Files:**
- Create: `web-lit/src/components/skeleton.ts`

- [ ] **Step 1: Create skeleton component**

```typescript
// web-lit/src/components/skeleton.ts
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('skeleton')
export class Skeleton extends LitElement {
  static styles = css`
    :host { display: block; }
    .skeleton {
      background: linear-gradient(90deg, var(--card) 25%, var(--bg-hover) 50%, var(--card) 75%);
      background-size: 200% 100%;
      animation: skeleton-loading 1.5s infinite;
      border-radius: var(--radius-md);
    }
    @keyframes skeleton-loading {
      0% { background-position: 200% 0; }
      100% { background-position: -200% 0; }
    }
    .skeleton-text {
      height: 14px;
      margin-bottom: 8px;
    }
    .skeleton-title {
      height: 20px;
      width: 60%;
      margin-bottom: 12px;
    }
    .skeleton-card {
      height: 120px;
    }
  `

  @property({ type: String }) variant = 'text' // text, title, card

  render() {
    return html`<div class="skeleton skeleton-${this.variant}"></div>`
  }
}

declare global {
  interface HTMLElementTagNameMap { 'sk-skeleton': Skeleton }
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/components/skeleton.ts
git commit -m "feat(web-lit): add skeleton loading component"
```

---

## Task 6: Enhance Devices Page with Grid View and Filters

**Files:**
- Modify: `web-lit/src/pages/devices-page.ts`

- [ ] **Step 1: Add view toggle state and grid rendering**

Add to the state section (around line 520):
```typescript
@state() viewMode: 'grid' | 'table' = 'grid'
@state() tagId = ''
```

Add grid rendering method (around line 803 after renderDeviceList):
```typescript
private renderDeviceGrid() {
  if (this.devices.length === 0) {
    return this.renderEmptyGrid()
  }
  return html`
    <div class="device-grid">
      ${this.devices.map(device => html`
        <device-card
          .device=${device}
          .onEdit=${(d: Device) => this.openEditModal(d)}
          .onDelete=${(d: Device) => this.deleteDevice(d)}
        ></device-card>
      `)}
    </div>
  `
}

private renderEmptyGrid() {
  return html`
    <div class="device-grid">
      <div class="empty-card" @click=${() => this.openCreateModal()}>
        <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15"/>
        </svg>
        <p>暂无设备</p>
        <span>点击添加设备</span>
      </div>
    </div>
  `
}
```

Add CSS for grid (around line 100):
```css
.device-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 16px;
}

.empty-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 160px;
  background: var(--card);
  border-radius: var(--radius-lg);
  border: 2px dashed var(--border);
  cursor: pointer;
  color: var(--muted);
  transition: border-color 0.15s ease, color 0.15s ease;
}
.empty-card:hover {
  border-color: var(--accent);
  color: var(--accent);
}
.empty-card svg { margin-bottom: 8px; opacity: 0.5; }
.empty-card p { margin: 0; font-size: 14px; }
.empty-card span { font-size: 12px; }

@media (max-width: 768px) {
  .device-grid {
    grid-template-columns: 1fr;
  }
}
```

Add view toggle button in header-actions (around line 768):
```typescript
<button class="view-toggle" @click=${() => { this.viewMode = this.viewMode === 'grid' ? 'table' : 'grid' }}>
  ${this.viewMode === 'grid' ? html`
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <path d="M3 10h18M3 14h18M3 6h18M3 18h18"/>
    </svg>
  ` : html`
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/>
      <rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/>
    </svg>
  `}
</button>
```

Modify render() method to use grid when viewMode === 'grid'.

- [ ] **Step 2: Add tag filter and isCreatedByMe to filters section**

Add tag filter dropdown and checkbox in filters section (around line 777).

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/pages/devices-page.ts
git commit -m "feat(web-lit): add grid view, tag filter, and view toggle"
```

---

## Task 7: Create Template Card Component

**Files:**
- Create: `web-lit/src/components/template-card.ts`

- [ ] **Step 1: Create template card**

```typescript
// web-lit/src/components/template-card.ts
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { ProcessedDeviceTemplate } from '../services/templates'

@customElement('template-card')
export class TemplateCard extends LitElement {
  static styles = css`
    :host { display: block; cursor: pointer; }
    .card {
      background: var(--card);
      border-radius: var(--radius-lg);
      padding: 16px;
      border: 2px solid transparent;
      transition: border-color 0.15s ease, transform 0.15s ease;
    }
    .card:hover {
      border-color: var(--accent);
      transform: translateY(-2px);
    }
    .card-header {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 8px;
    }
    .category-icon {
      width: 40px;
      height: 40px;
      display: flex;
      align-items: center;
      justify-content: center;
      border-radius: var(--radius-md);
      font-size: 20px;
      background: var(--bg-muted);
    }
    .template-name {
      font-size: 14px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0;
    }
    .template-meta {
      font-size: 12px;
      color: var(--muted);
    }
    .template-description {
      font-size: 13px;
      color: var(--text);
      margin: 8px 0;
      display: -webkit-box;
      -webkit-line-clamp: 2;
      -webkit-box-orient: vertical;
      overflow: hidden;
    }
    .template-tags {
      display: flex;
      flex-wrap: wrap;
      gap: 4px;
      margin-top: 8px;
    }
    .tag {
      font-size: 10px;
      padding: 2px 6px;
      border-radius: var(--radius-sm);
      background: var(--bg-muted);
      color: var(--muted);
    }
  `

  @property({ type: Object }) template!: ProcessedDeviceTemplate
  @property({ type: Function }) onUse!: (t: ProcessedDeviceTemplate) => void

  private getCategoryIcon(): string {
    const icons: Record<string, string> = {
      sensors: '🌡️',
      controllers: '🎛️',
      cameras: '📷',
      gateways: '🌐',
      default: '📦',
    }
    return icons[this.template.category] || icons.default
  }

  private handleClick() {
    this.onUse(this.template)
  }

  render() {
    const t = this.template
    return html`
      <div class="card" @click=${this.handleClick}>
        <div class="card-header">
          <div class="category-icon">${this.getCategoryIcon()}</div>
          <div>
            <h3 class="template-name">${typeof t.displayName === 'object' ? t.displayName['zh'] || t.displayName['en'] || t.name : t.displayName || t.name}</h3>
            <div class="template-meta">${t.manufacturer || ''} ${t.deviceType || ''}</div>
          </div>
        </div>
        ${t.description ? html`<p class="template-description">${typeof t.description === 'object' ? Object.values(t.description)[0] : t.description}</p>` : ''}
        <div class="template-tags">
          ${t.driverName ? html`<span class="tag">${t.driverName}</span>` : ''}
          ${t.protocolType ? html`<span class="tag">${t.protocolType}</span>` : ''}
          ${t.version ? html`<span class="tag">v${t.version}</span>` : ''}
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'template-card': TemplateCard }
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/components/template-card.ts
git commit -m "feat(web-lit): add template card component"
```

---

## Task 8: Create Template Preview Component

**Files:**
- Create: `web-lit/src/components/template-preview.ts`

- [ ] **Step 1: Create template preview**

```typescript
// web-lit/src/components/template-preview.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import type { ProcessedDeviceTemplate } from '../services/templates'

@customElement('template-preview')
export class TemplatePreview extends LitElement {
  static styles = css`
    :host { display: flex; flex-direction: column; height: 100%; }
    .tabs {
      display: flex;
      border-bottom: 1px solid var(--border);
      padding: 0 16px;
    }
    .tab {
      padding: 12px 16px;
      font-size: 13px;
      color: var(--muted);
      cursor: pointer;
      border-bottom: 2px solid transparent;
      margin-bottom: -1px;
    }
    .tab.active {
      color: var(--accent);
      border-bottom-color: var(--accent);
    }
    .content { flex: 1; overflow-y: auto; padding: 16px; }
    .property-item, .command-item {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 10px 12px;
      background: var(--bg);
      border-radius: var(--radius-md);
      margin-bottom: 8px;
    }
    .property-name, .command-name {
      font-size: 13px;
      font-weight: 500;
      color: var(--text);
    }
    .property-meta, .command-meta {
      display: flex;
      gap: 8px;
      align-items: center;
    }
    .badge {
      font-size: 10px;
      padding: 2px 6px;
      border-radius: var(--radius-sm);
      background: var(--bg-muted);
      color: var(--muted);
    }
    .badge.readonly { background: var(--warn-subtle); color: var(--warn); }
    .badge.writable { background: var(--ok-subtle); color: var(--ok); }
  `

  @property({ type: Object }) template!: ProcessedDeviceTemplate
  @state() activeTab: 'properties' | 'commands' = 'properties'

  render() {
    return html`
      <div class="tabs">
        <div class="tab ${this.activeTab === 'properties' ? 'active' : ''}" @click=${() => this.activeTab = 'properties'}>
          属性 (${this.template.properties?.length || 0})
        </div>
        <div class="tab ${this.activeTab === 'commands' ? 'active' : ''}" @click=${() => this.activeTab = 'commands'}>
          命令 (${this.template.commands?.length || 0})
        </div>
      </div>
      <div class="content">
        ${this.activeTab === 'properties' ? this.renderProperties() : this.renderCommands()}
      </div>
    `
  }

  private renderProperties() {
    const props = this.template.properties || []
    if (props.length === 0) {
      return html`<div class="empty">此模板没有定义属性</div>`
    }
    return props.map(p => html`
      <div class="property-item">
        <span class="property-name">${typeof p.displayName === 'object' ? p.displayName['zh'] || p.name : p.displayName || p.name}</span>
        <div class="property-meta">
          <span class="badge">${p.dataType}</span>
          ${p.unit ? html`<span class="badge">${p.unit}</span>` : ''}
          <span class="badge ${p.isReadOnly ? 'readonly' : 'writable'}">${p.isReadOnly ? '只读' : '可写'}</span>
        </div>
      </div>
    `)
  }

  private renderCommands() {
    const cmds = this.template.commands || []
    if (cmds.length === 0) {
      return html`<div class="empty">此模板没有定义命令</div>`
    }
    return cmds.map(c => html`
      <div class="command-item">
        <span class="command-name">${typeof c.displayName === 'object' ? c.displayName['zh'] || c.name : c.displayName || c.name}</span>
        <div class="command-meta">
          ${c.isRequired ? html`<span class="badge readonly">必需</span>` : ''}
        </div>
      </div>
    `)
  }
}

declare global {
  interface HTMLElementTagNameMap { 'template-preview': TemplatePreview }
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/components/template-preview.ts
git commit -m "feat(web-lit): add template preview component"
```

---

## Task 9: Create Device Info Form Component

**Files:**
- Create: `web-lit/src/components/device-info-form.ts`

- [ ] **Step 1: Create device info form**

```typescript
// web-lit/src/components/device-info-form.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { driverApi, type Driver, type DriverConfigOption } from '../services/drivers'
import type { ProcessedDeviceTemplate } from '../services/templates'

@customElement('device-info-form')
export class DeviceInfoForm extends LitElement {
  static styles = css`
    :host { display: block; }
    .form-group { margin-bottom: 16px; }
    .form-label {
      display: block;
      font-size: 13px;
      font-weight: 500;
      color: var(--text);
      margin-bottom: 6px;
    }
    .form-label .required { color: var(--danger); margin-left: 2px; }
    .form-input, .form-select, .form-textarea {
      width: 100%;
      padding: 10px 12px;
      background: var(--card);
      border: none;
      border-bottom: 1px solid var(--input);
      border-radius: var(--radius-md) var(--radius-md) 0 0;
      color: var(--text);
      font-size: 14px;
    }
    .form-input:focus, .form-select:focus, .form-textarea:focus {
      outline: none;
      border-bottom-color: var(--accent);
    }
    .form-textarea { resize: vertical; min-height: 80px; }
    .form-error {
      font-size: 12px;
      color: var(--danger);
      margin-top: 4px;
    }
    .form-hint {
      font-size: 12px;
      color: var(--muted);
      margin-top: 4px;
    }
  `

  @property({ type: Object }) template!: ProcessedDeviceTemplate
  @property({ type: String }) value = ''
  @state() drivers: Driver[] = []
  @state() driverConfig: DriverConfigOption[] = []
  @state() errors: Record<string, string> = {}

  private get _formData() {
    try { return JSON.parse(this.value) } catch { return {} }
  }

  async connectedCallback() {
    super.connectedCallback()
    await this.loadDrivers()
  }

  async loadDrivers() {
    try {
      const res = await driverApi.getDrivers()
      if (res.result) this.drivers = res.result
    } catch { this.drivers = [] }
  }

  async loadDriverConfig(driverName: string) {
    if (!driverName) { this.driverConfig = []; return }
    try {
      const res = await driverApi.getDriverConfig(driverName)
      if (res.result) this.driverConfig = res.result
    } catch { this.driverConfig = [] }
  }

  private handleInput(field: string, value: string) {
    const data = { ...this._formData, [field]: value }
    if (field === 'driverName') {
      this.loadDriverConfig(value)
      data.driverOptions = '{}'
    }
    this.value = JSON.stringify(data)
    this.dispatchEvent(new CustomEvent('change', { detail: data }))
  }

  private handleDriverOption(name: string, optValue: string) {
    const data = this._formData
    const opts = JSON.parse(data.driverOptions || '{}')
    opts[name] = optValue
    data.driverOptions = JSON.stringify(opts)
    this.value = JSON.stringify(data)
    this.dispatchEvent(new CustomEvent('change', { detail: data }))
  }

  render() {
    const d = this._formData
    return html`
      <div class="form-group">
        <label class="form-label">设备名称 <span class="required">*</span></label>
        <input type="text" class="form-input" .value=${d.name || ''} @input=${(e: InputEvent) => this.handleInput('name', (e.target as HTMLInputElement).value)} />
        ${this.errors.name ? html`<span class="form-error">${this.errors.name}</span>` : ''}
      </div>

      <div class="form-group">
        <label class="form-label">描述</label>
        <textarea class="form-textarea" .value=${d.description || ''} @input=${(e: InputEvent) => this.handleInput('description', (e.target as HTMLTextAreaElement).value)}></textarea>
      </div>

      <div class="form-group">
        <label class="form-label">设备地址</label>
        <input type="text" class="form-input" .value=${d.address || ''} @input=${(e: InputEvent) => this.handleInput('address', (e.target as HTMLInputElement).value)} />
      </div>

      <div class="form-group">
        <label class="form-label">安装位置</label>
        <input type="text" class="form-input" .value=${d.position || ''} @input=${(e: InputEvent) => this.handleInput('position', (e.target as HTMLInputElement).value)} />
      </div>

      <div class="form-group">
        <label class="form-label">驱动</label>
        <select class="form-select" .value=${d.driverName || ''} @change=${(e: Event) => this.handleInput('driverName', (e.target as HTMLSelectElement).value)}>
          <option value="">选择驱动</option>
          ${this.drivers.map(dr => html`<option value=${dr.name}>${dr.name}</option>`)}
        </select>
      </div>

      ${this.driverConfig.length > 0 ? html`
        <div class="form-group">
          <label class="form-label">驱动配置</label>
          ${this.driverConfig.map(opt => html`
            <div style="margin-bottom: 12px;">
              <label class="form-label">
                ${opt.label} ${opt.required ? html`<span class="required">*</span>` : ''}
              </label>
              ${opt.type === 'boolean' ? html`
                <select class="form-select" @change=${(e: Event) => this.handleDriverOption(opt.name, (e.target as HTMLSelectElement).value)}>
                  <option value="true">是</option>
                  <option value="false">否</option>
                </select>
              ` : html`
                <input type=${opt.type === 'number' ? 'number' : 'text'} class="form-input"
                  .value=${JSON.parse(d.driverOptions || '{}')[opt.name] || opt.defaultValue || ''}
                  @input=${(e: InputEvent) => this.handleDriverOption(opt.name, (e.target as HTMLInputElement).value)} />
              `}
              ${opt.description ? html`<span class="form-hint">${opt.description}</span>` : ''}
            </div>
          `)}
        </div>
      ` : ''}
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'device-info-form': DeviceInfoForm }
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/components/device-info-form.ts
git commit -m "feat(web-lit): add device info form component"
```

---

## Task 10: Create Create Device Wizard

**Files:**
- Create: `web-lit/src/components/create-device-wizard.ts`

- [ ] **Step 1: Create create device wizard**

```typescript
// web-lit/src/components/create-device-wizard.ts
import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { templateApi, transformDeviceTemplate, type ProcessedDeviceTemplate } from '../services/templates'
import { deviceApi } from '../services/devices'
import './template-card'
import './template-preview'
import './device-info-form'

type WizardStep = 'template' | 'device'

@customElement('create-device-wizard')
export class CreateDeviceWizard extends LitElement {
  static styles = css`
    :host { display: block; }
    .overlay {
      position: fixed;
      inset: 0;
      z-index: 1000;
      background: rgba(0, 0, 0, 0.6);
      backdrop-filter: blur(4px);
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .modal {
      background: var(--bg);
      width: 95vw;
      max-width: 1200px;
      height: 85vh;
      border-radius: var(--radius-lg);
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }
    .header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 20px 24px;
      border-bottom: 1px solid var(--border);
    }
    .header h2 {
      font-size: 18px;
      font-weight: 600;
      margin: 0;
    }
    .close-btn {
      width: 32px;
      height: 32px;
      display: flex;
      align-items: center;
      justify-content: center;
      border: none;
      border-radius: var(--radius-md);
      background: transparent;
      color: var(--muted);
      cursor: pointer;
    }
    .close-btn:hover { background: var(--bg-hover); }
    .body { flex: 1; display: flex; overflow: hidden; }
    .step-indicator {
      display: flex;
      gap: 8px;
      margin-left: 24px;
    }
    .step-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--muted);
    }
    .step-dot.active { background: var(--accent); }
    .content { flex: 1; overflow-y: auto; padding: 24px; }
    .search-bar {
      max-width: 400px;
      margin-bottom: 24px;
    }
    .search-input {
      width: 100%;
      padding: 10px 14px;
      background: var(--card);
      border: none;
      border-radius: var(--radius-md);
      color: var(--text);
      font-size: 14px;
    }
    .category-tabs {
      display: flex;
      gap: 8px;
      margin-bottom: 24px;
    }
    .category-tab {
      padding: 6px 12px;
      border-radius: var(--radius-md);
      font-size: 13px;
      background: var(--card);
      color: var(--muted);
      cursor: pointer;
      border: none;
    }
    .category-tab.active {
      background: var(--accent);
      color: white;
    }
    .template-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
      gap: 16px;
    }
    .device-step {
      display: flex;
      height: 100%;
    }
    .form-area {
      flex: 1;
      padding: 24px;
      overflow-y: auto;
    }
    .preview-area {
      width: 400px;
      border-left: 1px solid var(--border);
      background: var(--card);
    }
    .footer {
      display: flex;
      justify-content: flex-end;
      gap: 12px;
      padding: 16px 24px;
      border-top: 1px solid var(--border);
    }
    .btn {
      padding: 10px 20px;
      border-radius: var(--radius-md);
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      border: none;
    }
    .btn-secondary { background: var(--bg-secondary); color: var(--text); }
    .btn-primary { background: var(--accent); color: white; }
    .btn-primary:disabled { opacity: 0.6; cursor: not-allowed; }
    .toast {
      position: fixed;
      bottom: 24px;
      left: 50%;
      transform: translateX(-50%);
      padding: 12px 24px;
      background: var(--card);
      border-radius: var(--radius-md);
      box-shadow: var(--shadow-lg);
      z-index: 2000;
    }
    .toast.success { border-left: 4px solid var(--ok); }
    .toast.error { border-left: 4px solid var(--danger); }
  `

  @state() open = false
  @state() step: WizardStep = 'template'
  @state() templates: ProcessedDeviceTemplate[] = []
  @state() filteredTemplates: ProcessedDeviceTemplate[] = []
  @state() selectedTemplate: ProcessedDeviceTemplate | null = null
  @state() searchQuery = ''
  @state() category = ''
  @state() formData = '{}'
  @state() loading = false
  @state() creating = false
  @state() toast = ''

  async show() {
    this.open = true
    this.step = 'template'
    this.selectedTemplate = null
    this.formData = '{}'
    await this.loadTemplates()
  }

  hide() {
    this.open = false
  }

  async loadTemplates() {
    this.loading = true
    try {
      const res = await templateApi.getTemplates()
      if (res.result) {
        this.templates = res.result.map(transformDeviceTemplate)
        this.filterTemplates()
      }
    } finally {
      this.loading = false
    }
  }

  filterTemplates() {
    let filtered = this.templates
    if (this.searchQuery) {
      const q = this.searchQuery.toLowerCase()
      filtered = filtered.filter(t =>
        t.name.toLowerCase().includes(q) ||
        (t.displayName as any)?.['zh']?.toLowerCase().includes(q)
      )
    }
    if (this.category) {
      filtered = filtered.filter(t => t.category === this.category)
    }
    this.filteredTemplates = filtered
  }

  selectTemplate(t: ProcessedDeviceTemplate) {
    this.selectedTemplate = t
    this.step = 'device'
  }

  handleFormChange(e: CustomEvent) {
    this.formData = JSON.stringify(e.detail)
  }

  async handleCreate() {
    this.creating = true
    try {
      const data = JSON.parse(this.formData)
      const driverOptions = data.driverOptions ? JSON.parse(data.driverOptions) : {}
      await deviceApi.createDevice({
        name: data.name,
        displayName: data.name,
        description: data.description,
        address: data.address,
        position: data.position,
        driverName: data.driverName,
        driverOptions: Object.keys(driverOptions).length > 0 ? JSON.stringify(driverOptions) : undefined,
        propertyValues: {},
        enabledCommands: this.selectedTemplate?.commands?.map(c => c.name) || [],
      })
      this.showToast('设备创建成功', 'success')
      this.hide()
      this.dispatchEvent(new CustomEvent('success'))
    } catch (err: any) {
      this.showToast(err.message || '创建失败', 'error')
    } finally {
      this.creating = false
    }
  }

  showToast(message: string, type: 'success' | 'error') {
    this.toast = `${type}:${message}`
    setTimeout(() => { this.toast = '' }, 3000)
  }

  render() {
    if (!this.open) return html``
    return html`
      <div class="overlay" @click=${() => this.hide()}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="header">
            <div style="display: flex; align-items: center; gap: 16px;">
              <h2>创建设备</h2>
              <div class="step-indicator">
                <div class="step-dot ${this.step === 'template' ? 'active' : ''}"></div>
                <div class="step-dot ${this.step === 'device' ? 'active' : ''}"></div>
              </div>
            </div>
            <button class="close-btn" @click=${() => this.hide()}>
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 6L6 18M6 6l12 12"/>
              </svg>
            </button>
          </div>

          <div class="body">
            ${this.step === 'template' ? this.renderTemplateStep() : this.renderDeviceStep()}
          </div>

          ${this.step === 'device' ? html`
            <div class="footer">
              <button class="btn btn-secondary" @click=${() => this.step = 'template'}>上一步</button>
              <button class="btn btn-primary" ?disabled=${this.creating} @click=${this.handleCreate}>
                ${this.creating ? '创建中...' : '创建'}
              </button>
            </div>
          ` : ''}
        </div>
      </div>
      ${this.toast ? html`<div class="toast ${this.toast.split(':')[0]}">${this.toast.split(':')[1]}</div>` : ''}
    `
  }

  private renderTemplateStep() {
    const categories = ['', 'sensors', 'controllers', 'cameras', 'gateways', 'others']
    const labels: Record<string, string> = { '': '全部', sensors: '传感器', controllers: '控制器', cameras: '摄像头', gateways: '网关', others: '其他' }
    return html`
      <div class="content">
        <div class="search-bar">
          <input type="text" class="search-input" placeholder="搜索模板..."
            .value=${this.searchQuery} @input=${(e: InputEvent) => { this.searchQuery = (e.target as HTMLInputElement).value; this.filterTemplates() }} />
        </div>
        <div class="category-tabs">
          ${categories.map(c => html`
            <button class="category-tab ${this.category === c ? 'active' : ''}" @click=${() => { this.category = c; this.filterTemplates() }}>
              ${labels[c]}
            </button>
          `)}
        </div>
        <div class="template-grid">
          ${this.filteredTemplates.map(t => html`
            <template-card .template=${t} .onUse=${(tmpl: ProcessedDeviceTemplate) => this.selectTemplate(tmpl)}></template-card>
          `)}
        </div>
      </div>
    `
  }

  private renderDeviceStep() {
    if (!this.selectedTemplate) return html``
    return html`
      <div class="device-step">
        <div class="form-area">
          <device-info-form
            .template=${this.selectedTemplate}
            .value=${this.formData}
            @change=${this.handleFormChange}
          ></device-info-form>
        </div>
        <div class="preview-area">
          <template-preview .template=${this.selectedTemplate}></template-preview>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'create-device-wizard': CreateDeviceWizard }
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/components/create-device-wizard.ts
git commit -m "feat(web-lit): add create device wizard with template selection"
```

---

## Task 11: Create Property Chart Dialog

**Files:**
- Create: `web-lit/src/components/property-chart-dialog.ts`

- [ ] **Step 1: Create property chart dialog**

```typescript
// web-lit/src/components/property-chart-dialog.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi } from '../services/devices'
import type { DeviceProperty, PerformanceHistory } from '../services/devices'

@customElement('property-chart-dialog')
export class PropertyChartDialog extends LitElement {
  static styles = css`
    :host { display: block; }
    .overlay {
      position: fixed;
      inset: 0;
      z-index: 1000;
      background: rgba(0, 0, 0, 0.6);
      backdrop-filter: blur(4px);
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .dialog {
      background: var(--bg);
      width: 90vw;
      max-width: 800px;
      max-height: 80vh;
      border-radius: var(--radius-lg);
      display: flex;
      flex-direction: column;
    }
    .header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 16px 20px;
      border-bottom: 1px solid var(--border);
    }
    .header h3 { margin: 0; font-size: 16px; }
    .close-btn {
      width: 32px; height: 32px;
      display: flex; align-items: center; justify-content: center;
      border: none; border-radius: var(--radius-md);
      background: transparent; color: var(--muted); cursor: pointer;
    }
    .body { flex: 1; overflow-y: auto; padding: 20px; }
    .time-range {
      display: flex;
      gap: 8px;
      margin-bottom: 16px;
    }
    .time-btn {
      padding: 6px 12px;
      border: none;
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 12px;
      cursor: pointer;
    }
    .time-btn.active { background: var(--accent); color: white; }
    .chart-container {
      height: 300px;
      background: var(--card);
      border-radius: var(--radius-md);
      padding: 16px;
    }
    .chart-svg { width: 100%; height: 100%; }
    .no-data {
      display: flex;
      align-items: center;
      justify-content: center;
      height: 200px;
      color: var(--muted);
    }
  `

  @property({ type: Boolean }) open = false
  @property({ type: Object }) property!: DeviceProperty
  @property({ type: String }) deviceId = ''
  @state() timeRange = 1 // hours
  @state() data: PerformanceHistory | null = null
  @state() loading = true

  async updated(changedProperties: Map<string, any>) {
    if (changedProperties.has('open') && this.open) {
      await this.loadData()
    }
  }

  async loadData() {
    this.loading = true
    try {
      const res = await deviceApi.getDevicePerformance(this.deviceId, this.timeRange)
      if (res.result) {
        this.data = res.result
      }
    } finally {
      this.loading = false
    }
  }

  private setTimeRange(hours: number) {
    this.timeRange = hours
    this.loadData()
  }

  private close() {
    this.open = false
    this.dispatchEvent(new CustomEvent('close'))
  }

  render() {
    if (!this.open) return html``
    return html`
      <div class="overlay" @click=${() => this.close()}>
        <div class="dialog" @click=${(e: Event) => e.stopPropagation()}>
          <div class="header">
            <h3>属性历史: ${this.property?.displayName || this.property?.name}</h3>
            <button class="close-btn" @click=${() => this.close()}>
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 6L6 18M6 6l12 12"/>
              </svg>
            </button>
          </div>
          <div class="body">
            <div class="time-range">
              ${[1, 6, 24, 168, 720].map(h => html`
                <button class="time-btn ${this.timeRange === h ? 'active' : ''}"
                  @click=${() => this.setTimeRange(h)}>
                  ${h === 1 ? '1小时' : h === 6 ? '6小时' : h === 24 ? '24小时' : h === 168 ? '7天' : '30天'}
                </button>
              `)}
            </div>
            ${this.loading ? html`<div class="no-data">加载中...</div>` :
              this.data?.data?.length ? this.renderChart() : html`<div class="no-data">暂无历史数据</div>`
            }
          </div>
        </div>
      </div>
    `
  }

  private renderChart() {
    const points = this.data?.data || []
    if (points.length < 2) return html`<div class="no-data">数据点不足</div>`
    const width = 700
    const height = 250
    const padding = 30
    const max = Math.max(...points.map(p => p.value))
    const min = Math.min(...points.map(p => p.value))
    const range = max - min || 1
    const coords = points.map((p, i) => {
      const x = padding + (i / (points.length - 1)) * (width - padding * 2)
      const y = height - padding - ((p.value - min) / range) * (height - padding * 2)
      return `${x},${y}`
    }).join(' ')
    return html`
      <div class="chart-container">
        <svg class="chart-svg" viewBox="0 0 ${width} ${height}">
          <polyline
            points=${coords}
            fill="none"
            stroke="var(--accent)"
            stroke-width="2"
          />
          ${points.map((p, i) => {
            const x = padding + (i / (points.length - 1)) * (width - padding * 2)
            const y = height - padding - ((p.value - min) / range) * (height - padding * 2)
            return html`<circle cx=${x} cy=${y} r="3" fill="var(--accent)"/>`
          })}
        </svg>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'property-chart-dialog': PropertyChartDialog }
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/components/property-chart-dialog.ts
git commit -m "feat(web-lit): add property chart dialog"
```

---

## Task 12: Create Command Execute Dialog

**Files:**
- Create: `web-lit/src/components/command-execute-dialog.ts`

- [ ] **Step 1: Create command execute dialog**

```typescript
// web-lit/src/components/command-execute-dialog.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi } from '../services/devices'
import type { DeviceCommand } from '../services/devices'

@customElement('command-execute-dialog')
export class CommandExecuteDialog extends LitElement {
  static styles = css`
    :host { display: block; }
    .overlay {
      position: fixed;
      inset: 0;
      z-index: 1000;
      background: rgba(0, 0, 0, 0.6);
      backdrop-filter: blur(4px);
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .dialog {
      background: var(--bg);
      width: 90vw;
      max-width: 500px;
      border-radius: var(--radius-lg);
    }
    .header {
      display: flex;
      justify-content: space-between;
      padding: 16px 20px;
      border-bottom: 1px solid var(--border);
    }
    .header h3 { margin: 0; font-size: 16px; }
    .close-btn {
      width: 32px; height: 32px;
      display: flex; align-items: center; justify-content: center;
      border: none; border-radius: var(--radius-md);
      background: transparent; color: var(--muted); cursor: pointer;
    }
    .body { padding: 20px; }
    .command-info {
      background: var(--card);
      padding: 12px;
      border-radius: var(--radius-md);
      margin-bottom: 16px;
    }
    .command-name { font-weight: 600; margin-bottom: 4px; }
    .command-desc { font-size: 12px; color: var(--muted); }
    .param-group { margin-bottom: 12px; }
    .param-label { display: block; font-size: 13px; font-weight: 500; margin-bottom: 4px; }
    .param-input {
      width: 100%;
      padding: 8px 12px;
      background: var(--card);
      border: none;
      border-bottom: 1px solid var(--input);
      color: var(--text);
      font-size: 14px;
    }
    .footer {
      display: flex;
      justify-content: flex-end;
      gap: 12px;
      padding: 16px 20px;
      border-top: 1px solid var(--border);
    }
    .btn {
      padding: 8px 16px;
      border-radius: var(--radius-md);
      font-size: 14px;
      cursor: pointer;
      border: none;
    }
    .btn-secondary { background: var(--bg-secondary); color: var(--text); }
    .btn-primary { background: var(--accent); color: white; }
    .btn-primary:disabled { opacity: 0.6; }
    .toast {
      position: fixed;
      bottom: 24px;
      left: 50%;
      transform: translateX(-50%);
      padding: 12px 24px;
      background: var(--card);
      border-radius: var(--radius-md);
      z-index: 2000;
    }
    .toast.success { border-left: 4px solid var(--ok); }
    .toast.error { border-left: 4px solid var(--danger); }
  `

  @property({ type: Boolean }) open = false
  @property({ type: Object }) command!: DeviceCommand
  @property({ type: String }) deviceId = ''
  @state() params: Record<string, any> = {}
  @state() executing = false
  @state() toast = ''

  updated(changedProperties: Map<string, any>) {
    if (changedProperties.has('open') && this.open) {
      // Initialize params with default values
      const defaults: Record<string, any> = {}
      if (this.command?.parameters) {
        Object.entries(this.command.parameters).forEach(([k, v]) => { defaults[k] = v })
      }
      this.params = defaults
    }
  }

  private close() {
    this.open = false
    this.dispatchEvent(new CustomEvent('close'))
  }

  private handleParamChange(key: string, value: any) {
    this.params = { ...this.params, [key]: value }
  }

  private async execute() {
    this.executing = true
    try {
      await deviceApi.executeCommand(this.deviceId, this.command.id, this.params)
      this.showToast('命令已发送', 'success')
      this.close()
      this.dispatchEvent(new CustomEvent('success'))
    } catch (err: any) {
      this.showToast(err.message || '执行失败', 'error')
    } finally {
      this.executing = false
    }
  }

  private showToast(message: string, type: 'success' | 'error') {
    this.toast = `${type}:${message}`
    setTimeout(() => { this.toast = '' }, 3000)
  }

  render() {
    if (!this.open) return html``
    const paramEntries = Object.entries(this.command?.parameters || {})
    return html`
      <div class="overlay" @click=${() => this.close()}>
        <div class="dialog" @click=${(e: Event) => e.stopPropagation()}>
          <div class="header">
            <h3>执行指令</h3>
            <button class="close-btn" @click=${() => this.close()}>
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 6L6 18M6 6l12 12"/>
              </svg>
            </button>
          </div>
          <div class="body">
            <div class="command-info">
              <div class="command-name">${this.command?.name}</div>
              <div class="command-desc">${this.command?.description || '无描述'}</div>
            </div>
            ${paramEntries.length > 0 ? paramEntries.map(([key, defaultValue]) => html`
              <div class="param-group">
                <label class="param-label">${key}</label>
                <input type=${typeof defaultValue === 'number' ? 'number' : 'text'}
                  class="param-input"
                  .value=${this.params[key] ?? defaultValue ?? ''}
                  @input=${(e: InputEvent) => this.handleParamChange(key, (e.target as HTMLInputElement).value)}
                />
              </div>
            `) : html`<p style="color: var(--muted); font-size: 13px;">此指令无需参数</p>`}
          </div>
          <div class="footer">
            <button class="btn btn-secondary" @click=${() => this.close()}>取消</button>
            <button class="btn btn-primary" ?disabled=${this.executing} @click=${this.execute}>
              ${this.executing ? '执行中...' : '确认执行'}
            </button>
          </div>
        </div>
      </div>
      ${this.toast ? html`<div class="toast ${this.toast.split(':')[0]}">${this.toast.split(':')[1]}</div>` : ''}
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'command-execute-dialog': CommandExecuteDialog }
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/components/command-execute-dialog.ts
git commit -m "feat(web-lit): add command execute dialog"
```

---

## Task 13: Enhance Device Detail Page

**Files:**
- Modify: `web-lit/src/pages/device-detail-page.ts`

- [ ] **Step 1: Add auto-refresh and new dialogs**

Add to state section:
```typescript
@state() refreshInterval: number | null = null
@state() showCommandDialog = false
@state() selectedCommand: DeviceCommand | null = null
@state() showPropertyChart = false
@state() selectedProperty: DeviceProperty | null = null
```

Add in connectedCallback:
```typescript
this.refreshInterval = window.setInterval(() => {
  if (this.deviceId) this.loadDevice(this.deviceId)
}, 3000)
```

Add in disconnectedCallback:
```typescript
if (this.refreshInterval) clearInterval(this.refreshInterval)
```

Add refresh button in header-actions section (around line 934):
```typescript
<button class="btn" @click=${() => this.loadDevice(this.deviceId)}>
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M23 4v6h-6M1 20v-6h6"/>
    <path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/>
  </svg>
  刷新
</button>
```

Add command execute trigger in render() commands section (around line 1033):
```typescript
<button class="command-btn" @click=${() => { this.selectedCommand = cmd; this.showCommandDialog = true }}>
  执行
</button>
```

Add mini chart button in properties list (around line 1009):
```typescript
${this.isNumericProperty(prop) ? html`
  <button class="chart-btn" @click=${() => { this.selectedProperty = prop; this.showPropertyChart = true }} title="查看曲线">
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <path d="M3 3v18h18"/>
      <path d="M18 17l-5-5-3 3-4-4"/>
    </svg>
  </button>
` : ''}
```

Add mini chart SVG in properties list item rendering:
```typescript
private renderMiniChart(property: DeviceProperty) {
  const data = [30, 45, 35, 50, 40, 55, 45]
  const points = data.map((v, i) => `${i * 10},${20 - v / 3}`).join(' ')
  return html`<svg width="60" height="20" class="mini-chart"><polyline points=${points} fill="none" stroke="var(--accent)" stroke-width="1.5"/></svg>`
}
```

- [ ] **Step 2: Add CSS for chart button**

Add CSS (around line 405):
```css
.chart-btn {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
  margin-left: 8px;
}
.chart-btn:hover { background: var(--bg-hover); color: var(--accent); }
```

Add monitoring tab (around line 306):
```typescript
{ id: 'monitoring', name: '监控' },
```

And add rendering for monitoring tab content with placeholder.

- [ ] **Step 3: Add command-execute-dialog and property-chart-dialog to render**

In render() method, add before closing template:
```typescript
${this.showCommandDialog ? html`
  <command-execute-dialog
    .open=${this.showCommandDialog}
    .command=${this.selectedCommand}
    .deviceId=${this.deviceId}
    @close=${() => this.showCommandDialog = false}
    @success=${() => this.loadDevice(this.deviceId)}
  ></command-execute-dialog>
` : ''}

${this.showPropertyChart ? html`
  <property-chart-dialog
    .open=${this.showPropertyChart}
    .property=${this.selectedProperty}
    .deviceId=${this.deviceId}
    @close=${() => this.showPropertyChart = false}
  ></property-chart-dialog>
` : ''}
```

- [ ] **Step 4: Commit**

```bash
git add web-lit/src/pages/device-detail-page.ts
git commit -m "feat(web-lit): enhance device detail with auto-refresh, charts, commands"
```

---

## Task 14: Create Monitoring Components

**Files:**
- Create: `web-lit/src/components/monitoring/device-status-card.ts`
- Create: `web-lit/src/components/monitoring/performance-metrics-card.ts`
- Create: `web-lit/src/components/monitoring/performance-alerts.ts`
- Create: `web-lit/src/components/monitoring/trace-records.ts`

- [ ] **Step 1: Create device status card**

```typescript
// web-lit/src/components/monitoring/device-status-card.ts
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { DeviceOnlineStatus, DeviceMetrics } from '../../services/devices'

@customElement('device-status-card')
export class DeviceStatusCard extends LitElement {
  static styles = css`
    :host { display: block; }
    .card {
      background: var(--card);
      border-radius: var(--radius-lg);
      padding: 16px;
    }
    .status-row {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 12px;
    }
    .status-icon {
      width: 40px;
      height: 40px;
      border-radius: var(--radius-md);
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .status-icon.online { background: var(--ok-subtle); color: var(--ok); }
    .status-icon.offline { background: var(--bg-muted); color: var(--muted); }
    .status-text { font-size: 16px; font-weight: 600; }
    .status-sub { font-size: 12px; color: var(--muted); }
    .metrics-row {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 12px;
    }
    .metric-item { text-align: center; }
    .metric-value { font-size: 18px; font-weight: 600; }
    .metric-label { font-size: 11px; color: var(--muted); }
  `

  @property({ type: Object }) status!: DeviceOnlineStatus
  @property({ type: Object }) metrics!: DeviceMetrics

  render() {
    return html`
      <div class="card">
        <div class="status-row">
          <div class="status-icon ${this.status?.is_online ? 'online' : 'offline'}">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              ${this.status?.is_online
                ? html`<path d="M5 12.55a11 11 0 0114.08 0M8.53 16.11a6 6 0 016.97 0M12 20h.01"/>`
                : html`<path d="M1 1l22 22M16.72 11.06A10.94 10.94 0 0119 12.55M5 12.55a11 11 0 015.71-2.63M12 20h.01"/>`
              }
            </svg>
          </div>
          <div>
            <div class="status-text">${this.status?.is_online ? '在线' : '离线'}</div>
            <div class="status-sub">最后检查: ${this.status?.last_check ? new Date(this.status.last_check).toLocaleTimeString() : '-'}</div>
          </div>
        </div>
        ${this.metrics ? html`
          <div class="metrics-row">
            <div class="metric-item">
              <div class="metric-value">${this.metrics.cpu_usage || 0}%</div>
              <div class="metric-label">CPU</div>
            </div>
            <div class="metric-item">
              <div class="metric-value">${this.metrics.memory_usage || 0}%</div>
              <div class="metric-label">内存</div>
            </div>
            <div class="metric-item">
              <div class="metric-value">${this.metrics.temperature || '-'}${this.metrics.temperature ? '°C' : ''}</div>
              <div class="metric-label">温度</div>
            </div>
          </div>
        ` : ''}
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'device-status-card': DeviceStatusCard }
}
```

- [ ] **Step 2: Create performance alerts component**

```typescript
// web-lit/src/components/monitoring/performance-alerts.ts
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { PerformanceAlert } from '../../services/devices'

@customElement('performance-alerts')
export class PerformanceAlerts extends LitElement {
  static styles = css`
    :host { display: block; }
    .alert-item {
      display: flex;
      align-items: flex-start;
      gap: 12px;
      padding: 12px;
      background: var(--card);
      border-radius: var(--radius-md);
      margin-bottom: 8px;
    }
    .alert-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      margin-top: 6px;
      flex-shrink: 0;
    }
    .alert-dot.info { background: var(--info); }
    .alert-dot.warning { background: var(--warn); }
    .alert-dot.error { background: var(--danger); }
    .alert-dot.critical { background: var(--danger); box-shadow: 0 0 6px var(--danger); }
    .alert-content { flex: 1; }
    .alert-message { font-size: 13px; margin-bottom: 4px; }
    .alert-meta { font-size: 11px; color: var(--muted); }
    .empty { text-align: center; padding: 32px; color: var(--muted); }
  `

  @property({ type: Array }) alerts: PerformanceAlert[] = []

  render() {
    if (!this.alerts?.length) {
      return html`<div class="empty">暂无告警</div>`
    }
    return html`
      ${this.alerts.map(alert => html`
        <div class="alert-item">
          <div class="alert-dot ${alert.level}"></div>
          <div class="alert-content">
            <div class="alert-message">${alert.message}</div>
            <div class="alert-meta">${new Date(alert.triggered_at).toLocaleString()} - ${alert.alert_type}</div>
          </div>
        </div>
      `)}
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'performance-alerts': PerformanceAlerts }
}
```

- [ ] **Step 3: Create trace records component**

```typescript
// web-lit/src/components/monitoring/trace-records.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi, type DeviceTrace } from '../../services/devices'

@customElement('trace-records')
export class TraceRecords extends LitElement {
  static styles = css`
    :host { display: block; }
    .trace-item {
      display: flex;
      align-items: flex-start;
      gap: 12px;
      padding: 10px 0;
      border-bottom: 1px solid var(--border);
    }
    .trace-level {
      font-size: 10px;
      padding: 2px 6px;
      border-radius: var(--radius-sm);
      text-transform: uppercase;
    }
    .trace-level.info { background: var(--info); color: white; }
    .trace-level.warning { background: var(--warn); color: black; }
    .trace-level.error { background: var(--danger); color: white; }
    .trace-content { flex: 1; }
    .trace-title { font-size: 13px; font-weight: 500; margin-bottom: 2px; }
    .trace-message { font-size: 12px; color: var(--muted); }
    .trace-time { font-size: 11px; color: var(--muted); white-space: nowrap; }
    .empty { text-align: center; padding: 32px; color: var(--muted); }
  `

  @property({ type: String }) deviceId = ''
  @state() traces: DeviceTrace[] = []
  @state() loading = true

  async connectedCallback() {
    super.connectedCallback()
    await this.loadTraces()
  }

  async loadTraces() {
    this.loading = true
    try {
      const res = await deviceApi.getDeviceTraces(this.deviceId, { limit: 50 })
      if (res.result) this.traces = res.result
    } finally {
      this.loading = false
    }
  }

  render() {
    if (this.loading) return html`<div class="empty">加载中...</div>`
    if (!this.traces?.length) return html`<div class="empty">暂无追踪记录</div>`
    return html`
      ${this.traces.map(t => html`
        <div class="trace-item">
          <span class="trace-level ${t.level}">${t.level}</span>
          <div class="trace-content">
            <div class="trace-title">${t.title}</div>
            <div class="trace-message">${t.message}</div>
          </div>
          <span class="trace-time">${new Date(t.created_at).toLocaleString()}</span>
        </div>
      `)}
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'trace-records': TraceRecords }
}
```

- [ ] **Step 4: Commit**

```bash
git add web-lit/src/components/monitoring/
git commit -m "feat(web-lit): add monitoring components (status card, alerts, traces)"
```

---

## Task 15: Install uPlot

**Files:**
- Modify: `web-lit/package.json`

- [ ] **Step 1: Add uPlot dependency**

```bash
cd web-lit && pnpm add uplot
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/package.json web-lit/pnpm-lock.yaml
git commit -m "chore(web-lit): add uPlot for performance charts"
```

---

## Task 16: Create Performance Chart Component with uPlot

**Files:**
- Create: `web-lit/src/components/monitoring/performance-chart.ts`

- [ ] **Step 1: Create performance chart with uPlot**

```typescript
// web-lit/src/components/monitoring/performance-chart.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi, type PerformanceHistory } from '../../services/devices'
import uPlot from 'uplot'
import 'uplot/dist/uPlot.min.css'

@customElement('performance-chart')
export class PerformanceChart extends LitElement {
  static styles = css`
    :host { display: block; }
    .chart-container {
      background: var(--card);
      border-radius: var(--radius-lg);
      padding: 16px;
    }
    .time-range {
      display: flex;
      gap: 8px;
      margin-bottom: 16px;
    }
    .time-btn {
      padding: 6px 12px;
      border: none;
      border-radius: var(--radius-md);
      background: var(--bg-secondary);
      color: var(--text);
      font-size: 12px;
      cursor: pointer;
    }
    .time-btn.active { background: var(--accent); color: white; }
    .chart { width: 100%; height: 300px; }
    .no-data {
      display: flex;
      align-items: center;
      justify-content: center;
      height: 200px;
      color: var(--muted);
    }
  `

  @property({ type: String }) deviceId = ''
  @property({ type: Number }) refreshInterval = 10000
  @state() timeRange = 1
  @state() data: PerformanceHistory | null = null
  @state() loading = true
  private chart: uPlot | null = null
  private interval: number | null = null

  async connectedCallback() {
    super.connectedCallback()
    await this.loadData()
    this.interval = window.setInterval(() => this.loadData(), this.refreshInterval)
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    if (this.interval) clearInterval(this.interval)
    if (this.chart) this.chart.destroy()
  }

  async loadData() {
    const res = await deviceApi.getDevicePerformance(this.deviceId, this.timeRange)
    if (res.result) {
      this.data = res.result
      this.renderChart()
    }
    this.loading = false
  }

  private renderChart() {
    if (!this.data?.data?.length || !this.shadowRoot) return
    const d = this.data.data
    const timestamps = d.map(p => p.timestamp / 1000)
    const values = d.map(p => p.value)
    const opts: uPlot.Options = {
      width: this.shadowRoot.querySelector('.chart')?.clientWidth || 600,
      height: 280,
      series: [
        {},
        { label: this.data.metric, stroke: '#3b82f6', width: 2 },
      ],
    }
    if (this.chart) this.chart.destroy()
    this.chart = new uPlot(opts, [timestamps, values], this.shadowRoot.querySelector('.chart') as HTMLElement)
  }

  private setTimeRange(h: number) {
    this.timeRange = h
    this.loadData()
  }

  render() {
    return html`
      <div class="chart-container">
        <div class="time-range">
          ${[1, 6, 24, 168, 720].map(h => html`
            <button class="time-btn ${this.timeRange === h ? 'active' : ''}" @click=${() => this.setTimeRange(h)}>
              ${h === 1 ? '1小时' : h === 6 ? '6小时' : h === 24 ? '24小时' : h === 168 ? '7天' : '30天'}
            </button>
          `)}
        </div>
        ${this.loading ? html`<div class="no-data">加载中...</div>` : html`<div class="chart"></div>`}
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'performance-chart': PerformanceChart }
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/components/monitoring/performance-chart.ts
git commit -m "feat(web-lit): add uPlot performance chart component"
```

---

## Task 17: Update Design Doc Status

**Files:**
- Modify: `docs/superpowers/specs/2026-04-05-weblit-device-management-design.md`

- [ ] **Step 1: Mark design as implementing**

Update the status header:
```markdown
> 状态: 实现中
> 实现开始: 2026-04-05
```

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-05-weblit-device-management-design.md
git commit -m "docs: mark device management design as implementing"
```

---

## Summary

| Task | Files | Status |
|------|-------|--------|
| 1 | devices-page.ts (maintenance tab) | ⏳ |
| 2 | tags.ts | ⏳ |
| 3 | tag-filter.ts | ⏳ |
| 4 | device-card.ts | ⏳ |
| 5 | skeleton.ts | ⏳ |
| 6 | devices-page.ts (grid view) | ⏳ |
| 7 | template-card.ts | ⏳ |
| 8 | template-preview.ts | ⏳ |
| 9 | device-info-form.ts | ⏳ |
| 10 | create-device-wizard.ts | ⏳ |
| 11 | property-chart-dialog.ts | ⏳ |
| 12 | command-execute-dialog.ts | ⏳ |
| 13 | device-detail-page.ts | ⏳ |
| 14 | monitoring/*.ts | ⏳ |
| 15 | package.json (uPlot) | ⏳ |
| 16 | performance-chart.ts | ⏳ |
| 17 | design doc status | ⏳ |
