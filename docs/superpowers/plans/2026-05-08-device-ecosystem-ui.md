# Device Ecosystem v0.2 — Frontend UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Lit 3 frontend for Device Ecosystem v0.2: marketplace browsing/installation, template publishing, driver health dashboard, device export/clone.

**Architecture:** Add API client modules for marketplace and driver-health endpoints. Create two new Lit views (`view-marketplace`, `view-driver-health`) and extend existing `view-devices` and `view-templates` with new actions. Register routes in `app.ts`.

**Tech Stack:** Lit 3, TypeScript, Vite, nanostore (stores), custom CSS variables, `ApiClient` with automatic snake/camel case conversion.

---

## File Structure

### New API client modules

| File | Responsibility |
|------|----------------|
| `web/src/api/marketplace.ts` | Marketplace proxy APIs (templates, drivers, install, publish) |
| `web/src/api/driver-health.ts` | Driver health dashboard API |

### Modified API client modules

| File | Change |
|------|--------|
| `web/src/api/devices.ts` | Add `exportTemplate(id)` and `cloneDevice(id)` methods |

### New UI views

| File | Responsibility |
|------|----------------|
| `web/src/ui/views/marketplace.ts` | Browse marketplace templates/drivers, install, publish local templates |
| `web/src/ui/views/driver-health.ts` | Workspace driver health status table |

### Modified UI views

| File | Change |
|------|--------|
| `web/src/ui/views/devices.ts` | Add "Export Template" and "Clone" action buttons per row |
| `web/src/ui/views/templates.ts` | Add "Publish to Marketplace" button in detail modal |
| `web/src/ui/app.ts` | Register `marketplace` and `driver-health` lazy views + nav items |

---

## Task 1: Add marketplace API client

**Files:**
- Create: `web/src/api/marketplace.ts`

- [ ] **Step 1: Write the marketplace API module**

```typescript
/**
 * Marketplace API — proxy to external marketplace + local publish
 */

import { apiGet, apiPost } from './client.js';

export interface MarketplaceTemplate {
  id: string;
  name: string;
  version: string;
  description?: string;
  category?: string;
  author?: string;
  tags?: string[];
  deviceType?: string;
  protocolType?: string;
  driverName?: string;
  rating?: number;
  downloadCount?: number;
}

export interface MarketplaceDriver {
  id: string;
  name: string;
  version: string;
  description?: string;
  protocolType?: string;
  rating?: number;
  downloadCount?: number;
}

export const marketplaceApi = {
  async getTemplates(params?: { category?: string; search?: string; page?: number; pageSize?: number }) {
    return apiGet<{ data: MarketplaceTemplate[]; pagination: { page: number; pageSize: number; totalPages: number; totalCount: number } }>('/marketplace/templates', params as Record<string, any>);
  },

  async getTemplate(id: string) {
    return apiGet<MarketplaceTemplate>(`/marketplace/templates/${id}`);
  },

  async installTemplate(id: string, version?: string) {
    return apiPost<string>(`/marketplace/templates/${id}/install`, { version });
  },

  async getDrivers(params?: { protocolType?: string; search?: string; page?: number; pageSize?: number }) {
    return apiGet<{ data: MarketplaceDriver[]; pagination: { page: number; pageSize: number; totalPages: number; totalCount: number } }>('/marketplace/drivers', params as Record<string, any>);
  },

  async getDriver(id: string) {
    return apiGet<MarketplaceDriver>(`/marketplace/drivers/${id}`);
  },

  async installDriver(id: string, version?: string) {
    return apiPost<string>(`/marketplace/drivers/${id}/install`, { version });
  },

  async publishTemplate(templateId: string) {
    return apiPost<Record<string, unknown>>('/marketplace/publish/template', { templateId });
  },
};
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npx tsc --noEmit src/api/marketplace.ts`
Expected: PASS (no type errors)

- [ ] **Step 3: Commit**

```bash
git add web/src/api/marketplace.ts
git commit -m "feat(api): add marketplace API client"
```

---

## Task 2: Add driver-health API client

