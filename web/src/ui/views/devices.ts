import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { SignalWatcher } from "@lit-labs/signals";
import { deviceApi } from "../../api/devices.js";
import { driverApi } from "../../api/drivers.js";
import { templateApi } from "../../api/templates.js";
import { tagApi } from "../../api/tags.js";
import { eventApi } from "../../api/events.js";
import { deviceCache } from "../../stores/device-cache.js";
import type { Device, DeviceProfile, DeviceProperty, CreateDeviceRequest, DriverConfigOption, Tag, DeviceEvent } from "../../types/index.js";
import { success, error as toastError } from "../components/toast.js";
import { icons } from "../icons.js";

// Template with parsed JSON fields (backend returns JSON-as-string)
interface ProcessedTemplate {
  id: string;
  name: string;
  displayName: Record<string, string>;
  description: Record<string, string> | null;
  category: string;
  version: string;
  manufacturer?: string;
  deviceType: string;
  protocolType?: string;
  driverName?: string;
  tags: string[];
  deviceInfo: DeviceInfo;
  properties: any[];
  commands: any[];
  isBuiltin: boolean;
}

interface DeviceInfo {
  defaultNamePattern: string;
  defaultDisplayNamePattern?: string;
  defaultDescription?: Record<string, string>;
  defaultPosition?: string;
  defaultDriverOptions?: string;
  requiredFields: string[];
}

function parseJsonField<T>(jsonString: any, fallback: T): T {
  if (!jsonString) return fallback;
  if (typeof jsonString !== "string") return jsonString;
  try {
    return JSON.parse(jsonString);
  } catch {
    return fallback;
  }
}

function transformTemplate(raw: any): ProcessedTemplate {
  return {
    id: raw.id,
    name: raw.name,
    displayName: parseJsonField(raw.displayName, {}),
    description: parseJsonField(raw.description, null),
    category: raw.category || "others",
    version: raw.version || "",
    manufacturer: raw.manufacturer,
    deviceType: raw.deviceType || "",
    protocolType: raw.protocolType,
    driverName: raw.driverName,
    tags: parseJsonField(raw.tags, []),
    deviceInfo: parseJsonField(raw.deviceInfo, { defaultNamePattern: raw.name, requiredFields: [] } as DeviceInfo),
    properties: parseJsonField(raw.properties, []),
    commands: parseJsonField(raw.commands, []),
    isBuiltin: raw.isBuiltin === 1 || raw.isBuiltin === true,
  };
}

function isFieldRequired(deviceInfo: DeviceInfo | undefined, fieldName: string): boolean {
  return deviceInfo?.requiredFields?.includes(fieldName) || false;
}

function getLocalizedText(obj: Record<string, string> | undefined, fallback: string): string {
  if (!obj || typeof obj !== "object") return fallback;
  return obj["zh"] || obj["en"] || Object.values(obj)[0] || fallback;
}

const CATEGORY_LABELS: Record<string, string> = {
  sensors: "传感器",
  controllers: "控制器",
  cameras: "摄像头",
  gateways: "网关",
  others: "其他",
};

const CATEGORY_ICONS: Record<string, string> = {
  sensors: "🌡️",
  controllers: "🎛️",
  cameras: "📷",
  gateways: "🌐",
  others: "📦",
};

type ViewMode = "table" | "grid";

@customElement("view-devices")
export class DevicesView extends SignalWatcher(LitElement) {
  @state() loading = true;
  @state() error = "";
  @state() devices: Device[] = [];
  @state() total = 0;
  @state() totalPages = 0;
  @state() page = 1;
  @state() pageSize = 20;
  @state() searchName = "";
  @state() selectedDevice: DeviceProfile | null = null;
  @state() detailLoading = false;
  @state() detailTab: string = "properties";

  // View mode
  @state() viewMode: ViewMode = "grid";

  // Filters
  @state() filterStatus = "";
  @state() filterProtocol = "";

  // Create/Edit modal
  @state() showModal = false;
  @state() editingDevice: Device | null = null;
  @state() saving = false;
  @state() formName = "";
  @state() formType = "";
  @state() formAddress = "";
  @state() formDescription = "";
  @state() formManufacturer = "";
  @state() formModel = "";
  @state() formProtocol = "";

  // Wizard (2-step template-based)
  @state() showWizard = false;
  @state() wizardStep: "template" | "device" = "template";
  @state() wizardSaving = false;
  @state() wizTemplates: ProcessedTemplate[] = [];
  @state() wizTemplateLoading = false;
  @state() wizTemplateSearch = "";
  @state() wizSelectedTemplate: ProcessedTemplate | null = null;
  @state() wizName = "";
  @state() wizDescription = "";
  @state() wizAddress = "";
  @state() wizPosition = "";
  @state() wizDriver = "";
  @state() wizDriverConfig: Record<string, string> = {};
  @state() wizConfigOptions: DriverConfigOption[] = [];
  @state() wizConfigLoading = false;
  @state() wizValidationErrors: Record<string, string> = {};
  @state() driverNames: string[] = [];

  // Command execution
  @state() executingCommand = "";

  // Tags
  @state() allTags: Tag[] = [];
  @state() editingTagsDeviceId: string | null = null;
  @state() tagSearchKeyword = "";
  @state() tagSaving = false;
  private _boundCloseTagEditor = () => { this.editingTagsDeviceId = null; };
  private _boundHandleDeviceUpdated = (e: Event) => {
    const { deviceId } = (e as CustomEvent).detail as { deviceId: string; eventType: string; data: any };
    console.log('[DevicesView] device-updated event:', deviceId, 'selectedDevice:', this.selectedDevice?.device?.id);
    if (this.selectedDevice?.device?.id === deviceId) {
      console.log('[DevicesView] Refreshing detail page');
      this.loadDeviceDetail(deviceId);
    }
  };

  // Property history dialog
  @state() showHistoryDialog = false;
  @state() historyPropertyName = "";
  @state() historyPropertyUnit = "";
  @state() historyLoading = false;
  @state() historyData: { time: string; value: number }[] = [];
  @state() historyRange: string = "1h";
  @state() historyCustomStart = "";
  @state() historyCustomEnd = "";
  private historyDeviceId = "";

  createRenderRoot() {
    return this;
  }

  updated(changedProperties: Map<string, unknown>) {
    super.updated(changedProperties);
    if (this.showHistoryDialog && !this.historyLoading && this.historyData.length > 0) {
      requestAnimationFrame(() => this.drawHistoryChart());
    }
  }

  connectedCallback() {
    super.connectedCallback();
    const path = window.location.pathname;
    if (path.startsWith("/devices/")) {
      const id = path.split("/")[2];
      if (id) {
        this.loadDeviceDetail(id);
        document.addEventListener("click", this._boundCloseTagEditor);
        document.addEventListener("device-updated", this._boundHandleDeviceUpdated);
        return;
      }
    }
    // 从缓存加载设备（首次触发 fetch + SSE 自动连接）
    this.loadDevicesFromCache();
    this.loadDriverNames();
    this.loadAllTags();
    document.addEventListener("click", this._boundCloseTagEditor);
    document.addEventListener("device-updated", this._boundHandleDeviceUpdated);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    // 不断开 SSE — 缓存层管理连接生命周期
    document.removeEventListener("click", this._boundCloseTagEditor);
    document.removeEventListener("device-updated", this._boundHandleDeviceUpdated);
  }

  private async loadDevicesFromCache() {
    this.loading = true;
    this.error = "";
    try {
      const devices = await deviceCache.getDevices();
      this.devices = devices;
      this.total = devices.length;
    } catch (err: any) {
      this.error = err.message || "加载设备列表失败";
    } finally {
      this.loading = false;
    }
  }

  // === Data Loading ===

  async loadDevices() {
    this.loading = true;
    this.error = "";
    try {
      const params: Record<string, any> = {
        page: this.page,
        pageSize: this.pageSize,
      };
      if (this.searchName) params.name = this.searchName;
      if (this.filterStatus) params.status = this.filterStatus;
      if (this.filterProtocol) params.protocolType = this.filterProtocol;

      const res = await deviceApi.getDevices(params);
      const data = res.result;
      if (data) {
        this.devices = data.data || [];
        this.totalPages = data.pagination?.totalPages || 0;
        this.total = data.pagination?.totalCount || this.devices.length;
      }
    } catch (err: any) {
      this.error = err.message || "加载设备列表失败";
    } finally {
      this.loading = false;
    }
  }

  async loadDeviceDetail(id: string) {
    this.detailLoading = true;
    this.error = "";
    try {
      const res = await deviceApi.getDeviceProfile(id);
      this.selectedDevice = res.result || null;
    } catch (err: any) {
      this.error = err.message || "加载设备详情失败";
    } finally {
      this.detailLoading = false;
      this.loading = false;
    }
  }

  async loadDriverNames() {
    try {
      const res = await driverApi.getDriverNames();
      const data = res.result;
      if (Array.isArray(data)) {
        this.driverNames = data;
      }
    } catch {
      // non-critical
    }
  }

  async loadDriverConfig(driverName: string) {
    this.wizConfigLoading = true;
    this.wizConfigOptions = [];
    this.wizDriverConfig = {};
    try {
      const res = await driverApi.getDriverConfig(driverName);
      const data = res.result;
      if (data) {
        this.wizConfigOptions = (data.configOptions || []).map((o: any) => ({
          label: o.label,
          name: o.name,
          defaultValue: o.defaultValue || "",
          optionType: o.optionType || "string",
          required: o.required ?? false,
          description: o.description,
        }));
        const defaults: Record<string, string> = {};
        for (const opt of this.wizConfigOptions) {
          defaults[opt.name] = opt.defaultValue;
        }
        this.wizDriverConfig = defaults;
      }
    } catch {
      // config may not exist for all drivers
    } finally {
      this.wizConfigLoading = false;
    }
  }

