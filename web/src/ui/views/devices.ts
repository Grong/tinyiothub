import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { deviceApi } from "../../api/devices.js";
import { driverApi } from "../../api/drivers.js";
import { templateApi } from "../../api/templates.js";
import { tagApi } from "../../api/tags.js";
import { API_BASE } from "../../api/config.js";
import type { Device, DeviceProfile, DeviceProperty, CreateDeviceRequest, DriverConfigOption, Tag, Template } from "../../types/index.js";
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
export class DevicesView extends LitElement {
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

  // SSE
  private eventSource: EventSource | null = null;

  // Tags
  @state() allTags: Tag[] = [];
  @state() editingTagsDeviceId: string | null = null;
  @state() tagSearchKeyword = "";
  @state() tagSaving = false;
  private _boundCloseTagEditor = () => { this.editingTagsDeviceId = null; };

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    const path = window.location.pathname;
    if (path.startsWith("/devices/")) {
      const id = path.split("/")[2];
      if (id) {
        this.loadDeviceDetail(id);
        return;
      }
    }
    this.loadDevices();
    this.loadDriverNames();
    this.loadAllTags();
    this.connectSSE();
    document.addEventListener("click", this._boundCloseTagEditor);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.disconnectSSE();
    document.removeEventListener("click", this._boundCloseTagEditor);
  }

  // === SSE ===

  connectSSE() {
    this.disconnectSSE();
    const token = sessionStorage.getItem("auth-token") || localStorage.getItem("auth-token");
    if (!token) return;
    const url = `${API_BASE}/events/sse?token=${encodeURIComponent(token)}&event_types=device.status_change,device.connection`;
    try {
      this.eventSource = new EventSource(url);
      this.eventSource.onmessage = (ev) => {
        try {
          const data = JSON.parse(ev.data);
          if (data.event_type === "device.status_change" || data.event_type === "device.connection") {
            this.handleDeviceStatusEvent(data);
          }
        } catch {
          // ignore non-JSON pings
        }
      };
      this.eventSource.onerror = () => {
        this.disconnectSSE();
        setTimeout(() => this.connectSSE(), 5000);
      };
    } catch {
      // SSE not supported
    }
  }

  disconnectSSE() {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
  }

  handleDeviceStatusEvent(data: any) {
    const deviceId = data.device_id || data.deviceId;
    if (!deviceId) return;
    const idx = this.devices.findIndex((d) => d.id === deviceId);
    if (idx >= 0) {
      const newStatus = data.status || data.new_status;
      if (newStatus) {
        this.devices = this.devices.map((d, i) =>
          i === idx ? { ...d, status: newStatus } : d
        );
      }
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
      if (data && Array.isArray(data.drivers)) {
        this.driverNames = data.drivers.map((d: any) => d.name || d);
      } else if (Array.isArray(data)) {
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
      this.allTags = res.result || [];
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
        await tagApi.createBinding({ tagId: tag.id, targetId: device.id });
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
    window.history.pushState({}, "", "/devices");
    window.dispatchEvent(new PopStateEvent("popstate"));
    this.loadDevices();
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
      <div style="display: flex; gap: 12px; margin-bottom: 16px; align-items: center; flex-wrap: wrap;">
        <input
          type="text"
          placeholder="搜索设备名称..."
          .value=${this.searchName}
          @input=${(e: Event) => { this.searchName = (e.target as HTMLInputElement).value; }}
          @keydown=${(e: KeyboardEvent) => { if (e.key === "Enter") { this.page = 1; this.loadDevices(); } }}
          style="flex: 1; min-width: 180px; max-width: 300px;"
        />
        <select .value=${this.filterStatus} @change=${(e: Event) => { this.filterStatus = (e.target as HTMLSelectElement).value; this.page = 1; this.loadDevices(); }}>
          <option value="">全部状态</option>
          <option value="online">在线</option>
          <option value="offline">离线</option>
          <option value="error">故障</option>
          <option value="maintenance">维护</option>
        </select>
        <select .value=${this.filterProtocol} @change=${(e: Event) => { this.filterProtocol = (e.target as HTMLSelectElement).value; this.page = 1; this.loadDevices(); }}>
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
    const isEditingTags = this.editingTagsDeviceId === d.id;
    return html`
      <div style="position: relative;">
        <div class="card" style="overflow: hidden;">
          <!-- Header -->
          <div style="padding: 14px 16px 0; display: flex; align-items: center; justify-content: space-between;">
            <div style="display: flex; align-items: center; gap: 8px; min-width: 0;">
              <span style="width: 8px; height: 8px; border-radius: 50%; background: ${this.statusColor(d.status)}; flex-shrink: 0;"></span>
              <span style="font-weight: 600; font-size: 15px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${d.displayName || d.name}</span>
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

          <!-- Info -->
          <div
            style="padding: 10px 16px; cursor: pointer;"
            @click=${() => this.navigateToDevice(d.id)}
          >
            <div style="display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: var(--muted);">
              ${d.deviceType ? html`<div>${d.deviceType}</div>` : nothing}
              ${d.protocolType || d.driverName ? html`<div>${d.protocolType || d.driverName}</div>` : nothing}
              ${d.address ? html`<div>${d.address}</div>` : nothing}
            </div>
          </div>

          <!-- Tags -->
          <div style="padding: 0 16px 8px; display: flex; flex-wrap: wrap; gap: 4px; align-items: center;">
            ${deviceTags.map(t => html`
              <span style="display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border-radius: 9999px; font-size: 11px; background: var(--bg-subtle); border: 1px solid var(--border);">
                <span style="width: 6px; height: 6px; border-radius: 50%; background: ${t.color || 'var(--primary, #3b82f6)'};"></span>
                ${t.name}
              </span>
            `)}
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

    return html`
      <button class="btn btn--ghost" @click=${this.backToList} style="margin-bottom: 16px;">
        &larr; 返回设备列表
      </button>
      <div class="card" style="padding: 24px; margin-bottom: 16px;">
        <div style="display: flex; justify-content: space-between; align-items: flex-start;">
          <div>
            <h2 style="margin: 0 0 8px; font-size: 20px;">${d.displayName || d.name}</h2>
            <div style="font-size: 13px; color: var(--muted); display: flex; gap: 16px; flex-wrap: wrap;">
              <span>类型: ${d.deviceType || "-"}</span>
              <span>协议: ${d.protocolType || d.driverName || "-"}</span>
              <span>厂商: ${d.factoryName || "-"}</span>
            </div>
          </div>
          <div style="display: flex; align-items: center; gap: 8px;">
            <span style="display: inline-flex; align-items: center; gap: 6px; padding: 4px 12px; border-radius: 9999px; font-size: 13px; background: var(--bg-subtle);">
              <span style="width: 8px; height: 8px; border-radius: 50%; background: ${this.statusColor(d.status)};"></span>
              ${this.statusLabel(d.status)}
            </span>
            <button class="btn btn--ghost btn--sm" @click=${() => this.openEdit(d)}>编辑</button>
          </div>
        </div>
      </div>

      <div style="display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; margin-bottom: 16px;">
        <div class="card" style="padding: 16px;">
          <div style="color: var(--muted); font-size: 12px;">属性总数</div>
          <div style="font-size: 24px; font-weight: 700; margin-top: 4px;">${ov.totalProperties}</div>
        </div>
        <div class="card" style="padding: 16px;">
          <div style="color: var(--muted); font-size: 12px;">在线属性</div>
          <div style="font-size: 24px; font-weight: 700; margin-top: 4px; color: var(--success);">${ov.onlineProperties}</div>
        </div>
        <div class="card" style="padding: 16px;">
          <div style="color: var(--muted); font-size: 12px;">命令数</div>
          <div style="font-size: 24px; font-weight: 700; margin-top: 4px;">${ov.totalCommands}</div>
        </div>
        <div class="card" style="padding: 16px;">
          <div style="color: var(--muted); font-size: 12px;">活跃告警</div>
          <div style="font-size: 24px; font-weight: 700; margin-top: 4px; color: ${ov.activeAlarms > 0 ? 'var(--danger)' : 'inherit'};">${ov.activeAlarms}</div>
        </div>
      </div>

      ${profile.properties.length > 0 ? html`
        <div class="card" style="padding: 20px; margin-bottom: 16px;">
          <div style="font-weight: 600; margin-bottom: 12px;">设备属性</div>
          <table style="width: 100%; border-collapse: collapse;">
            <thead>
              <tr style="border-bottom: 1px solid var(--border);">
                <th style="padding: 8px 12px; text-align: left; font-size: 12px; color: var(--muted);">属性名</th>
                <th style="padding: 8px 12px; text-align: left; font-size: 12px; color: var(--muted);">当前值</th>
                <th style="padding: 8px 12px; text-align: left; font-size: 12px; color: var(--muted);">数据类型</th>
                <th style="padding: 8px 12px; text-align: left; font-size: 12px; color: var(--muted);">单位</th>
                <th style="padding: 8px 12px; text-align: left; font-size: 12px; color: var(--muted);">更新时间</th>
              </tr>
            </thead>
            <tbody>
              ${profile.properties.map((p: DeviceProperty) => html`
                <tr style="border-bottom: 1px solid var(--border);">
                  <td style="padding: 8px 12px; font-size: 13px;">${p.displayName || p.name}</td>
                  <td style="padding: 8px 12px; font-size: 13px; font-weight: 500;">${p.currentValue ?? p.value ?? "-"}</td>
                  <td style="padding: 8px 12px; font-size: 13px; color: var(--muted);">${p.dataType}</td>
                  <td style="padding: 8px 12px; font-size: 13px; color: var(--muted);">${p.unit || "-"}</td>
                  <td style="padding: 8px 12px; font-size: 13px; color: var(--muted);">${p.updatedAt?.slice(0, 16) || "-"}</td>
                </tr>
              `)}
            </tbody>
          </table>
        </div>
      ` : nothing}

      ${profile.commands.length > 0 ? html`
        <div class="card" style="padding: 20px;">
          <div style="font-weight: 600; margin-bottom: 12px;">设备命令</div>
          <div style="display: flex; flex-direction: column; gap: 8px;">
            ${profile.commands.map(c => html`
              <div style="display: flex; align-items: center; justify-content: space-between; padding: 12px; background: var(--bg-subtle); border-radius: 8px;">
                <div>
                  <div style="font-weight: 500; font-size: 14px;">${c.name}</div>
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
      ` : nothing}
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
      <div class="wizard-fullscreen">
        <!-- Header bar -->
        <div class="wizard-fullscreen__header">
          <button class="wizard-fullscreen__back" @click=${isStep1 ? this.closeWizard : this.wizardBack}>
            <span style="transform: rotate(90deg); display: inline-flex;">${icons.arrowDown}</span>
            <span>${isStep1 ? "返回设备列表" : "返回模板选择"}</span>
          </button>
          <span class="wizard-fullscreen__title">${isStep1 ? "选择设备模板" : "填写设备信息"}</span>
          <div class="wizard-fullscreen__steps">
            <div class="wizard-fullscreen__dot ${isStep1 ? 'wizard-fullscreen__dot--active' : 'wizard-fullscreen__dot--done'}"></div>
            <div class="wizard-fullscreen__dot ${!isStep1 ? 'wizard-fullscreen__dot--active' : ''}"></div>
          </div>
          ${isStep1 ? html`
            <button class="btn btn--ghost" @click=${this.closeWizard} style="margin-left: 8px;">取消</button>
          ` : nothing}
        </div>
        <!-- Body -->
        <div class="wizard-fullscreen__body">
          ${isStep1 ? this.renderWizardTemplateSelection() : this.renderWizardDeviceInfo()}
        </div>
      </div>
    `;
  }

  renderWizardTemplateSelection() {
    const groups = this.wizardTemplatesByCategory;
    const categories = Object.keys(groups);

    return html`
      <p style="text-align: center; color: var(--muted); font-size: 14px; margin: 0 0 20px;">
        选择一个设备模板来快速创建和配置您的IoT设备
      </p>
      <!-- Search bar centered, max 640px -->
      <div style="display: flex; justify-content: center; margin-bottom: 24px;">
        <div style="position: relative; width: 100%; max-width: 640px;">
          <span style="position: absolute; left: 12px; top: 50%; transform: translateY(-50%); color: var(--muted);">
            ${icons.search}
          </span>
          <input
            type="text"
            placeholder="搜索设备模板..."
            .value=${this.wizTemplateSearch}
            @input=${(e: Event) => { this.wizTemplateSearch = (e.target as HTMLInputElement).value; }}
            style="width: 100%; padding: 10px 14px 10px 38px; box-sizing: border-box; border-radius: 10px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 14px;"
          />
        </div>
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
      <div style="padding: 20px 24px;">
        <div style="font-size: 14px; font-weight: 600; margin-bottom: 16px;">填写设备信息</div>

        <!-- Template summary card -->
        <div style="display: flex; align-items: center; gap: 12px; padding: 14px; border: 1px solid var(--border); border-radius: 10px; background: var(--bg-subtle); margin-bottom: 16px;">
          <span style="font-size: 28px;">${CATEGORY_ICONS[t.category] || "📦"}</span>
          <div style="min-width: 0; flex: 1;">
            <div style="font-weight: 600; font-size: 15px;">${displayName}</div>
            <div style="font-size: 12px; color: var(--muted); margin-top: 2px;">
              ${t.manufacturer ? html`<span>${t.manufacturer} · </span>` : nothing}
              <span>${t.deviceType || t.category}</span>
              ${t.version ? html` · v${t.version}` : nothing}
              ${this.wizDriver ? html` · ${this.wizDriver}` : nothing}
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
}