**Files:**
- Create: `web/src/api/driver-health.ts`

- [ ] **Step 1: Write the driver-health API module**

```typescript
/**
 * Driver Health Dashboard API
 */

import { apiGet } from './client.js';

export interface DriverHealthInfo {
  driverName: string;
  version: string;
  loadedAt: string;
  refCount: number;
  status: 'active' | 'error' | 'unloading';
}

export interface WorkspaceDriverHealth {
  workspaceId: string;
  drivers: DriverHealthInfo[];
}

export const driverHealthApi = {
  async getWorkspaceHealth() {
    return apiGet<WorkspaceDriverHealth>('/driver-health/drivers');
  },
};
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npx tsc --noEmit src/api/driver-health.ts`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add web/src/api/driver-health.ts
git commit -m "feat(api): add driver-health API client"
```

---

## Task 3: Extend devices API with export and clone

**Files:**
- Modify: `web/src/api/devices.ts`

- [ ] **Step 1: Add two new methods to `deviceApi`**

Add after the existing `createDeviceFromTemplate` method (before the closing `};`):

```typescript
  async exportDeviceAsTemplate(id: string) {
    return apiPost<{ templateId: string; name: string }>(`/devices/${id}/export-template`);
  },

  async cloneDevice(id: string) {
    return apiPost<Device>(`/devices/${id}/clone`);
  },
```

The full `deviceApi` object should now end with:

```typescript
  async createDeviceFromTemplate(data: { templateId: string; deviceInput: any }) {
    return apiPost<any>('/devices/from-template', data);
  },

  async exportDeviceAsTemplate(id: string) {
    return apiPost<{ templateId: string; name: string }>(`/devices/${id}/export-template`);
  },

  async cloneDevice(id: string) {
    return apiPost<Device>(`/devices/${id}/clone`);
  },
};
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npx tsc --noEmit src/api/devices.ts`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add web/src/api/devices.ts
git commit -m "feat(api): add device export-template and clone endpoints"
```

---

## Task 4: Create marketplace view

**Files:**
- Create: `web/src/ui/views/marketplace.ts`

- [ ] **Step 1: Write the marketplace view**