  // === Tags ===

  async loadAllTags() {
    try {
      const res = await tagApi.getTags();
      this.allTags = res.result?.data || [];
    } catch {
      // non-critical
    }
  }

  toggleTagEditor(deviceId: string) {
    this.editingTagsDeviceId = this.editingTagsDeviceId === deviceId ? null : deviceId;
    this.tagSearchKeyword = "";
  }

  async toggleTag(device: Device, tag: Tag) {
    if (this.tagSaving) return;
    this.tagSaving = true;
    try {
      const deviceTags = device.tags || [];
      const existing = deviceTags.find(t => t.id === tag.id);
      if (existing) {
        await tagApi.removeBinding(existing.id);
      } else {
        await tagApi.createBinding({ tagId: tag.id, targetId: device.id, targetType: 'device' });
      }
      await this.loadDevices();
    } catch (err: any) {
      toastError(err.message || "标签操作失败");
    } finally {
      this.tagSaving = false;
    }
  }

  // === Navigation ===

  navigateToDevice(id: string) {
    window.history.pushState({}, "", `/devices/${id}`);
    window.dispatchEvent(new PopStateEvent("popstate"));
    this.loadDeviceDetail(id);
  }

  backToList() {
    this.selectedDevice = null;
    this.detailTab = "properties";
    window.history.pushState({}, "", "/devices");
    window.dispatchEvent(new PopStateEvent("popstate"));
    this.loadDevices();
  }

  switchDetailTab(key: string) {
    this.detailTab = key;
  }

  isNumericType(dataType: string): boolean {
    const dt = dataType?.toLowerCase() || "";
    return ["int", "integer", "float", "double", "number", "long", "short", "decimal", "byte"].some(t => dt.includes(t));
  }

  async openPropertyHistory(name: string, unit: string) {
    const deviceId = this.selectedDevice?.device?.id;
    if (!deviceId) return;

    this.showHistoryDialog = true;
    this.historyPropertyName = name;
    this.historyPropertyUnit = unit;
    this.historyDeviceId = deviceId;
    this.historyRange = "1h";
    this.historyCustomStart = "";
    this.historyCustomEnd = "";
    this.historyData = [];
    this.loadHistoryData();
  }

  async loadHistoryData() {
    if (!this.historyDeviceId || !this.historyPropertyName) return;
    this.historyLoading = true;

    let startTime: string | undefined;
    let endTime: string | undefined;
    const now = new Date();

    if (this.historyRange === "custom") {
      if (this.historyCustomStart) startTime = this.historyCustomStart;
      if (this.historyCustomEnd) endTime = this.historyCustomEnd;
    } else {
      const minutes: Record<string, number> = { "30m": 30, "1h": 60, "5h": 300, "24h": 1440 };
      const m = minutes[this.historyRange] || 60;
      const start = new Date(now.getTime() - m * 60 * 1000);
      startTime = start.toISOString();
    }

    try {
      const res = await eventApi.getEvents({
        deviceId: this.historyDeviceId,
        eventType: "device.property_change",
        startTime,
        endTime,
        pageSize: 500,
      });

      const events = (res as any)?.result?.items || [];
      const points: { time: string; value: number }[] = [];
      const name = this.historyPropertyName;

      for (const ev of events) {
        const title = ev.title || "";
        if (!title.includes(` - ${name}`) && !title.endsWith(` ${name}`)) continue;

        const preview = ev.contentPreview || ev.content_preview || "";
        const match = preview.match(/Current value:\s*([-\d.]+)/i)
          || preview.match(/当前值:\s*([-\d.]+)/i)
          || preview.match(/value:\s*([-\d.]+)/i);
        if (!match) continue;

        const val = parseFloat(match[1]);
        if (isNaN(val)) continue;

        const ts = ev.createdAt || ev.timestamp || ev.created_at || "";
        points.push({ time: ts, value: val });
      }

      points.sort((a, b) => a.time.localeCompare(b.time));
      this.historyData = points;
    } catch {
      this.historyData = [];
    } finally {
      this.historyLoading = false;
    }
  }

  onHistoryRangeChange(range: string) {
    this.historyRange = range;
    if (range !== "custom") {
      this.loadHistoryData();
    }
  }

  onHistoryCustomTimeApply() {
    if (!this.historyCustomStart && !this.historyCustomEnd) return;
    this.loadHistoryData();
  }

  closeHistoryDialog() {
    this.showHistoryDialog = false;
    this.historyData = [];
    this.historyPropertyName = "";
    this.historyPropertyUnit = "";
    this.historyRange = "1h";
    this.historyCustomStart = "";
    this.historyCustomEnd = "";
    this.historyDeviceId = "";
  }