```typescript
import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { marketplaceApi, type MarketplaceTemplate, type MarketplaceDriver } from "../../api/marketplace.js";
import { templateApi } from "../../api/templates.js";
import { success, error as toastError } from "../components/toast.js";

type Tab = "templates" | "drivers";

@customElement("view-marketplace")
export class MarketplaceView extends LitElement {
  @state() activeTab: Tab = "templates";
  @state() loading = true;
  @state() templates: MarketplaceTemplate[] = [];
  @state() drivers: MarketplaceDriver[] = [];
  @state() searchKeyword = "";
  @state() installingId: string | null = null;
  @state() publishingId: string | null = null;
  @state() localTemplates: { id: string; name: string }[] = [];

  connectedCallback() {
    super.connectedCallback();
    this.loadTemplates();
    this.loadLocalTemplates();
  }

  async loadTemplates() {
    this.loading = true;
    try {
      const res = await marketplaceApi.getTemplates({ pageSize: 100 });
      this.templates = res.result?.data ?? [];
    } catch (e: any) {
      toastError(e.message || "加载市场模板失败");
    } finally {
      this.loading = false;
    }
  }

  async loadDrivers() {
    this.loading = true;
    try {
      const res = await marketplaceApi.getDrivers({ pageSize: 100 });
      this.drivers = res.result?.data ?? [];
    } catch (e: any) {
      toastError(e.message || "加载市场驱动失败");
    } finally {
      this.loading = false;
    }
  }

  async loadLocalTemplates() {
    try {
      const res = await templateApi.getTemplates({ pageSize: 100 });
      this.localTemplates = (res.result?.data ?? []).map((t: any) => ({ id: t.id, name: t.name }));
    } catch {
      // ignore — publish section just won't show templates
    }
  }

  async installTemplate(id: string) {
    this.installingId = id;
    try {
      await marketplaceApi.installTemplate(id);
      success("模板安装成功");
    } catch (e: any) {
      toastError(e.message || "安装失败");
    } finally {
      this.installingId = null;
    }
  }

  async installDriver(id: string) {
    this.installingId = id;
    try {
      await marketplaceApi.installDriver(id);
      success("驱动安装成功");
    } catch (e: any) {
      toastError(e.message || "安装失败");
    } finally {
      this.installingId = null;
    }
  }

  async publishTemplate(templateId: string) {
    this.publishingId = templateId;
    try {
      await marketplaceApi.publishTemplate(templateId);
      success("模板发布成功");
    } catch (e: any) {
      toastError(e.message || "发布失败");
    } finally {
      this.publishingId = null;
    }
  }

  switchTab(tab: Tab) {
    this.activeTab = tab;
    if (tab === "templates") this.loadTemplates();
    else this.loadDrivers();
  }

  private get filteredTemplates() {
    if (!this.searchKeyword) return this.templates;
    const kw = this.searchKeyword.toLowerCase();
    return this.templates.filter(
      (t) =>
        t.name?.toLowerCase().includes(kw) ||
        t.description?.toLowerCase().includes(kw) ||
        t.category?.toLowerCase().includes(kw)
    );
  }

  private get filteredDrivers() {
    if (!this.searchKeyword) return this.drivers;
    const kw = this.searchKeyword.toLowerCase();
    return this.drivers.filter(
      (d) => d.name?.toLowerCase().includes(kw) || d.description?.toLowerCase().includes(kw)
    );
  }

  render() {
    return html`
      <div class="page">
        <div class="page-header">
          <h1 class="page-title">应用市场</h1>
          <div class="tabs">
            <button
              class="tab ${this.activeTab === "templates" ? "tab--active" : ""}"
              @click=${() => this.switchTab("templates")}
            >
              模板
            </button>
            <button
              class="tab ${this.activeTab === "drivers" ? "tab--active" : ""}"
              @click=${() => this.switchTab("drivers")}
            >
              驱动
            </button>
          </div>
        </div>

        <div class="toolbar">
          <input
            class="input"
            type="text"
            placeholder="搜索..."
            .value=${this.searchKeyword}
            @input=${(e: InputEvent) => { this.searchKeyword = (e.target as HTMLInputElement).value; }}
          />
        </div>

        ${this.activeTab === "templates"
          ? this.renderTemplatesTab()
          : this.renderDriversTab()}

        ${this.localTemplates.length > 0 ? this.renderPublishSection() : nothing}
      </div>
    `;
  }

  renderTemplatesTab() {
    if (this.loading) return html`<div class="card">加载中...</div>`;
    const items = this.filteredTemplates;
    if (items.length === 0) return html`<div class="card empty-hint">暂无模板</div>`;
    return html`
      <div class="card-grid">
        ${items.map((t) => html`
          <div class="card card--hover">
            <div class="card__header">
              <h3 class="card__title">${t.name}</h3>
              <span class="badge">${t.version}</span>
            </div>
            <p class="card__meta">${t.category || "其他"} · ${t.deviceType || "-"}</p>
            <p class="card__desc">${t.description || "暂无描述"}</p>
            <div class="card__footer">
              <button
                class="btn btn--primary btn--sm"
                ?disabled=${this.installingId === t.id}
                @click=${() => this.installTemplate(t.id)}
              >
                ${this.installingId === t.id ? "安装中..." : "安装"}
              </button>
            </div>
          </div>
        `)}
      </div>
    `;
  }

  renderDriversTab() {
    if (this.loading) return html`<div class="card">加载中...</div>`;
    const items = this.filteredDrivers;
    if (items.length === 0) return html`<div class="card empty-hint">暂无驱动</div>`;
    return html`
      <div class="card-grid">
        ${items.map((d) => html`
          <div class="card card--hover">
            <div class="card__header">
              <h3 class="card__title">${d.name}</h3>
              <span class="badge">${d.version}</span>
            </div>
            <p class="card__meta">${d.protocolType || "-"}</p>
            <p class="card__desc">${d.description || "暂无描述"}</p>
            <div class="card__footer">
              <button
                class="btn btn--primary btn--sm"
                ?disabled=${this.installingId === d.id}
                @click=${() => this.installDriver(d.id)}
              >
                ${this.installingId === d.id ? "安装中..." : "安装"}
              </button>
            </div>
          </div>
        `)}
      </div>
    `;
  }

  renderPublishSection() {
    return html`
      <div class="card" style="margin-top: 24px;">
        <h3 class="card__title">发布本地模板到市场</h3>
        <div style="display: flex; gap: 12px; flex-wrap: wrap; margin-top: 12px;">
          ${this.localTemplates.map((t) => html`
            <button
              class="btn btn--secondary btn--sm"
              ?disabled=${this.publishingId === t.id}
              @click=${() => this.publishTemplate(t.id)}
            >
              ${this.publishingId === t.id ? "发布中..." : t.name}
            </button>
          `)}
        </div>
      </div>
    `;
  }

  static styles = [];
}
```

- [ ] **Step 2: Verify the view compiles**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npx tsc --noEmit src/ui/views/marketplace.ts`
Expected: PASS (ignore warnings about unused imports if any)

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/views/marketplace.ts
git commit -m "feat(ui): add marketplace view with browse, install, publish"
```

---

## Task 5: Create driver-health view

**Files:**
- Create: `web/src/ui/views/driver-health.ts`

- [ ] **Step 1: Write the driver-health view**

```typescript
import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import { driverHealthApi, type DriverHealthInfo } from "../../api/driver-health.js";
import { error as toastError } from "../components/toast.js";

@customElement("view-driver-health")
export class DriverHealthView extends LitElement {
  @state() loading = true;
  @state() health: DriverHealthInfo[] = [];
  @state() workspaceId = "";
  @state() error = "";

  connectedCallback() {
    super.connectedCallback();
    this.loadHealth();
  }

  async loadHealth() {
    this.loading = true;
    this.error = "";
    try {
      const res = await driverHealthApi.getWorkspaceHealth();
      this.workspaceId = res.result?.workspaceId ?? "";
      this.health = res.result?.drivers ?? [];
    } catch (e: any) {
      this.error = e.message || "加载健康状态失败";
      toastError(this.error);
    } finally {
      this.loading = false;
    }
  }

  statusColor(status: string): string {
    switch (status) {
      case "active": return "var(--success)";
      case "error": return "var(--danger)";
      case "unloading": return "var(--warning)";
      default: return "var(--muted)";
    }
  }

  statusLabel(status: string): string {
    switch (status) {
      case "active": return "正常";
      case "error": return "故障";
      case "unloading": return卸载中";
      default: return status;
    }
  }

  render() {
    return html`
      <div class="page">
        <div class="page-header">
          <h1 class="page-title">驱动健康</h1>
          <button class="btn btn--secondary btn--sm" @click=${() => this.loadHealth()}>
            刷新
          </button>
        </div>

        ${this.loading
          ? html`<div class="card">加载中...</div>`
          : this.error
            ? html`<div class="card card--error">${this.error}</div>`
            : this.renderTable()}
      </div>
    `;
  }

  renderTable() {
    if (this.health.length === 0) {
      return html`<div class="card empty-hint">当前工作空间没有加载的动态驱动</div>`;
    }
    return html`
      <div class="card">
        <table class="data-table">
          <thead>
            <tr>
              <th>驱动名称</th>
              <th>版本</th>
              <th>加载时间</th>
              <th>引用计数</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            ${this.health.map((h) => html`
              <tr>
                <td>${h.driverName}</td>
                <td>${h.version}</td>
                <td>${h.loadedAt}</td>
                <td>${h.refCount}</td>
                <td>
                  <span class="status-badge">
                    <span class="status-dot" style="background: ${this.statusColor(h.status)};"></span>
                    <span class="status-badge__label">${this.statusLabel(h.status)}</span>
                  </span>
                </td>
              </tr>
            `)}
          </tbody>
        </table>
      </div>
    `;
  }

  static styles = [];
}
```