  renderHistoryDialog() {
    if (!this.showHistoryDialog) return nothing;

    const ranges = [
      { key: "30m", label: "30分钟" },
      { key: "1h", label: "1小时" },
      { key: "5h", label: "5小时" },
      { key: "24h", label: "24小时" },
      { key: "custom", label: "自定义" },
    ];

    return html`
      <div class="modal-overlay" @click=${this.closeHistoryDialog}>
        <div class="modal" style="max-width: 720px; width: 90vw;" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <span>${this.historyPropertyName}${this.historyPropertyUnit ? ` (${this.historyPropertyUnit})` : ""} — 历史曲线</span>
            <button class="btn btn--icon" @click=${this.closeHistoryDialog} style="margin-left: auto;">×</button>
          </div>
          <div class="modal-body" style="min-height: 300px; padding: 16px;">
            <!-- Time range selector -->
            <div style="display: flex; gap: 6px; margin-bottom: 14px; flex-wrap: wrap; align-items: center;">
              ${ranges.map(r => html`
                <button
                  class="btn"
                  style="padding: 4px 12px; font-size: 12px; border-radius: 16px; ${this.historyRange === r.key
                    ? "background: var(--accent); color: #fff; border-color: var(--accent);"
                    : "background: var(--bg-subtle); border-color: var(--border); color: var(--text);"}"
                  @click=${() => this.onHistoryRangeChange(r.key)}
                >${r.label}</button>
              `)}
            </div>
            ${this.historyRange === "custom" ? html`
              <div style="display: flex; gap: 8px; align-items: center; margin-bottom: 14px; flex-wrap: wrap;">
                <label style="font-size: 12px; color: var(--muted);">开始</label>
                <input type="datetime-local" style="font-size: 12px; padding: 4px 8px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg); color: var(--text);"
                  .value=${this.historyCustomStart}
                  @change=${(e: Event) => { this.historyCustomStart = (e.target as HTMLInputElement).value; }}
                />
                <label style="font-size: 12px; color: var(--muted);">结束</label>
                <input type="datetime-local" style="font-size: 12px; padding: 4px 8px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg); color: var(--text);"
                  .value=${this.historyCustomEnd}
                  @change=${(e: Event) => { this.historyCustomEnd = (e.target as HTMLInputElement).value; }}
                />
                <button class="btn" style="font-size: 12px; padding: 4px 12px; border-radius: 6px; background: var(--accent); color: #fff; border-color: var(--accent);"
                  @click=${this.onHistoryCustomTimeApply}
                >查询</button>
              </div>
            ` : nothing}
            <!-- Chart -->
            ${this.historyLoading
              ? html`<div style="display: flex; align-items: center; justify-content: center; height: 260px; color: var(--muted);">加载中...</div>`
              : this.historyData.length === 0
                ? html`<div style="display: flex; align-items: center; justify-content: center; height: 260px; color: var(--muted);">暂无历史数据</div>`
                : html`<div id="history-chart-container" style="width: 100%; height: 280px; position: relative;">
                    <canvas id="history-chart" style="width: 100%; height: 100%;"></canvas>
                  </div>`
            }
          </div>
        </div>
      </div>
    `;
  }

  drawHistoryChart() {
    const canvas = this.querySelector("#history-chart") as HTMLCanvasElement;
    if (!canvas || this.historyData.length === 0) return;

    const container = this.querySelector("#history-chart-container") as HTMLElement;
    if (!container) return;

    const dpr = window.devicePixelRatio || 1;
    const w = container.clientWidth;
    const h = container.clientHeight;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    canvas.style.width = w + "px";
    canvas.style.height = h + "px";

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.scale(dpr, dpr);

    const data = this.historyData;
    const padding = { top: 24, right: 20, bottom: 36, left: 56 };
    const chartW = w - padding.left - padding.right;
    const chartH = h - padding.top - padding.bottom;

    const values = data.map(d => d.value);
    let minVal = Math.min(...values);
    let maxVal = Math.max(...values);
    if (minVal === maxVal) { minVal -= 1; maxVal += 1; }
    const range = maxVal - minVal;

    const cs = getComputedStyle(document.documentElement);
    const textColor = cs.getPropertyValue("--muted").trim() || "#888";
    const lineColor = cs.getPropertyValue("--accent").trim() || "#6366f1";
    const gridColor = cs.getPropertyValue("--border").trim() || "#e5e7eb";

    ctx.clearRect(0, 0, w, h);

    // Grid lines + Y labels
    ctx.strokeStyle = gridColor;
    ctx.lineWidth = 0.5;
    ctx.fillStyle = textColor;
    ctx.font = "11px system-ui, sans-serif";
    ctx.textAlign = "right";
    const yTicks = 5;
    for (let i = 0; i <= yTicks; i++) {
      const y = padding.top + (chartH / yTicks) * i;
      const val = maxVal - (range / yTicks) * i;
      ctx.beginPath();
      ctx.moveTo(padding.left, y);
      ctx.lineTo(w - padding.right, y);
      ctx.stroke();
      ctx.fillText(val.toFixed(1), padding.left - 6, y + 4);
    }

    // X labels
    ctx.textAlign = "center";
    const xLabelCount = Math.min(data.length, 6);
    const xStep = Math.max(1, Math.floor(data.length / xLabelCount));
    for (let i = 0; i < data.length; i += xStep) {
      const x = padding.left + (chartW / (data.length - 1)) * i;
      const label = data[i].time.slice(5, 16);
      ctx.fillText(label, x, h - padding.bottom + 16);
    }

    // Line
    ctx.strokeStyle = lineColor;
    ctx.lineWidth = 2;
    ctx.lineJoin = "round";
    ctx.beginPath();
    for (let i = 0; i < data.length; i++) {
      const x = padding.left + (chartW / (data.length - 1)) * i;
      const y = padding.top + chartH - ((data[i].value - minVal) / range) * chartH;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    // Dots
    ctx.fillStyle = lineColor;
    for (let i = 0; i < data.length; i++) {
      const x = padding.left + (chartW / (data.length - 1)) * i;
      const y = padding.top + chartH - ((data[i].value - minVal) / range) * chartH;
      ctx.beginPath();
      ctx.arc(x, y, 3, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  goToPage(p: number) {
    this.page = p;
    this.loadDevices();
  }

  // === Helpers ===

  statusLabel(status?: string): string {
    switch (status) {
      case "online": return "在线";
      case "offline": return "离线";
      case "error": return "故障";
      case "maintenance": return "维护";
      default: return "未知";
    }
  }

  statusColor(status?: string): string {
    switch (status) {
      case "online": return "var(--success)";
      case "offline": return "var(--muted)";
      case "error": return "var(--danger)";
      case "maintenance": return "var(--warning)";
      default: return "var(--muted)";
    }
  }

  // === Edit Modal ===

  openCreate() {
    this.editingDevice = null;
    this.formName = "";
    this.formType = "";
    this.formAddress = "";
    this.formDescription = "";
    this.formManufacturer = "";
    this.formModel = "";
    this.formProtocol = "";
    this.showModal = true;
  }

  openEdit(d: Device) {
    this.editingDevice = d;
    this.formName = d.name;
    this.formType = d.deviceType || "";
    this.formAddress = d.address || "";
    this.formDescription = d.description || "";
    this.formManufacturer = d.factoryName || "";
    this.formModel = d.deviceModel || "";
    this.formProtocol = d.protocolType || "";
    this.showModal = true;
  }

  closeModal() {
    this.showModal = false;
    this.editingDevice = null;
  }

  async saveForm() {
    if (!this.formName.trim()) return;
    this.saving = true;
    try {
      const payload: CreateDeviceRequest = {
        name: this.formName,
        type: this.formType || undefined,
        ipAddress: this.formAddress || undefined,
        description: this.formDescription || undefined,
        manufacturer: this.formManufacturer || undefined,
        model: this.formModel || undefined,
        protocol: this.formProtocol || undefined,
      };
      if (this.editingDevice) {
        await deviceApi.updateDevice(this.editingDevice.id, payload as any);
        success("设备已更新");
      } else {
        await deviceApi.createDevice(payload);
        success("设备已创建");
      }
      this.closeModal();
      await this.loadDevices();
    } catch (err: any) {
      toastError(err.message || "操作失败");
    } finally {
      this.saving = false;
    }
  }

  async deleteDevice(d: Device) {
    if (!confirm(`确定要删除设备 "${d.displayName || d.name}" 吗？`)) return;
    try {
      await deviceApi.deleteDevice(d.id);
      success("设备已删除");
      await this.loadDevices();
    } catch (err: any) {
      toastError(err.message || "删除失败");
    }
  }

  async executeCommand(deviceId: string, commandName: string) {
    if (this.executingCommand) return;
    this.executingCommand = commandName;
    try {
      await deviceApi.executeCommand(deviceId, commandName);
      success(`命令 "${commandName}" 执行成功`);
      await this.loadDeviceDetail(deviceId);
    } catch (err: any) {
      toastError(err.message || "命令执行失败");
    } finally {
      this.executingCommand = "";
    }
  }

  // === Wizard (2-step template-based) ===

  openWizard() {
    this.wizardStep = "template";
    this.wizSelectedTemplate = null;
    this.wizTemplateSearch = "";
    this.wizName = "";
    this.wizDescription = "";
    this.wizAddress = "";
    this.wizPosition = "";
    this.wizDriver = "";
    this.wizDriverConfig = {};
    this.wizConfigOptions = [];
    this.wizValidationErrors = {};
    this.wizardSaving = false;
    this.showWizard = true;
    this.loadTemplates();
  }

  closeWizard() {
    this.showWizard = false;
  }

  async loadTemplates() {
    this.wizTemplateLoading = true;
    try {
      const res = await templateApi.getTemplates({ page: 1, pageSize: 200 });
      const data = res.result;
      const rawList = data?.data || data || [];
      this.wizTemplates = (Array.isArray(rawList) ? rawList : []).map(transformTemplate);
    } catch {
      this.wizTemplates = [];
    } finally {
      this.wizTemplateLoading = false;
    }
  }

  selectTemplate(template: ProcessedTemplate) {
    this.wizSelectedTemplate = template;

    // Auto-fill from template defaults
    const di = template.deviceInfo;
    this.wizName = di.defaultNamePattern
      ? di.defaultNamePattern.replace("{name}", template.name)
      : template.name;
    this.wizDescription = di.defaultDescription
      ? getLocalizedText(di.defaultDescription, "")
      : getLocalizedText(template.description || {}, "");
    this.wizAddress = "";
    this.wizPosition = di.defaultPosition || "";
    this.wizDriver = template.driverName || "";
    this.wizDriverConfig = {};
    this.wizConfigOptions = [];
    this.wizValidationErrors = {};

    if (this.wizDriver) {
      this.loadDriverConfig(this.wizDriver);
    }
    this.wizardStep = "device";
  }

  wizardBack() {
    this.wizardStep = "template";
    this.wizValidationErrors = {};
  }

  async onWizardDriverSelect(driverName: string) {
    this.wizDriver = driverName;
    this.wizDriverConfig = {};
    this.wizConfigOptions = [];
    if (driverName) {
      await this.loadDriverConfig(driverName);
    }
  }

  get filteredWizardTemplates(): ProcessedTemplate[] {
    const q = this.wizTemplateSearch.trim().toLowerCase();
    if (!q) return this.wizTemplates;
    return this.wizTemplates.filter(t => {
      const name = t.name?.toLowerCase() || "";
      const displayName = getLocalizedText(t.displayName, "").toLowerCase();
      const desc = t.description ? Object.values(t.description).join(" ").toLowerCase() : "";
      return name.includes(q) || displayName.includes(q) || desc.includes(q);
    });
  }

  get wizardTemplatesByCategory(): Record<string, ProcessedTemplate[]> {
    const groups: Record<string, ProcessedTemplate[]> = {};
    for (const t of this.filteredWizardTemplates) {
      const cat = t.category || "others";
      if (!groups[cat]) groups[cat] = [];
      groups[cat].push(t);
    }
    return groups;
  }

  validateWizardForm(): boolean {
    const errors: Record<string, string> = {};

    if (!this.wizName.trim()) {
      errors.deviceName = "设备名称不能为空";
    } else if (this.wizName.trim().length < 2) {
      errors.deviceName = "设备名称至少需要2个字符";
    } else if (this.wizName.trim().length > 50) {
      errors.deviceName = "设备名称不能超过50个字符";
    }

    if (this.wizSelectedTemplate && isFieldRequired(this.wizSelectedTemplate.deviceInfo, "address") && !this.wizAddress.trim()) {
      errors.deviceAddress = "设备地址是必填字段";
    }

    if (this.wizDriver && this.wizConfigOptions.length > 0) {
      for (const opt of this.wizConfigOptions) {
        if (opt.required) {
          const userValue = this.wizDriverConfig[opt.name];
          const hasUserValue = userValue !== undefined && userValue.trim() !== "";
          const hasDefaultValue = opt.defaultValue && opt.defaultValue.trim() !== "";
          if (!hasUserValue && !hasDefaultValue) {
            errors[`driverConfig.${opt.name}`] = `${opt.label}是必填字段`;
          }
        }
      }
    }

    this.wizValidationErrors = errors;
    return Object.keys(errors).length === 0;
  }

  async submitWizard() {
    if (!this.wizSelectedTemplate) {
      toastError("请先选择设备模板");
      return;
    }
    if (!this.validateWizardForm()) {
      toastError("请检查并修正表单中的错误");
      return;
    }
    if (this.wizardSaving) return;

    this.wizardSaving = true;
    try {
      // Build final driver config merging user values with defaults
      const finalDriverConfig: Record<string, string> = {};
      if (this.wizDriver && this.wizConfigOptions.length > 0) {
        for (const opt of this.wizConfigOptions) {
          const userValue = this.wizDriverConfig[opt.name];
          if (userValue !== undefined && userValue !== "") {
            finalDriverConfig[opt.name] = userValue;
          } else if (opt.defaultValue) {
            finalDriverConfig[opt.name] = opt.defaultValue;
          }
        }
      }

      const deviceInput = {
        name: this.wizName.trim(),
        displayName: this.wizName.trim(),
        description: this.wizDescription.trim() || undefined,
        address: this.wizAddress.trim() || undefined,
        position: this.wizPosition.trim() || undefined,
        driverName: this.wizDriver || undefined,
        driverOptions: Object.keys(finalDriverConfig).length > 0 ? JSON.stringify(finalDriverConfig) : undefined,
        propertyValues: {},
        enabledCommands: this.wizSelectedTemplate.commands?.map((c: any) => c.name) || [],
      };

      await deviceApi.createDeviceFromTemplate({
        templateId: this.wizSelectedTemplate.id,
        deviceInput,
      });

      success("设备创建成功");
      this.closeWizard();
      await this.loadDevices();
    } catch (err: any) {
      toastError(err.message || "设备创建失败");
    } finally {
      this.wizardSaving = false;
    }
  }

  // === Render ===

  render() {
    // 列表页: 从缓存信号读取最新数据（SSE 更新时 SignalWatcher 自动触发 re-render）
    if (!this.selectedDevice) {
      const cachedDevices = deviceCache.$devicesList.get();
      if (cachedDevices.length > 0 || !this.loading) {
        this.devices = cachedDevices;
        this.total = cachedDevices.length;
      }
    }

    if (this.loading) {
      return html`
        <div style="display: flex; align-items: center; justify-content: center; padding: 60px;">
          <span class="loading-spinner"></span>
          <span style="margin-left: 8px; color: var(--muted);">加载中...</span>
        </div>
      `;
    }

    if (this.error) {
      return html`
        <div style="text-align: center; padding: 60px;">
          <div style="color: var(--danger); margin-bottom: 12px;">${this.error}</div>
          <button class="btn btn--primary" @click=${() => this.selectedDevice ? this.loadDeviceDetail(this.selectedDevice.device.id) : this.loadDevices()}>重试</button>
        </div>
      `;
    }

    if (this.selectedDevice) {
      return this.renderDeviceDetail();
    }

    return this.renderDeviceList();
  }

  renderToolbar() {
    return html`
      <div style="display: flex; gap: 10px; margin-bottom: 16px; align-items: center; flex-wrap: wrap;">
        <div class="field" style="flex: 1; min-width: 180px; max-width: 300px;">
          <input
            type="text"
            placeholder="搜索设备名称..."
            .value=${this.searchName}
            @input=${(e: Event) => { this.searchName = (e.target as HTMLInputElement).value; }}
            @keydown=${(e: KeyboardEvent) => { if (e.key === "Enter") { this.page = 1; this.loadDevices(); } }}
          />
        </div>
        <select class="select" style="width: auto; min-width: 120px;" .value=${this.filterStatus} @change=${(e: Event) => { this.filterStatus = (e.target as HTMLSelectElement).value; this.page = 1; this.loadDevices(); }}>
          <option value="">全部状态</option>
          <option value="online">在线</option>
          <option value="offline">离线</option>
          <option value="error">故障</option>
          <option value="maintenance">维护</option>
        </select>
        <select class="select" style="width: auto; min-width: 120px;" .value=${this.filterProtocol} @change=${(e: Event) => { this.filterProtocol = (e.target as HTMLSelectElement).value; this.page = 1; this.loadDevices(); }}>
          <option value="">全部协议</option>
          <option value="modbus-tcp">Modbus TCP</option>
          <option value="modbus-rtu">Modbus RTU</option>
          <option value="mqtt">MQTT</option>
          <option value="onvif">ONVIF</option>
          <option value="snmp">SNMP</option>
        </select>
        <div style="display: flex; gap: 4px; margin-left: auto;">
          <button
            class="btn btn--ghost btn--sm"
            style=${`padding: 6px 10px; ${this.viewMode === 'table' ? 'background: var(--bg-subtle);' : ''}`}
            @click=${() => { this.viewMode = "table"; }}
            title="列表视图"
          >&#9776;</button>
          <button
            class="btn btn--ghost btn--sm"
            style=${`padding: 6px 10px; ${this.viewMode === 'grid' ? 'background: var(--bg-subtle);' : ''}`}
            @click=${() => { this.viewMode = "grid"; }}
            title="卡片视图"
          >&#9638;</button>
        </div>
        <button class="btn btn--primary" @click=${this.openWizard}>新建设备</button>
      </div>
    `;
  }

  renderDeviceList() {
    return html`
      ${this.renderToolbar()}
      ${this.viewMode === "table" ? this.renderTableView() : this.renderGridView()}
      ${this.totalPages > 1 ? html`
        <div class="pagination">
          <button class="btn btn--ghost btn--sm" ?disabled=${this.page <= 1} @click=${() => this.goToPage(this.page - 1)}>上一页</button>
          <span class="pagination-info">第 ${this.page} / ${this.totalPages} 页，共 ${this.total} 条</span>
          <button class="btn btn--ghost btn--sm" ?disabled=${this.page >= this.totalPages} @click=${() => this.goToPage(this.page + 1)}>下一页</button>
        </div>
      ` : ""}
      ${this.showModal ? this.renderModal() : nothing}
      ${this.showWizard ? this.renderWizard() : nothing}
    `;
  }

  renderTableView() {
    return html`
      <div class="card" style="overflow: hidden;">
        <table style="width: 100%; border-collapse: collapse;">
          <thead>
            <tr style="border-bottom: 1px solid var(--border);">
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">设备名称</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">类型</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">协议</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">状态</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">标签</th>
              <th style="padding: 12px 16px; text-align: right; font-size: 13px; color: var(--muted); font-weight: 500;">操作</th>
            </tr>
          </thead>
          <tbody>
            ${this.devices.length === 0
              ? html`<tr><td colspan="6" style="padding: 40px; text-align: center; color: var(--muted);">暂无设备</td></tr>`
              : this.devices.map(d => html`
                <tr style="border-bottom: 1px solid var(--border);">
                  <td style="padding: 12px 16px;">
                    <div style="font-weight: 500;">${d.displayName || d.name}</div>
                    <div style="font-size: 12px; color: var(--muted);">${d.name}</div>
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px;">${d.deviceType || "-"}</td>
                  <td style="padding: 12px 16px; font-size: 13px;">${d.protocolType || d.driverName || "-"}</td>
                  <td style="padding: 12px 16px;">
                    <span style="display: inline-flex; align-items: center; gap: 6px; font-size: 13px;">
                      <span style="width: 8px; height: 8px; border-radius: 50%; background: ${this.statusColor(d.status)};"></span>
                      ${this.statusLabel(d.status)}
                    </span>
                  </td>
                  <td style="padding: 12px 16px; position: relative;">
                    ${this.renderTableCellTags(d)}
                  </td>
                  <td style="padding: 12px 16px; text-align: right;">
                    <button class="btn btn--ghost btn--sm" style="font-size: 12px;" @click=${() => this.navigateToDevice(d.id)}>详情</button>
                    <button class="btn btn--ghost btn--sm" style="font-size: 12px;" @click=${() => this.openEdit(d)}>编辑</button>
                    <button class="btn btn--ghost btn--sm" style="font-size: 12px; color: var(--danger);" @click=${() => this.deleteDevice(d)}>删除</button>
                  </td>
                </tr>
              `)}
          </tbody>
        </table>
      </div>
    `;
  }

  renderGridView() {
    if (this.devices.length === 0) {
      return html`
        <div class="card" style="padding: 40px; text-align: center; color: var(--muted);">暂无设备</div>
      `;
    }
    return html`
      <div style="display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 16px;">
        ${this.devices.map(d => this.renderDeviceCard(d))}
      </div>
    `;
  }

  renderTableCellTags(d: Device) {
    const deviceTags = d.tags || [];
    const isEditingTags = this.editingTagsDeviceId === d.id;
    return html`
      <div style="display: inline-flex; flex-wrap: wrap; gap: 4px; align-items: center;">
        ${deviceTags.slice(0, 3).map(t => html`
          <span style="display: inline-flex; align-items: center; gap: 3px; padding: 1px 6px; border-radius: 9999px; font-size: 11px; background: var(--bg-subtle); border: 1px solid var(--border);">
            <span style="width: 5px; height: 5px; border-radius: 50%; background: ${t.color || 'var(--primary, #3b82f6)'};"></span>
            ${t.name}
          </span>
        `)}
        ${deviceTags.length > 3 ? html`<span style="font-size: 11px; color: var(--muted);">+${deviceTags.length - 3}</span>` : nothing}
        ${deviceTags.length === 0 ? html`<span style="font-size: 11px; color: var(--muted);">-</span>` : nothing}
        <button
          class="btn btn--ghost btn--sm"
          style="padding: 1px 3px; font-size: 11px;"
          title="管理标签"
          @click=${(e: Event) => { e.stopPropagation(); this.toggleTagEditor(d.id); }}
        >${icons.tag}</button>
      </div>
      ${isEditingTags ? this.renderTagPopover(d, deviceTags) : nothing}
    `;
  }

  renderTagPopover(d: Device, deviceTags: Tag[]) {
    return html`
      <div
        style="position: absolute; top: 100%; left: 0; z-index: 100; margin-top: 4px; min-width: 220px;
          border: 1px solid var(--border); border-radius: 8px; padding: 8px;
          background: var(--bg); box-shadow: 0 4px 12px rgba(0,0,0,0.15);"
        @click=${(e: Event) => e.stopPropagation()}
      >
        <input
          type="text"
          placeholder="搜索标签..."
          .value=${this.tagSearchKeyword}
          @input=${(e: Event) => { this.tagSearchKeyword = (e.target as HTMLInputElement).value; }}
          style="width: 100%; font-size: 12px; padding: 4px 8px; margin-bottom: 6px; box-sizing: border-box;"
        />
        <div style="display: flex; flex-wrap: wrap; gap: 4px; max-height: 120px; overflow-y: auto;">
          ${this.allTags
            .filter(t => !this.tagSearchKeyword || t.name.toLowerCase().includes(this.tagSearchKeyword.toLowerCase()))
            .map(t => {
              const bound = deviceTags.some(dt => dt.id === t.id);
              return html`
                <button
                  class="btn btn--sm"
                  style=${`font-size: 11px; padding: 2px 8px; border-radius: 9999px;
                    ${bound ? 'background: var(--primary, #3b82f6); color: #fff; border-color: var(--primary, #3b82f6);' : 'background: var(--bg-subtle); border: 1px solid var(--border);'}`}
                  ?disabled=${this.tagSaving}
                  @click=${() => this.toggleTag(d, t)}
                >
                  <span style="display: inline-flex; align-items: center; gap: 4px;">
                    ${bound ? icons.check : icons.plus}
                    ${t.name}
                  </span>
                </button>
              `;
            })}
          ${this.allTags.filter(t => !this.tagSearchKeyword || t.name.toLowerCase().includes(this.tagSearchKeyword.toLowerCase())).length === 0
            ? html`<span style="font-size: 11px; color: var(--muted); padding: 4px;">无匹配标签</span>`
            : nothing}
        </div>
      </div>
    `;
  }

  renderDeviceCard(d: Device) {
    const deviceTags = d.tags || [];
    const visibleTags = deviceTags.slice(0, 3);
    const hiddenTagCount = deviceTags.length - 3;
    const isEditingTags = this.editingTagsDeviceId === d.id;

    // Middle content for tooltip
    const infoLines = [
      d.deviceType || null,
      d.protocolType || d.driverName || null,
      d.address || null,
    ].filter(Boolean);
    const infoTooltip = infoLines.join('\n');

    return html`
      <div style="position: relative;">
        <div class="card" style="padding: 2px; overflow: hidden; display: flex; flex-direction: column; height: 180px;">
          <!-- Header -->
          <div style="padding: 14px 16px 0; display: flex; align-items: center; justify-content: space-between; flex-shrink: 0;">
            <div style="display: flex; align-items: center; gap: 8px; min-width: 0;">
              <span style="width: 8px; height: 8px; border-radius: 50%; background: ${this.statusColor(d.status)}; flex-shrink: 0;"></span>
              <span style="font-weight: 600; font-size: 15px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;" title="${d.displayName || d.name}">${d.displayName || d.name}</span>
            </div>
            <div style="display: flex; gap: 2px; flex-shrink: 0;">
              <button
                class="btn btn--ghost btn--sm"
                style="padding: 4px 6px; color: var(--muted);"
                title="编辑"
                @click=${(e: Event) => { e.stopPropagation(); this.openEdit(d); }}
              >${icons.edit}</button>
              <button
                class="btn btn--ghost btn--sm"
                style="padding: 4px 6px; color: var(--danger);"
                title="删除"
                @click=${(e: Event) => { e.stopPropagation(); this.deleteDevice(d); }}
              >${icons.trash2}</button>
            </div>
          </div>

          <!-- Info (fixed height, truncated with tooltip) -->
          <div
            style="padding: 10px 16px; cursor: pointer; overflow: hidden; flex: 1; min-height: 0;"
            title="${infoTooltip}"
            @click=${() => this.navigateToDevice(d.id)}
          >
            <div style="display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: var(--muted);">
              ${d.deviceType ? html`<div style="white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${d.deviceType}</div>` : nothing}
              ${d.protocolType || d.driverName ? html`<div style="white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${d.protocolType || d.driverName}</div>` : nothing}
              ${d.address ? html`<div style="white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${d.address}</div>` : nothing}
            </div>
          </div>

          <!-- Tags (footer, fixed) -->
          <div style="padding: 0 16px 8px; display: flex; flex-wrap: wrap; gap: 4px; align-items: center; flex-shrink: 0; overflow: hidden;">
            ${visibleTags.map(t => html`
              <span style="display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border-radius: 9999px; font-size: 11px; background: var(--bg-subtle); border: 1px solid var(--border);">
                <span style="width: 6px; height: 6px; border-radius: 50%; background: ${t.color || 'var(--primary, #3b82f6)'};"></span>
                ${t.name}
              </span>
            `)}
            ${hiddenTagCount > 0 ? html`
              <span style="padding: 2px 8px; border-radius: 9999px; font-size: 11px; background: var(--bg-subtle); border: 1px solid var(--border); color: var(--muted);" title="${deviceTags.slice(3).map(t => t.name).join(', ')}">
                +${hiddenTagCount}
              </span>
            ` : nothing}
            ${deviceTags.length === 0 ? html`<span style="font-size: 11px; color: var(--muted);">无标签</span>` : nothing}
            <button
              class="btn btn--ghost btn--sm"
              style="padding: 2px 4px; font-size: 11px;"
              title="管理标签"
              @click=${(e: Event) => { e.stopPropagation(); this.toggleTagEditor(d.id); }}
            >${icons.tag}</button>
          </div>
        </div>

        <!-- Tag editor popover — floats outside the card -->
        ${isEditingTags ? this.renderTagPopover(d, deviceTags) : nothing}
      </div>
    `;
  }

  renderDeviceDetail() {
    const profile = this.selectedDevice;
    if (!profile) return nothing;
    const d = profile.device;
    const ov = profile.overview;
    const deviceTags: Tag[] = (d as any).tags || [];

    return html`
      <!-- Header: name, status, type, tags, edit -->
      <div class="card" style="padding: 16px 20px; margin-bottom: 12px;">
        <div style="display: flex; justify-content: space-between; align-items: center;">
          <div style="display: flex; align-items: center; gap: 12px;">
            <button class="btn btn--ghost btn--sm" @click=${this.backToList} style="padding: 4px 8px;">
              &larr; 返回
            </button>
            <h2 style="margin: 0; font-size: 18px;">${d.displayName || d.name}</h2>
            <span style="display: inline-flex; align-items: center; gap: 5px; padding: 2px 10px; border-radius: 9999px; font-size: 12px; background: var(--bg-subtle);">
              <span style="width: 7px; height: 7px; border-radius: 50%; background: ${this.statusColor(d.status)};"></span>
              ${this.statusLabel(d.status)}
            </span>
            ${d.deviceType ? html`
              <span style="display: inline-flex; padding: 2px 8px; border-radius: 4px; font-size: 11px; background: var(--accent-subtle, rgba(59,130,246,0.08)); color: var(--accent, #3b82f6);">
                ${d.deviceType}
              </span>
            ` : nothing}
          </div>
          <button class="btn btn--ghost btn--sm" @click=${() => this.openEdit(d)}>编辑</button>
        </div>
        ${deviceTags.length > 0 ? html`
          <div style="padding-top: 8px; display: flex; flex-wrap: wrap; gap: 4px;">
            ${deviceTags.map((t: Tag) => html`
              <span style="display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border-radius: 9999px; font-size: 11px; background: var(--bg-subtle); border: 1px solid var(--border);">
                <span style="width: 6px; height: 6px; border-radius: 50%; background: ${t.color || 'var(--primary, #3b82f6)'};"></span>
                ${t.name}
              </span>
            `)}
          </div>
        ` : nothing}
      </div>

      <!-- Mini stat grid -->
      <div style="display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; margin-bottom: 12px;">
        <div class="detail-stat-mini">
          <div class="detail-stat-mini__label">属性总数</div>
          <div class="detail-stat-mini__value">${ov.totalProperties}</div>
        </div>
        <div class="detail-stat-mini">
          <div class="detail-stat-mini__label">在线属性</div>
          <div class="detail-stat-mini__value" style="color: var(--success);">${ov.onlineProperties}</div>
        </div>
        <div class="detail-stat-mini">
          <div class="detail-stat-mini__label">命令数</div>
          <div class="detail-stat-mini__value">${ov.totalCommands}</div>
        </div>
        <div class="detail-stat-mini">
          <div class="detail-stat-mini__label">活跃告警</div>
          <div class="detail-stat-mini__value" style="color: ${ov.activeAlarms > 0 ? 'var(--danger)' : 'inherit'};">${ov.activeAlarms}</div>
        </div>
      </div>

      <!-- Tab bar -->
      <div class="detail-tabs" style="margin-bottom: 12px;">
        <button class="detail-tab ${this.detailTab === 'properties' ? 'active' : ''}" @click=${() => this.switchDetailTab('properties')}>${icons.barChart} 属性</button>
        <button class="detail-tab ${this.detailTab === 'commands' ? 'active' : ''}" @click=${() => this.switchDetailTab('commands')}>${icons.zap} 命令</button>
        <button class="detail-tab ${this.detailTab === 'events' ? 'active' : ''}" @click=${() => this.switchDetailTab('events')}>${icons.scrollText} 事件</button>
        <button class="detail-tab ${this.detailTab === 'alarms' ? 'active' : ''}" @click=${() => this.switchDetailTab('alarms')}>${icons.bug} 告警</button>
      </div>

      <!-- Tab content -->
      ${this.detailTab === 'properties' ? this.renderDetailProperties() : nothing}
      ${this.detailTab === 'commands' ? this.renderDetailCommands() : nothing}
      ${this.detailTab === 'events' ? this.renderDetailEvents() : nothing}
      ${this.detailTab === 'alarms' ? this.renderDetailAlarms() : nothing}
      ${this.showModal ? this.renderModal() : nothing}
      ${this.showHistoryDialog ? this.renderHistoryDialog() : nothing}
    `;
  }

  renderDetailProperties() {
    const profile = this.selectedDevice;
    if (!profile || profile.properties.length === 0) {
      return html`<div class="card" style="padding: 32px; text-align: center; color: var(--muted);">暂无属性数据</div>`;
    }

    return html`
      <div class="card" style="padding: 16px 20px;">
        <table style="width: 100%; border-collapse: collapse;">
          <thead>
            <tr style="border-bottom: 1px solid var(--border);">
              <th style="padding: 6px 10px; text-align: left; font-size: 11px; color: var(--muted); font-weight: 600;">属性名</th>
              <th style="padding: 6px 10px; text-align: left; font-size: 11px; color: var(--muted); font-weight: 600;">当前值</th>
              <th style="padding: 6px 10px; text-align: left; font-size: 11px; color: var(--muted); font-weight: 600;">类型</th>
              <th style="padding: 6px 10px; text-align: left; font-size: 11px; color: var(--muted); font-weight: 600;">单位</th>
              <th style="padding: 6px 10px; text-align: center; font-size: 11px; color: var(--muted); font-weight: 600;">读写</th>
              <th style="padding: 6px 10px; text-align: left; font-size: 11px; color: var(--muted); font-weight: 600;">更新时间</th>
            </tr>
          </thead>
          <tbody>
            ${profile.properties.map((p: DeviceProperty) => html`
              <tr style="border-bottom: 1px solid var(--border);">
                <td style="padding: 6px 10px; font-size: 13px;">${p.displayName || p.name}</td>
                <td style="padding: 6px 10px; font-size: 13px; font-weight: 500;">
                  ${p.currentValue ?? p.value ?? "-"}
                  ${this.isNumericType(p.dataType) ? html`
                    <button
                      class="btn btn--icon btn--xs"
                      title="查看历史曲线"
                      @click=${() => this.openPropertyHistory(p.name, p.unit || "")}
                      style="margin-left: 4px; color: var(--accent); cursor: pointer; padding: 2px;"
                    >${icons.trendingUp}</button>
                  ` : nothing}
                </td>
                <td style="padding: 6px 10px; font-size: 12px; color: var(--muted);">${p.dataType}</td>
                <td style="padding: 6px 10px; font-size: 12px; color: var(--muted);">${p.unit || "-"}</td>
                <td style="padding: 6px 10px; text-align: center;">
                  <span style="display: inline-block; padding: 1px 6px; border-radius: 4px; font-size: 10px; font-weight: 600;
                    background: ${p.isReadOnly ? 'rgba(107,114,128,0.1)' : 'rgba(16,185,129,0.1)'};
                    color: ${p.isReadOnly ? 'var(--muted)' : 'var(--success)'};">
                    ${p.isReadOnly ? '只读' : '读写'}
                  </span>
                </td>
                <td style="padding: 6px 10px; font-size: 12px; color: var(--muted);">${p.updatedAt?.slice(0, 16) || "-"}</td>
              </tr>
            `)}
          </tbody>
        </table>
      </div>
    `;
  }

  renderDetailCommands() {
    const profile = this.selectedDevice;
    if (!profile) return nothing;
    const d = profile.device;

    if (profile.commands.length === 0) {
      return html`<div class="card" style="padding: 32px; text-align: center; color: var(--muted);">暂无命令</div>`;
    }

    return html`
      <div class="card" style="padding: 16px 20px;">
        <div style="display: flex; flex-direction: column; gap: 8px;">
          ${profile.commands.map(c => html`
            <div style="display: flex; align-items: center; justify-content: space-between; padding: 10px 12px; background: var(--bg-subtle); border-radius: 8px;">
              <div>
                <div style="font-weight: 500; font-size: 13px;">${c.name}</div>
                <div style="font-size: 12px; color: var(--muted);">${c.description || "无描述"}</div>
              </div>
              <button
                class="btn btn--primary btn--sm"
                ?disabled=${this.executingCommand === c.name}
                @click=${() => this.executeCommand(d.id, c.name)}
              >
                ${this.executingCommand === c.name ? "执行中..." : "执行"}
              </button>
            </div>
          `)}
        </div>
      </div>
    `;
  }

  renderDetailEvents() {
    const profile = this.selectedDevice;
    if (!profile) return nothing;

    const events = profile.recentEvents || [];
    if (events.length === 0) {
      return html`<div class="card" style="padding: 32px; text-align: center; color: var(--muted);">暂无事件记录</div>`;
    }

    const levelClass = (level: string) => {
      switch (level) {
        case 'info': return 'event-badge--info';
        case 'warning': return 'event-badge--warning';
        case 'error': return 'event-badge--error';
        case 'critical': return 'event-badge--critical';
        default: return 'event-badge--info';
      }
    };

    const levelLabel = (level: string) => {
      switch (level) {
        case 'info': return '信息';
        case 'warning': return '警告';
        case 'error': return '错误';
        case 'critical': return '严重';
        default: return level;
      }
    };

    return html`
      <div class="card" style="padding: 16px 20px;">
        ${events.map((ev: DeviceEvent) => html`
          <div class="event-item">
            <span class="event-badge ${levelClass(ev.level)}">${levelLabel(ev.level)}</span>
            <div style="flex: 1; min-width: 0;">
              <div style="font-size: 13px; font-weight: 500;">${ev.title}</div>
              ${ev.message ? html`<div style="font-size: 12px; color: var(--muted); margin-top: 2px;">${ev.message}</div>` : nothing}
            </div>
            <span style="font-size: 11px; color: var(--muted); flex-shrink: 0; margin-top: 2px;">${ev.createdAt?.slice(0, 16)}</span>
          </div>
        `)}
      </div>
    `;
  }

  renderDetailAlarms() {
    const profile = this.selectedDevice;
    if (!profile) return nothing;
    const ov = profile.overview;

    return html`
      <div class="card" style="padding: 20px;">
        <div style="display: flex; align-items: center; gap: 12px; margin-bottom: 16px;">
          <div style="font-size: 36px; font-weight: 700; color: ${ov.activeAlarms > 0 ? 'var(--danger)' : 'var(--success)'};">
            ${ov.activeAlarms}
          </div>
          <div>
            <div style="font-size: 14px; font-weight: 500;">活跃告警</div>
            <div style="font-size: 12px; color: var(--muted);">需要处理的告警数量</div>
          </div>
        </div>
        ${ov.activeAlarms === 0
          ? html`<div style="padding: 20px; text-align: center; color: var(--success); font-size: 14px;">暂无活跃告警</div>`
          : html`
            <div style="padding: 12px; background: rgba(239,68,68,0.05); border-radius: 8px; border: 1px solid rgba(239,68,68,0.15);">
              <div style="font-size: 13px; color: var(--danger);">存在 ${ov.activeAlarms} 个活跃告警需要处理</div>
            </div>
          `
        }
      </div>
    `;
  }

  renderModal() {
    return html`
      <div class="modal-overlay" @click=${this.closeModal}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">${this.editingDevice ? "编辑设备" : "新建设备"}</div>
          <div class="modal-body">
            <div class="field">
              <span>设备名称</span>
              <input type="text" placeholder="设备名称" .value=${this.formName} @input=${(e: any) => { this.formName = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>设备类型</span>
              <input type="text" placeholder="如 sensor, gateway" .value=${this.formType} @input=${(e: any) => { this.formType = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>地址</span>
              <input type="text" placeholder="如 192.168.1.100" .value=${this.formAddress} @input=${(e: any) => { this.formAddress = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>协议</span>
              <input type="text" placeholder="如 modbus-tcp, mqtt" .value=${this.formProtocol} @input=${(e: any) => { this.formProtocol = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>厂商</span>
              <input type="text" placeholder="可选" .value=${this.formManufacturer} @input=${(e: any) => { this.formManufacturer = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>型号</span>
              <input type="text" placeholder="可选" .value=${this.formModel} @input=${(e: any) => { this.formModel = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>描述</span>
              <input type="text" placeholder="可选描述" .value=${this.formDescription} @input=${(e: any) => { this.formDescription = e.target.value; }} />
            </div>
          </div>
          <div class="modal-footer">
            <button class="btn btn--ghost" @click=${this.closeModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.saving || !this.formName.trim()} @click=${this.saveForm}>
              ${this.saving ? "保存中..." : "保存"}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  renderWizard() {
    const isStep1 = this.wizardStep === "template";
    return html`
      <div class="wizard-overlay" @click=${(e: Event) => { if ((e.target as HTMLElement).classList.contains('wizard-overlay')) this.closeWizard(); }}>
        <div class="wizard-dialog">
          <!-- Header -->
          <div class="wizard-dialog__header">
            <button class="wizard-dialog__back" @click=${isStep1 ? this.closeWizard : this.wizardBack}>
              <span style="transform: rotate(90deg); display: inline-flex;">${icons.arrowDown}</span>
              <span>${isStep1 ? "返回设备列表" : "返回模板选择"}</span>
            </button>
            <span class="wizard-dialog__title">${isStep1 ? "选择设备模板" : "填写设备信息"}</span>
            <button class="modal-close wizard-dialog__close" @click=${this.closeWizard}>✕</button>
          </div>
          <!-- Body -->
          <div class="wizard-dialog__body">
            ${isStep1 ? this.renderWizardTemplateSelection() : this.renderWizardDeviceInfo()}
          </div>
          ${!isStep1 ? html`
            <div class="wizard-form-footer">
              <button class="btn btn--ghost" @click=${this.wizardBack}>上一步</button>
              <button class="btn btn--primary" ?disabled=${this.wizardSaving || !this.wizName.trim()} @click=${this.submitWizard}>
                ${this.wizardSaving ? "创建中..." : "创建设备"}
              </button>
            </div>
          ` : nothing}
        </div>
      </div>
    `;
  }

  renderWizardTemplateSelection() {
    const groups = this.wizardTemplatesByCategory;
    const categories = Object.keys(groups);

    return html`
      <!-- Search bar -->
      <div style="position: relative; margin-bottom: 20px;">
        <span style="position: absolute; left: 12px; top: 50%; transform: translateY(-50%); color: var(--muted);">
          ${icons.search}
        </span>
        <input
          type="text"
          placeholder="搜索设备模板..."
          .value=${this.wizTemplateSearch}
          @input=${(e: Event) => { this.wizTemplateSearch = (e.target as HTMLInputElement).value; }}
          style="width: 100%; padding: 10px 14px 10px 38px; box-sizing: border-box; border-radius: 10px; border: 1px solid var(--border); background: var(--bg-subtle); color: var(--text); font-size: 14px;"
        />
      </div>

      ${this.wizTemplateLoading ? html`
        <div style="display: flex; align-items: center; justify-content: center; padding: 60px;">
          <span class="loading-spinner"></span>
          <span style="margin-left: 8px; color: var(--muted);">加载中...</span>
        </div>
      ` : this.filteredWizardTemplates.length === 0 ? html`
        <div style="text-align: center; padding: 60px;">
          <div style="font-size: 48px; margin-bottom: 12px;">📦</div>
          <div style="font-size: 16px; font-weight: 500; color: var(--text);">没有找到匹配的模板</div>
          <div style="font-size: 13px; color: var(--muted); margin-top: 4px;">尝试调整搜索条件或浏览其他分类</div>
        </div>
      ` : html`
        ${categories.map(cat => html`
          <div style="margin-bottom: 28px;">
            <div style="display: flex; align-items: center; margin-bottom: 14px;">
              <span style="font-size: 16px; font-weight: 600;">${CATEGORY_LABELS[cat] || cat}</span>
              <span style="font-size: 12px; color: var(--muted); margin-left: 12px;">${groups[cat].length} 个模板</span>
            </div>
            <div class="wizard-template-grid">
              ${groups[cat].map(t => this.renderTemplateCard(t))}
            </div>
          </div>
        `)}
      `}
    `;
  }

  renderTemplateCard(t: ProcessedTemplate) {
    const displayName = getLocalizedText(t.displayName, t.name);
    return html`
      <div
        class="card"
        style="padding: 16px; cursor: pointer; transition: border-color 0.15s, box-shadow 0.15s;"
        @click=${() => this.selectTemplate(t)}
        @mouseenter=${(e: Event) => { (e.currentTarget as HTMLElement).style.borderColor = 'var(--primary, #3b82f6)'; (e.currentTarget as HTMLElement).style.boxShadow = '0 0 0 1px var(--primary, #3b82f6)'; }}
        @mouseleave=${(e: Event) => { (e.currentTarget as HTMLElement).style.borderColor = ''; (e.currentTarget as HTMLElement).style.boxShadow = ''; }}
      >
        <div style="display: flex; align-items: center; gap: 10px; margin-bottom: 10px;">
          <span style="font-size: 28px;">${CATEGORY_ICONS[t.category] || "📦"}</span>
          <div style="min-width: 0; flex: 1;">
            <div style="font-weight: 600; font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${displayName}</div>
            ${t.manufacturer ? html`<div style="font-size: 11px; color: var(--muted);">${t.manufacturer}</div>` : nothing}
          </div>
          ${t.isBuiltin ? html`<span style="font-size: 10px; padding: 1px 6px; border-radius: 4px; background: var(--bg-subtle); color: var(--muted); text-transform: uppercase;">内置</span>` : nothing}
        </div>
        <div style="display: flex; gap: 8px; font-size: 11px; color: var(--muted); flex-wrap: wrap;">
          ${t.deviceType ? html`<span>${t.deviceType}</span>` : nothing}
          ${t.protocolType ? html`<span>${t.protocolType}</span>` : nothing}
          ${t.version ? html`<span>v${t.version}</span>` : nothing}
        </div>
        <div style="display: flex; gap: 12px; font-size: 11px; color: var(--muted); margin-top: 8px;">
          <span>${t.properties.length} 属性</span>
          <span>${t.commands.length} 命令</span>
        </div>
      </div>
    `;
  }

  renderWizardDeviceInfo() {
    const t = this.wizSelectedTemplate;
    if (!t) return nothing;
    const displayName = getLocalizedText(t.displayName, t.name);
    const hasError = (name: string) => Boolean(this.wizValidationErrors[name]);
    const getError = (name: string) => this.wizValidationErrors[name] || "";

    return html`
      <div class="wizard-split">
        <!-- Left panel: form -->
        <div class="wizard-split__form">
          <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px;">
            <div style="font-size: 14px; font-weight: 600;">填写设备信息</div>
            <button class="btn btn--ghost" style="font-size: 12px; padding: 4px 10px;" @click=${this.wizardBack}>切换模板</button>
          </div>

          <!-- Template summary chip -->
          <div style="display: flex; align-items: center; gap: 12px; padding: 12px 14px; border: 1px solid var(--border); border-radius: 10px; background: var(--bg-subtle); margin-bottom: 16px;">
            <span style="font-size: 24px;">${CATEGORY_ICONS[t.category] || "📦"}</span>
            <div style="min-width: 0; flex: 1;">
              <div style="font-weight: 600; font-size: 14px;">${displayName}</div>
              <div style="font-size: 12px; color: var(--muted); margin-top: 2px;">
                ${t.manufacturer ? html`<span>${t.manufacturer} · </span>` : nothing}
                <span>${t.deviceType || t.category}</span>
                ${t.version ? html` · v${t.version}` : nothing}
              </div>
            </div>
            ${t.isBuiltin ? html`<span style="font-size: 10px; padding: 2px 8px; border-radius: 4px; background: var(--bg); color: var(--muted); text-transform: uppercase;">内置</span>` : nothing}
          </div>

          <!-- Device name -->
          <div class="field">
            <span>设备名称 <span style="color: var(--danger);">*</span></span>
            <input
              type="text"
              placeholder="请输入设备名称"
              .value=${this.wizName}
              @input=${(e: any) => { this.wizName = e.target.value; }}
              style=${hasError("deviceName") ? "border-color: var(--danger);" : ""}
            />
            ${hasError("deviceName") ? html`<div style="font-size: 12px; color: var(--danger); margin-top: 4px;">${getError("deviceName")}</div>` : nothing}
          </div>

          <!-- Device description -->
          <div class="field" style="margin-top: 12px;">
            <span>设备描述 <span style="font-size: 11px; color: var(--muted);">(可选)</span></span>
            <textarea
              placeholder="请输入设备描述"
              rows="2"
              .value=${this.wizDescription}
              @input=${(e: any) => { this.wizDescription = e.target.value; }}
              style="resize: none;"
            ></textarea>
          </div>

          <!-- Device address -->
          <div class="field" style="margin-top: 12px;">
            <span>设备地址 ${isFieldRequired(t.deviceInfo, "address")
              ? html`<span style="color: var(--danger);">*</span>`
              : html`<span style="font-size: 11px; color: var(--muted);">(可选)</span>`}</span>
            <input
              type="text"
              placeholder="请输入设备IP地址或连接地址"
              .value=${this.wizAddress}
              @input=${(e: any) => { this.wizAddress = e.target.value; }}
              style=${hasError("deviceAddress") ? "border-color: var(--danger);" : ""}
            />
            ${hasError("deviceAddress") ? html`<div style="font-size: 12px; color: var(--danger); margin-top: 4px;">${getError("deviceAddress")}</div>` : nothing}
          </div>

          <!-- Device position -->
          <div class="field" style="margin-top: 12px;">
            <span>安装位置 <span style="font-size: 11px; color: var(--muted);">(可选)</span></span>
            <input
              type="text"
              placeholder="请输入设备安装位置"
              .value=${this.wizPosition}
              @input=${(e: any) => { this.wizPosition = e.target.value; }}
            />
          </div>

          <!-- Driver select -->
          <div class="field" style="margin-top: 12px;">
            <span>设备驱动 <span style="font-size: 11px; color: var(--muted);">(选择适合的驱动程序)</span></span>
            <select .value=${this.wizDriver} @change=${(e: Event) => this.onWizardDriverSelect((e.target as HTMLSelectElement).value)}>
              <option value="">请选择驱动</option>
              ${this.driverNames.map(name => html`<option value=${name}>${name}</option>`)}
            </select>
            ${t.driverName && this.wizDriver !== t.driverName ? html`
              <div style="font-size: 11px; color: var(--muted); margin-top: 4px;">模板默认驱动: ${t.driverName}</div>
            ` : nothing}
          </div>

          <!-- Driver config -->
          ${this.wizDriver ? html`
            <div style="margin-top: 16px;">
              <div style="display: flex; align-items: center; gap: 6px; margin-bottom: 12px;">
                <span style="font-size: 14px; font-weight: 600;">驱动配置</span>
                <span style="font-size: 12px; color: var(--muted);">(${this.wizDriver})</span>
              </div>
              ${this.wizConfigLoading ? html`
                <div style="display: flex; align-items: center; justify-content: center; padding: 20px;">
                  <span class="loading-spinner"></span>
                  <span style="margin-left: 8px; color: var(--muted);">加载驱动配置参数...</span>
                </div>
              ` : this.wizConfigOptions.length > 0 ? html`
                ${this.wizConfigOptions.map(opt => this.renderWizardConfigField(opt))}
              ` : html`
                <div style="padding: 12px; border: 1px solid var(--border); border-radius: 8px; color: var(--muted); font-size: 13px; text-align: center;">
                  该驱动无需额外配置参数
                </div>
              `}
            </div>
          ` : nothing}
        </div>

        <!-- Right panel: template overview -->
        <div class="wizard-split__overview">
          ${this.renderTemplateOverview(t)}
        </div>
      </div>
    `;
  }

  renderWizardConfigField(opt: DriverConfigOption) {
    const value = this.wizDriverConfig[opt.name] ?? "";
    const hasError = Boolean(this.wizValidationErrors[`driverConfig.${opt.name}`]);
    const errorMsg = this.wizValidationErrors[`driverConfig.${opt.name}`] || "";
    const placeholder = opt.defaultValue ? `默认: ${opt.defaultValue}` : `请输入${opt.label}`;

    return html`
      <div class="field" style="margin-bottom: 10px;">
        <span>
          ${opt.label}
          ${opt.required ? html`<span style="color: var(--danger);">*</span>` : html`<span style="font-size: 11px; color: var(--muted);">(可选)</span>`}
          ${opt.defaultValue ? html`<span style="font-size: 11px; color: var(--muted); margin-left: 8px;">· 默认: ${opt.defaultValue}</span>` : nothing}
        </span>
        ${opt.optionType === "boolean" ? html`
          <select .value=${value || (opt.defaultValue === "true" ? "true" : "false")} @change=${(e: Event) => {
            this.wizDriverConfig = { ...this.wizDriverConfig, [opt.name]: (e.target as HTMLSelectElement).value };
          }}>
            <option value="">请选择</option>
            <option value="true">是</option>
            <option value="false">否</option>
          </select>
        ` : opt.optionType === "number" ? html`
          <input type="number" .value=${value} placeholder=${placeholder} @input=${(e: any) => {
            this.wizDriverConfig = { ...this.wizDriverConfig, [opt.name]: e.target.value };
          }} style=${hasError ? "border-color: var(--danger);" : ""} />
        ` : html`
          <input type="text" .value=${value} placeholder=${placeholder} @input=${(e: any) => {
            this.wizDriverConfig = { ...this.wizDriverConfig, [opt.name]: e.target.value };
          }} style=${hasError ? "border-color: var(--danger);" : ""} />
        `}
        ${hasError ? html`<div style="font-size: 12px; color: var(--danger); margin-top: 4px;">${errorMsg}</div>` : nothing}
      </div>
    `;
  }

  renderTemplateOverview(t: ProcessedTemplate) {
    const displayName = getLocalizedText(t.displayName, t.name);
    const description = getLocalizedText(t.description ?? undefined, "");

    // Compute stats from template properties
    const totalProps = t.properties.length;
    const totalCmds = t.commands.length;
    const readonlyProps = t.properties.filter((p: any) => p.accessMode === "r" || p.accessMode === "R").length;
    const writableProps = totalProps - readonlyProps;

    return html`
      <!-- Template summary -->
      <div style="display: flex; align-items: center; gap: 12px; margin-bottom: 16px;">
        <span style="font-size: 32px;">${CATEGORY_ICONS[t.category] || "📦"}</span>
        <div style="min-width: 0; flex: 1;">
          <div style="font-weight: 600; font-size: 16px;">${displayName}</div>
          <div style="font-size: 12px; color: var(--muted); margin-top: 2px;">
            ${t.manufacturer ? html`${t.manufacturer} · ` : nothing}${t.deviceType || t.category}${t.version ? html` · v${t.version}` : nothing}
          </div>
        </div>
        ${t.isBuiltin ? html`<span style="font-size: 10px; padding: 2px 8px; border-radius: 4px; background: var(--bg); color: var(--muted); text-transform: uppercase;">内置</span>` : nothing}
      </div>

      <!-- Description -->
      ${description ? html`
        <div style="font-size: 13px; color: var(--muted); line-height: 1.5; margin-bottom: 16px; padding: 10px 12px; background: var(--bg); border-radius: 8px;">
          ${description}
        </div>
      ` : nothing}

      <!-- Meta info -->
      <div style="display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 16px;">
        ${t.protocolType ? html`<span style="font-size: 11px; padding: 3px 8px; border-radius: 6px; background: var(--bg); color: var(--muted);">协议: ${t.protocolType}</span>` : nothing}
        ${t.driverName ? html`<span style="font-size: 11px; padding: 3px 8px; border-radius: 6px; background: var(--bg); color: var(--muted);">驱动: ${t.driverName}</span>` : nothing}
        ${t.category ? html`<span style="font-size: 11px; padding: 3px 8px; border-radius: 6px; background: var(--bg); color: var(--muted);">${CATEGORY_LABELS[t.category] || t.category}</span>` : nothing}
      </div>

      <!-- Tags -->
      ${t.tags && t.tags.length > 0 ? html`
        <div style="display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 16px;">
          ${t.tags.map(tag => html`<span style="font-size: 11px; padding: 2px 8px; border-radius: 4px; background: var(--primary, #3b82f6); color: white; opacity: 0.8;">${tag}</span>`)}
        </div>
      ` : nothing}

      <!-- Stats grid 2x2 -->
      <div class="wizard-overview__stats">
        <div class="wizard-overview__stat">
          <div class="wizard-overview__stat-value">${totalProps}</div>
          <div class="wizard-overview__stat-label">属性数</div>
        </div>
        <div class="wizard-overview__stat">
          <div class="wizard-overview__stat-value">${totalCmds}</div>
          <div class="wizard-overview__stat-label">命令数</div>
        </div>
        <div class="wizard-overview__stat">
          <div class="wizard-overview__stat-value">${readonlyProps}</div>
          <div class="wizard-overview__stat-label">只读属性</div>
        </div>
        <div class="wizard-overview__stat">
          <div class="wizard-overview__stat-value">${writableProps}</div>
          <div class="wizard-overview__stat-label">可写属性</div>
        </div>
      </div>

      <!-- Property list -->
      ${totalProps > 0 ? html`
        <div class="wizard-overview__section-title">属性列表</div>
        <ul class="wizard-overview__list" style="max-height: 240px; overflow-y: auto;">
          ${t.properties.map((p: any) => html`
            <li class="wizard-overview__list-item" style="flex-wrap: wrap; gap: 4px;">
              <div style="display: flex; align-items: center; gap: 6px; flex: 1; min-width: 0;">
                <span class="wizard-overview__list-item-name">${p.name || p.displayName || "unnamed"}</span>
                ${p.accessMode === "r" || p.accessMode === "R"
                  ? html`<span style="font-size: 10px; padding: 1px 5px; border-radius: 3px; background: var(--bg-subtle); color: var(--muted);">R</span>`
                  : html`<span style="font-size: 10px; padding: 1px 5px; border-radius: 3px; background: var(--primary, #3b82f6); color: white; opacity: 0.7;">RW</span>`
                }
              </div>
              <span class="wizard-overview__list-item-meta">
                ${p.dataType || ""}${p.unit ? ` ${p.unit}` : ""}
                ${p.minValue != null || p.maxValue != null
                  ? html` <span style="opacity: 0.7;">[${p.minValue ?? '–'}~${p.maxValue ?? '–'}]</span>`
                  : nothing
                }
              </span>
            </li>
          `)}
        </ul>
      ` : nothing}

      <!-- Command list -->
      ${totalCmds > 0 ? html`
        <div class="wizard-overview__section-title">命令列表</div>
        <ul class="wizard-overview__list" style="max-height: 200px; overflow-y: auto;">
          ${t.commands.map((c: any) => html`
            <li class="wizard-overview__list-item" style="flex-wrap: wrap; gap: 4px;">
              <div style="display: flex; align-items: center; gap: 6px; flex: 1; min-width: 0;">
                <span class="wizard-overview__list-item-name">${c.name || "unnamed"}</span>
                ${c.parameters && c.parameters.length > 0
                  ? html`<span style="font-size: 10px; color: var(--muted);">${c.parameters.length} 参数</span>`
                  : nothing
                }
              </div>
              <span class="wizard-overview__list-item-meta">${c.description || ""}</span>
            </li>
          `)}
        </ul>
      ` : nothing}

      ${totalProps === 0 && totalCmds === 0 ? html`
        <div style="text-align: center; padding: 24px; color: var(--muted); font-size: 13px;">
          该模板暂无属性和命令定义
        </div>
      ` : nothing}
    `;
  }
}