- [ ] **Step 2: Verify the view compiles**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npx tsc --noEmit src/ui/views/driver-health.ts`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/views/driver-health.ts
git commit -m "feat(ui): add driver-health dashboard view"
```

---

## Task 6: Add export and clone actions to devices view

**Files:**
- Modify: `web/src/ui/views/devices.ts`

- [ ] **Step 1: Add action handler methods**

Find the `deleteDevice` method around line 819. After it, add two new methods:

```typescript
  async exportDeviceTemplate(d: Device) {
    if (!confirm(`将设备 "${d.name}" 导出为模板？`)) return;
    try {
      const res = await deviceApi.exportDeviceAsTemplate(d.id);
      success(`导出成功：模板 ID ${res.result?.templateId ?? ""}`);
    } catch (e: any) {
      toastError(e.message || "导出失败");
    }
  }

  async cloneDevice(d: Device) {
    if (!confirm(`克隆设备 "${d.name}"？`)) return;
    try {
      await deviceApi.cloneDevice(d.id);
      success("设备克隆成功");
      this.loadDevices();
    } catch (e: any) {
      toastError(e.message || "克隆失败");
    }
  }
```

- [ ] **Step 2: Add buttons to table row actions**

Find the table row actions around line 1196. Change:

```typescript
                  <td class="cell-actions">
                    <button class="btn btn--ghost btn--sm" @click=${() => this.navigateToDevice(d.id)}>详情</button>
                    <button class="btn btn--ghost btn--sm" @click=${() => this.openEdit(d)}>编辑</button>
                    <button class="btn btn--ghost btn--sm btn--danger-text" @click=${() => this.deleteDevice(d)}>删除</button>
                  </td>
```

To:

```typescript
                  <td class="cell-actions">
                    <button class="btn btn--ghost btn--sm" @click=${() => this.navigateToDevice(d.id)}>详情</button>
                    <button class="btn btn--ghost btn--sm" @click=${() => this.openEdit(d)}>编辑</button>
                    <button class="btn btn--ghost btn--sm" @click=${() => this.exportDeviceTemplate(d)}>导出模板</button>
                    <button class="btn btn--ghost btn--sm" @click=${() => this.cloneDevice(d)}>克隆</button>
                    <button class="btn btn--ghost btn--sm btn--danger-text" @click=${() => this.deleteDevice(d)}>删除</button>
                  </td>
```

- [ ] **Step 3: Add buttons to grid card actions (if applicable)**

Search for grid card delete button around line 1314. Also add export and clone buttons nearby if the grid view has action buttons. If the grid view only has a delete icon/button, skip this step.

- [ ] **Step 4: Verify TypeScript compiles**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npx tsc --noEmit src/ui/views/devices.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/views/devices.ts
git commit -m "feat(ui): add export-template and clone actions to device list"
```

---

## Task 7: Add publish button to templates view

**Files:**
- Modify: `web/src/ui/views/templates.ts`

- [ ] **Step 1: Import marketplace API**

Add to the imports at the top of `web/src/ui/views/templates.ts`:

```typescript
import { marketplaceApi } from "../../api/marketplace.js";
```

- [ ] **Step 2: Add publish state and handler**

Add to the class state declarations (around line 91, after existing `@state()` fields):

```typescript
  @state() publishing = false;
```

Add a publish handler method in the class body:

```typescript
  async publishToMarketplace(t: ProcessedTemplate) {
    this.publishing = true;
    try {
      await marketplaceApi.publishTemplate(t.id);
      success("模板已发布到市场");
    } catch (e: any) {
      toastError(e.message || "发布失败");
    } finally {
      this.publishing = false;
    }
  }
```

- [ ] **Step 3: Add publish button in detail modal**

Find the template detail modal or the selected template view. Look for where the template detail is rendered (search for `selectedTemplate` usage). Add a "Publish to Marketplace" button near the existing action buttons (like "Create Device" or "Edit").

If the templates view uses a detail panel (not a modal), add the button in that panel. For example, if there is a detail section like:

```typescript
${this.selectedTemplate ? html`
  <div class="detail-panel">
    ...
    <div class="detail-actions">
      <button class="btn btn--primary" @click=${() => this.createDeviceFromTemplate(this.selectedTemplate!)}>
        创建设备
      </button>
    </div>
  </div>
` : nothing}
```

Add the publish button inside `.detail-actions`:

```typescript
      <button
        class="btn btn--secondary"
        ?disabled=${this.publishing}
        @click=${() => this.publishToMarketplace(this.selectedTemplate!)}
      >
        ${this.publishing ? "发布中..." : "发布到市场"}
      </button>
```

If the exact location of the detail panel is different, find the buttons near `selectedTemplate` and add the publish button adjacent to them.

- [ ] **Step 4: Verify TypeScript compiles**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npx tsc --noEmit src/ui/views/templates.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/views/templates.ts
git commit -m "feat(ui): add publish-to-marketplace button in templates view"
```

---

## Task 8: Register routes and navigation

**Files:**
- Modify: `web/src/ui/app.ts`

- [ ] **Step 1: Add lazy view loaders**

Find `const lazyViews` around line 19. Add two new entries:

```typescript
  marketplace: () => import("./views/marketplace.js").then(() => {}),
  "driver-health": () => import("./views/driver-health.js").then(() => {}),
```

The full `lazyViews` should now include:

```typescript
const lazyViews: Record<string, () => Promise<void>> = {
  dashboard: () => import("./views/dashboard.js").then(() => {}),
  devices:   () => import("./views/devices.js").then(() => {}),
  alarms:    () => import("./views/alarms.js").then(() => {}),
  events:    () => import("./views/events.js").then(() => {}),
  monitoring:() => import("./views/monitoring.js").then(() => {}),
  templates: () => import("./views/templates.js").then(() => {}),
  drivers:   () => import("./views/drivers.js").then(() => {}),
  tags:      () => import("./views/tags.js").then(() => {}),
  users:     () => import("./views/users.js").then(() => {}),
  settings:  () => import("./views/settings.js").then(() => {}),
  chat:      () => import("./views/chat.js").then(() => {}),
  agents:    () => import("./views/agents.js").then(() => {}),
  cron:      () => import("./views/cron.js").then(() => {}),
  terms:     () => import("./views/terms.js").then(() => {}),
  privacy:   () => import("./views/privacy.js").then(() => {}),
  marketplace: () => import("./views/marketplace.js").then(() => {}),
  "driver-health": () => import("./views/driver-health.js").then(() => {}),
};
```

- [ ] **Step 2: Add nav items**

Find `NAV_GROUPS` around line 49. Add `marketplace` to the "设备管理" group after `drivers`, and add `driver-health` to the "监控告警" group after `monitoring`.

Change the "设备管理" group from:

```typescript
  {
    label: "设备管理",
    items: [
      { route: "devices", label: "设备列表", icon: "M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" },
      { route: "templates", label: "设备模板", icon: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" },
      { route: "drivers", label: "驱动管理", icon: "M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" },
    ],
  },
```

To:

```typescript
  {
    label: "设备管理",
    items: [
      { route: "devices", label: "设备列表", icon: "M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" },
      { route: "templates", label: "设备模板", icon: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" },
      { route: "drivers", label: "驱动管理", icon: "M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" },
      { route: "marketplace", label: "应用市场", icon: "M3 3h18v18H3V3zm4 4v10h4V7H7zm6 0v10h4V7h-4z" },
    ],
  },
```

Change the "监控告警" group from:

```typescript
  {
    label: "监控告警",
    items: [
      { route: "alarms", label: "告警中心", icon: "M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0zM12 9v4M12 17h.01" },
      { route: "events", label: "事件日志", icon: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" },
      { route: "monitoring", label: "系统监控", icon: "M18 20V10M12 20V4M6 20v-6" },
      { route: "cron", label: "定时任务", icon: "M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm0 18a8 8 0 1 1 8-8 8 8 0 0 1-8 8zm1-8h4v2H11V6h2z" },
    ],
  },
```

To:

```typescript
  {
    label: "监控告警",
    items: [
      { route: "alarms", label: "告警中心", icon: "M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0zM12 9v4M12 17h.01" },
      { route: "events", label: "事件日志", icon: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" },
      { route: "monitoring", label: "系统监控", icon: "M18 20V10M12 20V4M6 20v-6" },
      { route: "driver-health", label: "驱动健康", icon: "M22 12h-4l-3 9L9 3l-3 9H2" },
      { route: "cron", label: "定时任务", icon: "M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm0 18a8 8 0 1 1 8-8 8 8 0 0 1-8 8zm1-8h4v2H11V6h2z" },
    ],
  },
```

- [ ] **Step 3: Add route title mappings**

Find the `titles` object or `getPageTitle()` method around line 305. Add title entries for the new routes:

```typescript
    marketplace: "应用市场",
    "driver-health": "驱动健康",
```

Also add subtitle entries if there's a subtitles object:

```typescript
    marketplace: "浏览和安装模板与驱动",
    "driver-health": "查看已加载动态驱动的运行状态",
```

- [ ] **Step 4: Add route rendering in renderMain()**

Find the main content switch that renders views based on `currentRoute` (search for `this.currentRoute === "devices"` or similar). Add cases for the two new routes.

Look for a pattern like:

```typescript
    if (this.currentRoute === "devices") return html`<view-devices></view-devices>`;
```

Add before the fallback:

```typescript
    if (this.currentRoute === "marketplace") return html`<view-marketplace></view-marketplace>`;
    if (this.currentRoute === "driver-health") return html`<view-driver-health></view-driver-health>`;
```

If the routing uses a dynamic map or object lookup instead of if/else, add the new routes to that map.

- [ ] **Step 5: Verify TypeScript compiles**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npx tsc --noEmit src/ui/app.ts`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add web/src/ui/app.ts
git commit -m "feat(ui): register marketplace and driver-health routes and nav items"
```

---

## Task 9: Full build verification

**Files:** None (verification only)

- [ ] **Step 1: Run frontend TypeScript check**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npx tsc --noEmit`
Expected: PASS with no errors (warnings are OK)

- [ ] **Step 2: Run frontend build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && npm run build`
Expected: PASS — Vite bundles successfully

- [ ] **Step 3: Run backend tests**

Run: `cargo test -p tinyiothub-cloud`
Expected: All existing tests still pass

- [ ] **Step 4: Commit any final fixes**

If any fixes were needed, commit them:

```bash
git add -A
git commit -m "chore(ui): final build fixes for device ecosystem frontend"
```

---

## Self-Review Checklist

### 1. Spec coverage

| Backend Feature | Frontend Task |
|-----------------|---------------|
| Marketplace template browse/install | Task 1 (API) + Task 4 (UI) |
| Marketplace driver browse/install | Task 1 (API) + Task 4 (UI) |
| Template publish to marketplace | Task 1 (API) + Task 7 (UI) |
| Driver health dashboard | Task 2 (API) + Task 5 (UI) |
| Device export as template | Task 3 (API) + Task 6 (UI) |
| Device clone | Task 3 (API) + Task 6 (UI) |
| Route registration | Task 8 |

### 2. Placeholder scan

- No "TBD", "TODO", or "implement later" strings.
- Every task contains exact file paths.
- Every code step contains complete code.
- Every test step contains exact commands and expected output.

### 3. Type consistency

- `MarketplaceTemplate` and `MarketplaceDriver` interfaces in Task 1 match the proxy response shapes from `handler.rs`.
- `DriverHealthInfo` in Task 2 matches `types.rs` in the backend.
- `deviceApi.exportDeviceAsTemplate` and `deviceApi.cloneDevice` in Task 3 match the handler signatures in `management.rs`.
- Route names `marketplace` and `driver-health` in Task 8 are consistent with lazy view keys and nav item routes.

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-05-08-device-ecosystem-ui.md`.**

**Two execution options:**

**1. Subagent-Driven (recommended)** — Fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

**Which approach?**
