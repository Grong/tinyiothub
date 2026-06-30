import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { SignalWatcher } from "@lit-labs/signals";
import { deviceApi } from "../../api/devices.js";
import { driverApi } from "../../api/drivers.js";
import { templateApi } from "../../api/templates.js";
import { tagApi } from "../../api/tags.js";
import { eventApi } from "../../api/events.js";
import { alarmApi } from "../../api/alarms.js";
import { deviceCache } from "../../stores/device-cache.js";
import { i18n, t } from "../../i18n/index.js";
import type { Device, DeviceProfile, DeviceProperty, CreateDeviceRequest, DriverConfigOption, Tag, DeviceEvent } from "../../types/index.js";
import type { AlarmRule, AlarmLevel, RuleType, AlarmCondition, ComparisonOperator, ChangeType, LogicalOperator, NotificationChannelType, CreateAlarmRuleRequest, UpdateAlarmRuleRequest } from "../../types/index.js";
import { success, error as toastError } from "../components/toast.js";
import { icons } from "../icons.js";
import "./gateway-pairing.js";

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
  const locale = i18n.getLocale();
  if (locale.startsWith("zh")) {
    return obj["zh"] || obj["en"] || Object.values(obj)[0] || fallback;
  }
  return obj["en"] || obj["zh"] || Object.values(obj)[0] || fallback;
}

const CATEGORY_LABELS: Record<string, string> = {
  sensors: "传感器",
  controllers: "控制器",
  cameras: "摄像头",
  gateways: "网关",
  others: "其他",
};

const CATEGORY_ICONS: Record<string, ReturnType<typeof html>> = {
  sensors: html`
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="24" height="24">
      <path d="M14 4v10.54a4 4 0 1 1-4 0V4a2 2 0 0 1 4 0Z" />
    </svg>
  `,
  controllers: html`
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="24" height="24">
      <line x1="4" x2="4" y1="21" y2="14" />
      <line x1="4" x2="4" y1="10" y2="3" />
      <line x1="12" x2="12" y1="21" y2="12" />
      <line x1="12" x2="12" y1="8" y2="3" />
      <line x1="20" x2="20" y1="21" y2="16" />
      <line x1="20" x2="20" y1="12" y2="3" />
      <line x1="1" x2="7" y1="14" y2="14" />
      <line x1="9" x2="15" y1="8" y2="8" />
      <line x1="17" x2="23" y1="16" y2="16" />
    </svg>
  `,
  cameras: html`
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="24" height="24">
      <path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z" />
      <circle cx="12" cy="13" r="3" />
    </svg>
  `,
  gateways: html`
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="24" height="24">
      <rect x="3" y="3" width="18" height="12" rx="2" />
      <line x1="8" x2="8" y1="21" y2="15" />
      <line x1="16" x2="16" y1="21" y2="15" />
      <line x1="12" x2="12" y1="21" y2="15" />
      <circle cx="8" cy="9" r="1" fill="currentColor" />
      <circle cx="16" cy="9" r="1" fill="currentColor" />
    </svg>
  `,
  others: html`
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="24" height="24">
      <path d="m7.5 4.27 9 5.15" />
      <path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l2.53-1.45" />
      <line x1="1" x2="23" y1="1" y2="23" />
    </svg>
  `,
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
  @state() showPairingDialog = false;
  @state() saving = false;
  @state() formName = "";
  @state() formType = "";
  @state() formAddress = "";
  @state() formDescription = "";
  @state() formManufacturer = "";
  @state() formModel = "";
  @state() formProtocol = "";
  @state() formPosition = "";
  @state() formPort = "";
  @state() formDriver = "";
  @state() formDriverConfig: Record<string, string> = {};
  @state() formDriverConfigOptions: DriverConfigOption[] = [];
  @state() formDriverConfigLoading = false;
  @state() formProperties: { name: string; displayName?: string; value: any; dataType: string; unit?: string; isReadOnly?: boolean; minValue?: number; maxValue?: number; description?: string }[] = [];
  @state() formModalTab: 'basic' | 'driver' | 'properties' | 'commands' = 'basic';
  @state() formCommands: { name: string; description?: string; parameters?: string }[] = [];
  @state() formProfileLoading = false;

  // Alarm rule management
  @state() alarmRules: AlarmRule[] = [];
  @state() rulesLoading = false;
  @state() showRuleModal = false;
  @state() editingRule: AlarmRule | null = null;
  @state() ruleSaving = false;
  // Device alarm list
  @state() deviceAlarms: import("../../types/index.js").Alarm[] = [];
  @state() alarmsLoading = false;
  // Rule form
  @state() ruleFormName = "";
  @state() ruleFormDesc = "";
  @state() ruleFormPropertyId = "";
  @state() ruleFormType: RuleType = "threshold";
  @state() ruleFormLevel: AlarmLevel = "Warning";
  @state() ruleFormCondition: AlarmCondition = { type: "threshold", operator: "greater_than", value: 0 };
  // Threshold/range
  @state() ruleFormOperator: ComparisonOperator = "greater_than";
  @state() ruleFormValue = 0;
  @state() ruleFormMin = 0;
  @state() ruleFormMax = 100;
  // Change
  @state() ruleFormChangeType: ChangeType = "any";
  @state() ruleFormChangeThreshold = 10;
  @state() ruleFormChangeWindow = 300;
  // Composite
  @state() ruleFormLogicOp: LogicalOperator = "and";
  @state() ruleCompositeConditions: AlarmCondition[] = [];
  // Notification
  @state() ruleFormNotifyEnabled = false;
  @state() ruleFormNotifyChannels: NotificationChannelType[] = [];
  @state() ruleFormNotifyRecipients = "";

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
  @state() tagCreating = false;
  private _boundCloseTagEditor = () => { this.editingTagsDeviceId = null; };
  private _unsubI18n?: () => void;

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
  private _boundHandleDeviceUpdated: EventListener = () => {};

  // Focus management for modals
  private modalLastFocus?: Element;
  private historyLastFocus?: Element;
  private wizardLastFocus?: Element;

  private handleModalKeydown(e: KeyboardEvent, closeFn: () => void) {
    if (e.key === "Escape") {
      e.preventDefault();
      closeFn();
      return;
    }
    if (e.key !== "Tab") return;
    const container = e.currentTarget as HTMLElement;
    if (!container) return;
    const focusables = Array.from(
      container.querySelectorAll<HTMLElement>(
        'a[href], button, textarea, input:not([type="hidden"]), select, [tabindex]:not([tabindex="-1"])'
      )
    ).filter(el => !el.hasAttribute("disabled") && (el as HTMLElement).offsetParent !== null);
    if (focusables.length === 0) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (e.shiftKey) {
      if (document.activeElement === first) {
        e.preventDefault();
        last.focus();
      }
    } else {
      if (document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  private focusFirst(container: HTMLElement, delay = 0) {
    setTimeout(() => {
      const el = container.querySelector<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      el?.focus();
    }, delay);
  }

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
    this._unsubI18n = i18n.subscribe(() => this.requestUpdate());
    document.addEventListener("click", this._boundCloseTagEditor);
    // SSE 推送时刷新当前分页数据
    this._boundHandleDeviceUpdated = () => {
      if (!this.selectedDevice) {
        this.loadDevices();
      }
    };
    document.addEventListener("device-updated", this._boundHandleDeviceUpdated);
    const path = window.location.pathname;
    if (path.startsWith("/devices/")) {
      const id = path.split("/")[2];
      if (id) {
        this.loadDeviceDetail(id);
        return;
      }
    }
    // 分页加载设备列表（SSE 缓存在进入详情页时按需初始化）
    this.loadDevices();
    this.loadDriverNames();
    this.loadAllTags();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this._unsubI18n?.();
    // 不断开 SSE — 缓存层管理连接生命周期
    document.removeEventListener("click", this._boundCloseTagEditor);
    document.removeEventListener("device-updated", this._boundHandleDeviceUpdated);
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
        this.totalPages = data.pagination?.totalPages || (this.devices.length > 0 ? 1 : 0);
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
      // 触发 deviceCache 初始化（建立 SSE 连接），同时获取详情
      const [profile] = await Promise.all([
        deviceApi.getDeviceProfile(id),
        deviceCache.getDevices(),
      ]);
      const result = profile.result || null;

      // 将属性存入缓存，SSE 推送时只更新 currentValue
      if (result?.properties?.length) {
        deviceCache.setDeviceProperties(id, result.properties);
      }

      this.selectedDevice = result;
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

  async createAndBindTag(device: Device, name: string) {
    if (this.tagCreating || !name.trim()) return;
    this.tagCreating = true;
    try {
      // Pick a color for the new tag
      const colors = ['#3b82f6', '#ef4444', '#10b981', '#f59e0b', '#8b5cf6', '#ec4899', '#06b6d4', '#84cc16'];
      const color = colors[Math.floor(Math.random() * colors.length)];

      const res = await tagApi.createTag({ name: name.trim(), type: 'device', color });
      const newTag = res.result as Tag;
      if (newTag?.id) {
        // Bind to device
        await tagApi.createBinding({ tagId: newTag.id, targetId: device.id, targetType: 'device' });
        // Refresh tag list and devices
        await Promise.all([this.loadAllTags(), this.loadDevices()]);
        this.tagSearchKeyword = '';
        success(`已创建并绑定标签「${name.trim()}」`);
      }
    } catch (err: any) {
      toastError(err.message || '创建标签失败');
    } finally {
      this.tagCreating = false;
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
    if (key === "alarms") {
      this.loadAlarmRules();
      this.loadDeviceAlarms();
    }
  }

  async loadDeviceAlarms() {
    const deviceId = this.selectedDevice?.device?.id;
    if (!deviceId) return;
    this.alarmsLoading = true;
    try {
      const res = await alarmApi.getAlarms({ statuses: ["active"], page: 1, pageSize: 50 });
      const alarmData = res.result as any;
      const allAlarms = alarmData?.data || [];
      this.deviceAlarms = allAlarms.filter((a: any) => a.deviceId === deviceId);
    } catch {
      this.deviceAlarms = [];
    } finally {
      this.alarmsLoading = false;
    }
  }

  // ===== Alarm Rule Management =====

  async loadAlarmRules() {
    const deviceId = this.selectedDevice?.device?.id;
    if (!deviceId) return;
    this.rulesLoading = true;
    try {
      const res = await alarmApi.getRules({ deviceId });
      const data = res.result;
      this.alarmRules = Array.isArray(data) ? data as AlarmRule[] : [];
    } catch {
      this.alarmRules = [];
    } finally {
      this.rulesLoading = false;
    }
  }

  openNewRule() {
    this.editingRule = null;
    this.resetRuleForm();
    this.showRuleModal = true;
  }

  openEditRule(rule: AlarmRule) {
    this.editingRule = rule;
    this.ruleFormName = rule.name;
    this.ruleFormDesc = rule.description || "";
    this.ruleFormPropertyId = rule.propertyId || "";
    this.ruleFormType = rule.ruleType as RuleType;
    this.ruleFormLevel = rule.alarmLevel as AlarmLevel;
    this.ruleFormCondition = rule.condition;
    const cond = rule.condition;
    if (cond.type === "threshold") {
      this.ruleFormOperator = cond.operator;
      this.ruleFormValue = cond.value;
    } else if (cond.type === "range") {
      this.ruleFormMin = cond.min ?? 0;
      this.ruleFormMax = cond.max ?? 100;
    } else if (cond.type === "change") {
      this.ruleFormChangeType = cond.changeType;
      this.ruleFormChangeThreshold = cond.threshold;
      this.ruleFormChangeWindow = cond.timeWindow;
    } else if (cond.type === "composite") {
      this.ruleFormLogicOp = cond.operator;
      this.ruleCompositeConditions = [...cond.conditions];
    }
    this.ruleFormNotifyEnabled = rule.notificationConfig?.enabled ?? false;
    this.ruleFormNotifyChannels = [...(rule.notificationConfig?.channels ?? [])];
    this.ruleFormNotifyRecipients = (rule.notificationConfig?.recipients ?? []).join(", ");
    this.showRuleModal = true;
  }

  resetRuleForm() {
    this.ruleFormName = "";
    this.ruleFormDesc = "";
    this.ruleFormPropertyId = "";
    this.ruleFormType = "threshold";
    this.ruleFormLevel = "Warning";
    this.ruleFormOperator = "greater_than";
    this.ruleFormValue = 0;
    this.ruleFormMin = 0;
    this.ruleFormMax = 100;
    this.ruleFormChangeType = "any";
    this.ruleFormChangeThreshold = 10;
    this.ruleFormChangeWindow = 300;
    this.ruleFormLogicOp = "and";
    this.ruleCompositeConditions = [];
    this.ruleFormNotifyEnabled = false;
    this.ruleFormNotifyChannels = [];
    this.ruleFormNotifyRecipients = "";
  }

  closeRuleModal() {
    this.showRuleModal = false;
    this.editingRule = null;
  }

  buildCondition(): AlarmCondition {
    switch (this.ruleFormType) {
      case "threshold":
        return { type: "threshold", operator: this.ruleFormOperator, value: this.ruleFormValue };
      case "range":
        return { type: "range", min: this.ruleFormMin, max: this.ruleFormMax, inclusive: true };
      case "change":
        return { type: "change", changeType: this.ruleFormChangeType, threshold: this.ruleFormChangeThreshold, timeWindow: this.ruleFormChangeWindow };
      case "composite":
        return { type: "composite", operator: this.ruleFormLogicOp, conditions: this.ruleCompositeConditions };
      default:
        return { type: "threshold", operator: "greater_than", value: 0 };
    }
  }

  async saveRule() {
    if (!this.ruleFormName.trim()) {
      toastError("请输入规则名称");
      return;
    }
    const deviceId = this.selectedDevice?.device?.id;
    if (!deviceId) return;

    this.ruleSaving = true;
    try {
      const condition = this.buildCondition();
      const notificationConfig = {
        enabled: this.ruleFormNotifyEnabled,
        channels: this.ruleFormNotifyChannels,
        recipients: this.ruleFormNotifyRecipients.split(",").map(s => s.trim()).filter(Boolean),
      };

      if (this.editingRule) {
        const updateReq: UpdateAlarmRuleRequest = {
          name: this.ruleFormName,
          description: this.ruleFormDesc || undefined,
          propertyId: this.ruleFormPropertyId || undefined,
          condition,
          alarmLevel: this.ruleFormLevel,
          notificationConfig,
        };
        await alarmApi.updateRule(this.editingRule.id, updateReq);
        success("规则已更新");
      } else {
        const createReq: CreateAlarmRuleRequest = {
          name: this.ruleFormName,
          description: this.ruleFormDesc || undefined,
          deviceId,
          propertyId: this.ruleFormPropertyId || undefined,
          ruleType: this.ruleFormType,
          condition,
          alarmLevel: this.ruleFormLevel,
          notificationConfig,
        };
        await alarmApi.createRule(createReq);
        success("规则已创建");
      }
      this.closeRuleModal();
      await this.loadAlarmRules();
    } catch (err: any) {
      toastError(err.message || "保存规则失败");
    } finally {
      this.ruleSaving = false;
    }
  }

  async deleteRule(rule: AlarmRule) {
    if (!confirm(`确定删除规则「${rule.name}」吗？`)) return;
    try {
      await alarmApi.deleteRule(rule.id);
      success("规则已删除");
      await this.loadAlarmRules();
    } catch (err: any) {
      toastError(err.message || "删除失败");
    }
  }

  async toggleRule(rule: AlarmRule) {
    try {
      await alarmApi.toggleRule(rule.id, !rule.isEnabled);
      success(rule.isEnabled ? "规则已禁用" : "规则已启用");
      await this.loadAlarmRules();
    } catch (err: any) {
      toastError(err.message || "操作失败");
    }
  }

  addCompositeCondition() {
    this.ruleCompositeConditions = [
      ...this.ruleCompositeConditions,
      { type: "threshold" as const, operator: "greater_than" as ComparisonOperator, value: 0 },
    ];
  }

  removeCompositeCondition(index: number) {
    this.ruleCompositeConditions = this.ruleCompositeConditions.filter((_, i) => i !== index);
  }

  updateCompositeCondition(index: number, cond: AlarmCondition) {
    this.ruleCompositeConditions = this.ruleCompositeConditions.map((c, i) =>
      i === index ? cond : c
    );
  }

  toggleChannel(channel: NotificationChannelType) {
    if (this.ruleFormNotifyChannels.includes(channel)) {
      this.ruleFormNotifyChannels = this.ruleFormNotifyChannels.filter(c => c !== channel);
    } else {
      this.ruleFormNotifyChannels = [...this.ruleFormNotifyChannels, channel];
    }
  }

  isNumericType(dataType: string): boolean {
    const dt = dataType?.toLowerCase() || "";
    return ["int", "integer", "float", "double", "number", "long", "short", "decimal", "byte"].some(t => dt.includes(t));
  }

  async openPropertyHistory(name: string, unit: string) {
    const deviceId = this.selectedDevice?.device?.id;
    if (!deviceId) return;

    this.historyLastFocus = document.activeElement ?? undefined;
    this.showHistoryDialog = true;
    this.historyPropertyName = name;
    this.historyPropertyUnit = unit;
    this.historyDeviceId = deviceId;
    this.historyRange = "1h";
    this.historyCustomStart = "";
    this.historyCustomEnd = "";
    this.historyData = [];
    this.loadHistoryData();
    requestAnimationFrame(() => {
      const overlay = this.querySelector(".modal-overlay[role='dialog']");
      if (overlay) this.focusFirst(overlay as HTMLElement, 50);
    });
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
    const el = this.historyLastFocus as HTMLElement | undefined;
    if (el?.focus) {
      requestAnimationFrame(() => el.focus());
    }
    this.historyLastFocus = undefined;
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
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label="历史曲线" @click=${this.closeHistoryDialog} @keydown=${(e: KeyboardEvent) => this.handleModalKeydown(e, this.closeHistoryDialog)}>
        <div class="modal modal--wide" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <span>${this.historyPropertyName}${this.historyPropertyUnit ? ` (${this.historyPropertyUnit})` : ""} — 历史曲线</span>
            <button class="btn btn--icon" aria-label="关闭" @click=${this.closeHistoryDialog}>×</button>
          </div>
          <div class="modal-body history-modal-body">
            <!-- Time range selector -->
            <div class="time-range-bar">
              ${ranges.map(r => html`
                <button
                  class="time-range-btn ${this.historyRange === r.key ? 'time-range-btn--active' : ''}"
                  @click=${() => this.onHistoryRangeChange(r.key)}
                >${r.label}</button>
              `)}
            </div>
            ${this.historyRange === "custom" ? html`
              <div class="time-range-inputs">
                <label>开始</label>
                <input type="datetime-local"
                  .value=${this.historyCustomStart}
                  @change=${(e: Event) => { this.historyCustomStart = (e.target as HTMLInputElement).value; }}
                />
                <label>结束</label>
                <input type="datetime-local"
                  .value=${this.historyCustomEnd}
                  @change=${(e: Event) => { this.historyCustomEnd = (e.target as HTMLInputElement).value; }}
                />
                <button class="btn time-range-query-btn"
                  @click=${this.onHistoryCustomTimeApply}
                >查询</button>
              </div>
            ` : nothing}
            <!-- Chart -->
            ${this.historyLoading
              ? html`<div class="history-chart-placeholder">加载中...</div>`
              : this.historyData.length === 0
                ? html`<div class="history-chart-placeholder">暂无历史数据</div>`
                : html`<div id="history-chart-container" class="history-chart-container">
                    <canvas id="history-chart"></canvas>
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

  private getPaginationItems(): (number | string)[] {
    const total = this.totalPages;
    const current = this.page;
    if (total <= 7) {
      return Array.from({ length: total }, (_, i) => i + 1);
    }
    if (current <= 4) {
      return [1, 2, 3, 4, 5, '...', total];
    }
    if (current >= total - 3) {
      return [1, '...', total - 4, total - 3, total - 2, total - 1, total];
    }
    return [1, '...', current - 1, current, current + 1, '...', total];
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
    this.modalLastFocus = document.activeElement ?? undefined;
    this.editingDevice = null;
    this.formName = "";
    this.formType = "";
    this.formAddress = "";
    this.formDescription = "";
    this.formManufacturer = "";
    this.formModel = "";
    this.formProtocol = "";
    this.formPosition = "";
    this.formPort = "";
    this.formDriver = "";
    this.formDriverConfig = {};
    this.formDriverConfigOptions = [];
    this.formProperties = [];
    this.formCommands = [];
    this.formProfileLoading = false;
    this.formModalTab = 'basic';
    this.showModal = true;
    requestAnimationFrame(() => {
      const overlay = this.querySelector(".modal-overlay[role='dialog']");
      if (overlay) this.focusFirst(overlay as HTMLElement, 50);
    });
  }

  async openEdit(d: Device) {
    this.modalLastFocus = document.activeElement ?? undefined;
    this.editingDevice = d;
    this.formName = d.name;
    this.formType = d.deviceType || "";
    this.formAddress = d.address || "";
    this.formDescription = d.description || "";
    this.formManufacturer = d.factoryName || "";
    this.formModel = d.deviceModel || "";
    this.formProtocol = d.protocolType || "";
    this.formPosition = d.position || "";
    this.formPort = "";
    this.formDriver = d.driverName || "";
    this.formDriverConfig = {};
    this.formDriverConfigOptions = [];
    this.formProperties = [];
    this.formCommands = [];
    this.formProfileLoading = false;
    this.formModalTab = 'basic';
    // Load driver config if driver is set
    if (d.driverName) this.loadFormDriverConfig(d.driverName);
    this.showModal = true;

    // Load full profile data (properties + commands) if available
    this.formProfileLoading = true;
    try {
      const profileRes = await deviceApi.getDeviceProfile(d.id);
      const profile = profileRes.result;
      if (profile?.properties?.length) {
        this.formProperties = profile.properties.map(p => ({
          name: p.name, displayName: p.displayName, value: p.currentValue ?? p.value ?? '', dataType: p.dataType,
          unit: p.unit, isReadOnly: p.isReadOnly, minValue: p.minValue, maxValue: p.maxValue, description: p.description,
        }));
      }
      if (profile?.commands?.length) {
        this.formCommands = profile.commands.map(c => ({
          name: c.name, description: c.description,
          parameters: c.parameters && Object.keys(c.parameters).length > 0 ? JSON.stringify(c.parameters) : '',
        }));
      }
    } catch {
      // Fallback: use properties from device list if profile unavailable
      this.formProperties = (d.properties || []).map(p => ({
        name: p.name, displayName: p.displayName, value: p.currentValue ?? p.value ?? '', dataType: p.dataType,
        unit: p.unit, isReadOnly: p.isReadOnly, minValue: p.minValue, maxValue: p.maxValue, description: p.description,
      }));
    } finally {
      this.formProfileLoading = false;
      this.requestUpdate();
    }
    requestAnimationFrame(() => {
      const overlay = this.querySelector(".modal-overlay[role='dialog']");
      if (overlay) this.focusFirst(overlay as HTMLElement, 50);
    });
  }

  closeModal() {
    this.showModal = false;
    this.editingDevice = null;
    const el = this.modalLastFocus as HTMLElement | undefined;
    if (el?.focus) {
      requestAnimationFrame(() => el.focus());
    }
    this.modalLastFocus = undefined;
  }

  async loadFormDriverConfig(driverName: string) {
    if (!driverName) { this.formDriverConfigOptions = []; return; }
    this.formDriverConfigLoading = true;
    try {
      const res = await driverApi.getDriverConfig(driverName);
      this.formDriverConfigOptions = (res.result as any)?.configOptions || [];
      // Init config with defaults
      const config: Record<string, string> = {};
      for (const opt of this.formDriverConfigOptions) {
        config[opt.name] = this.formDriverConfig[opt.name] || opt.defaultValue || '';
      }
      this.formDriverConfig = config;
    } catch { /* driver may not have config */ } finally {
      this.formDriverConfigLoading = false;
    }
  }

  onFormDriverChange(e: Event) {
    const driverName = (e.target as HTMLSelectElement).value;
    this.formDriver = driverName;
    this.formDriverConfig = {};
    this.loadFormDriverConfig(driverName);
  }

  async saveForm() {
    if (!this.formName.trim()) return;
    this.saving = true;
    try {
      const payload: Record<string, any> = {
        name: this.formName.trim(),
        type: this.formType || undefined,
        ipAddress: this.formAddress || undefined,
        port: this.formPort ? Number(this.formPort) : undefined,
        description: this.formDescription || undefined,
        manufacturer: this.formManufacturer || undefined,
        model: this.formModel || undefined,
        protocol: this.formProtocol || undefined,
        position: this.formPosition || undefined,
        driverName: this.formDriver || undefined,
        driverOptions: Object.keys(this.formDriverConfig).length > 0
          ? JSON.stringify(this.formDriverConfig) : undefined,
        properties: this.formProperties.length > 0
          ? this.formProperties.map(p => ({
              name: p.name, displayName: p.displayName, value: p.value, dataType: p.dataType,
              unit: p.unit, isReadOnly: p.isReadOnly, minValue: p.minValue, maxValue: p.maxValue, description: p.description,
            }))
          : undefined,
        commands: this.formCommands.length > 0
          ? this.formCommands.map(c => ({
              name: c.name, description: c.description,
              parameters: c.parameters ? (() => { try { return JSON.parse(c.parameters); } catch { return {}; } })() : {},
            }))
          : undefined,
      };
      if (this.editingDevice) {
        await deviceApi.updateDevice(this.editingDevice.id, payload);
        success("设备已更新");
      } else {
        await deviceApi.createDevice(payload as CreateDeviceRequest);
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
    this.wizardLastFocus = document.activeElement ?? undefined;
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
    requestAnimationFrame(() => {
      const overlay = this.querySelector(".wizard-overlay[role='dialog']");
      if (overlay) this.focusFirst(overlay as HTMLElement, 50);
    });
  }

  closeWizard() {
    this.showWizard = false;
    const el = this.wizardLastFocus as HTMLElement | undefined;
    if (el?.focus) {
      requestAnimationFrame(() => el.focus());
    }
    this.wizardLastFocus = undefined;
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
        <div class="page-loading">
          <span class="loading-spinner"></span>
          <span>加载中...</span>
        </div>
      `;
    }

    if (this.error) {
      return html`
        <div class="page-error">
          <div class="page-error__message">${this.error}</div>
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
      <div class="toolbar">
        <div class="field filter-bar__search">
          <input
            type="text"
            placeholder="搜索设备名称..."
            .value=${this.searchName}
            @input=${(e: Event) => { this.searchName = (e.target as HTMLInputElement).value; }}
            @keydown=${(e: KeyboardEvent) => { if (e.key === "Enter") { this.page = 1; this.loadDevices(); } }}
          />
        </div>
        <select class="select filter-bar__select" .value=${this.filterStatus} @change=${(e: Event) => { this.filterStatus = (e.target as HTMLSelectElement).value; this.page = 1; this.loadDevices(); }}>
          <option value="">全部状态</option>
          <option value="online">在线</option>
          <option value="offline">离线</option>
          <option value="error">故障</option>
          <option value="maintenance">维护</option>
        </select>
        <select class="select filter-bar__select" .value=${this.filterProtocol} @change=${(e: Event) => { this.filterProtocol = (e.target as HTMLSelectElement).value; this.page = 1; this.loadDevices(); }}>
          <option value="">全部协议</option>
          <option value="modbus-tcp">Modbus TCP</option>
          <option value="modbus-rtu">Modbus RTU</option>
          <option value="mqtt">MQTT</option>
          <option value="onvif">ONVIF</option>
          <option value="snmp">SNMP</option>
        </select>
        <div class="toolbar__spacer"></div>
        <div class="view-toggle">
          <button
            class="btn btn--ghost btn--sm view-toggle__btn ${this.viewMode === 'table' ? 'view-toggle__btn--active' : ''}"
            @click=${() => { this.viewMode = "table"; }}
            title="列表视图"
          >&#9776;</button>
          <button
            class="btn btn--ghost btn--sm view-toggle__btn ${this.viewMode === 'grid' ? 'view-toggle__btn--active' : ''}"
            @click=${() => { this.viewMode = "grid"; }}
            title="卡片视图"
          >&#9638;</button>
        </div>
        <button class="btn btn--primary" @click=${this.openWizard}>新建设备</button>
        <button class="btn btn--outline" @click=${() => { this.showPairingDialog = true; }}>${t("gatewayPairing.addGateway")}</button>
      </div>
    `;
  }

  renderDeviceList() {
    return html`
      <div class="device-list">
        ${this.renderToolbar()}
        <div class="device-list__content">
          ${this.viewMode === "table" ? this.renderTableView() : this.renderGridView()}
        </div>
        ${this.renderPagination()}
        ${this.showModal ? this.renderModal() : nothing}
        ${this.showWizard ? this.renderWizard() : nothing}
        ${this.showPairingDialog
          ? html`<gateway-pairing-dialog
              @close=${() => { this.showPairingDialog = false; }}
              @paired=${() => { this.showPairingDialog = false; this.page = 1; this.loadDevices(); }}
            ></gateway-pairing-dialog>`
          : nothing}
      </div>
    `;
  }

  renderPagination() {
    if (this.total === 0) return nothing;
    const items = this.getPaginationItems();
    return html`
      <div class="pagination">
        <button
          class="pagination__btn pagination__btn--arrow"
          ?disabled=${this.page <= 1}
          @click=${() => this.goToPage(this.page - 1)}
          aria-label="上一页"
        >‹</button>
        <div class="pagination__pages">
          ${items.map(item => {
            if (item === '...') {
              return html`<span class="pagination__ellipsis">…</span>`;
            }
            const p = item as number;
            const isActive = p === this.page;
            return html`
              <button
                class="pagination__btn ${isActive ? 'pagination__btn--active' : ''}"
                @click=${() => this.goToPage(p)}
                aria-label="第 ${p} 页"
                aria-current=${isActive ? 'page' : nothing}
              >${p}</button>
            `;
          })}
        </div>
        <button
          class="pagination__btn pagination__btn--arrow"
          ?disabled=${this.page >= this.totalPages}
          @click=${() => this.goToPage(this.page + 1)}
          aria-label="下一页"
        >›</button>
        <span class="pagination__meta">${this.page} / ${this.totalPages}</span>
      </div>
    `;
  }

  renderTableView() {
    const devices = this.devices;
    return html`
      <div class="card table-container">
        <table class="data-table">
          <thead>
            <tr>
              <th>设备名称</th>
              <th>类型</th>
              <th>协议</th>
              <th>状态</th>
              <th>标签</th>
              <th class="cell-actions">操作</th>
            </tr>
          </thead>
          <tbody>
            ${devices.length === 0
              ? html`<tr><td colspan="6" class="empty-hint">暂无设备</td></tr>`
              : devices.map(d => html`
                <tr>
                  <td>
                    <div class="data-table__primary">${d.displayName || d.name}</div>
                    <div class="data-table__secondary">${d.name}</div>
                  </td>
                  <td class="data-table__cell-sm">${d.deviceType || "-"}</td>
                  <td class="data-table__cell-sm">${d.protocolType || d.driverName || "-"}</td>
                  <td>
                    <span class="status-badge">
                      <span class="status-dot" style="background: ${this.statusColor(d.status)};"></span>
                      <span class="status-badge__label">${this.statusLabel(d.status)}</span>
                    </span>
                  </td>
                  <td class="cell-actions">
                    ${this.renderTableCellTags(d)}
                  </td>
                  <td class="cell-actions">
                    <button class="btn btn--ghost btn--sm" @click=${() => this.navigateToDevice(d.id)}>详情</button>
                    <button class="btn btn--ghost btn--sm" @click=${() => this.openEdit(d)}>编辑</button>
                    <button class="btn btn--ghost btn--sm" @click=${() => this.exportDeviceTemplate(d)}>导出模板</button>
                    <button class="btn btn--ghost btn--sm" @click=${() => this.cloneDevice(d)}>克隆</button>
                    <button class="btn btn--ghost btn--sm btn--danger-text" @click=${() => this.deleteDevice(d)}>删除</button>
                  </td>
                </tr>
              `)}
          </tbody>
        </table>
      </div>
    `;
  }

  renderGridView() {
    const devices = this.devices;
    if (devices.length === 0) {
      return html`
        <div class="card empty-hint">暂无设备</div>
      `;
    }
    return html`
      <div class="model-grid">
        ${devices.map(d => this.renderDeviceCard(d))}
      </div>
    `;
  }

  renderTableCellTags(d: Device) {
    const deviceTags = d.tags || [];
    const isEditingTags = this.editingTagsDeviceId === d.id;
    return html`
      <div class="tag-editor-trigger">
        ${deviceTags.slice(0, 3).map(t => html`
          <span class="tag-pill tag-pill--xs">${t.name}</span>
        `)}
        ${deviceTags.length > 3 ? html`<span class="tag-overflow-count">+${deviceTags.length - 3}</span>` : nothing}
        ${deviceTags.length === 0 ? html`<span class="tag-overflow-count">-</span>` : nothing}
        <button
          class="btn btn--ghost btn--sm tag-btn--edit"
          title="管理标签"
          @click=${(e: Event) => { e.stopPropagation(); this.toggleTagEditor(d.id); }}
        >${icons.tag}</button>
      </div>
      ${isEditingTags ? this.renderTagPopover(d, deviceTags) : nothing}
    `;
  }

  renderTagPopover(d: Device, deviceTags: Tag[]) {
    const keyword = this.tagSearchKeyword.trim();
    const filtered = this.allTags.filter(t => !keyword || t.name.toLowerCase().includes(keyword.toLowerCase()));
    const exactMatch = keyword && this.allTags.some(t => t.name.toLowerCase() === keyword.toLowerCase());
    const showCreate = keyword && !exactMatch;

    return html`
      <div class="tag-popover" @click=${(e: Event) => e.stopPropagation()}>
        <input
          type="text"
          class="tag-popover__search"
          placeholder="搜索或输入新标签名..."
          .value=${this.tagSearchKeyword}
          @input=${(e: Event) => { this.tagSearchKeyword = (e.target as HTMLInputElement).value; }}
          @keydown=${(e: KeyboardEvent) => {
            if (e.key === 'Enter' && showCreate) {
              e.preventDefault();
              this.createAndBindTag(d, keyword);
            }
          }}
        />
        <div class="tag-popover__list">
          ${showCreate ? html`
            <button
              class="btn btn--sm tag-btn tag-btn--create"
              ?disabled=${this.tagCreating}
              @click=${() => this.createAndBindTag(d, keyword)}
            >
              <span class="flex-mid gap-1">
                ${this.tagCreating ? html`<span class="tag-spinner"></span>` : icons.plus}
                创建标签「${keyword}」
              </span>
            </button>
          ` : nothing}
          ${filtered.map(t => {
              const bound = deviceTags.some(dt => dt.id === t.id);
              return html`
                <button
                  class="btn btn--sm tag-btn ${bound ? 'tag-btn--bound' : 'tag-btn--unbound'}"
                  ?disabled=${this.tagSaving}
                  @click=${() => this.toggleTag(d, t)}
                >
                  <span class="flex-mid gap-1">
                    ${bound ? icons.check : icons.plus}
                    ${t.name}
                  </span>
                </button>
              `;
            })}
          ${filtered.length === 0 && !showCreate
            ? html`<span class="tag-no-match">无匹配标签，输入关键字创建新标签</span>`
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
      <div class="device-card__wrap">
        <div class="card device-card">
          <!-- Header -->
          <div class="device-card__header">
            <div class="device-card__header-left">
              <span class="status-dot status-dot--sm" style="background: ${this.statusColor(d.status)};"></span>
              <span class="device-card__title" title="${d.displayName || d.name}">${d.displayName || d.name}</span>
              ${d.linked_gateway ? html`<span class="device-card__gateway-tag">via gateway</span>` : nothing}
            </div>
            <div class="device-card__actions">
              <button
                class="btn btn--ghost btn--sm device-card__action-btn"
                title="编辑"
                @click=${(e: Event) => { e.stopPropagation(); this.openEdit(d); }}
              >${icons.edit}</button>
              <button
                class="btn btn--ghost btn--sm device-card__action-btn btn--danger-text"
                title="删除"
                @click=${(e: Event) => { e.stopPropagation(); this.deleteDevice(d); }}
              >${icons.trash2}</button>
            </div>
          </div>

          <!-- Info -->
          <div
            class="device-card__body"
            title="${infoTooltip}"
            @click=${() => this.navigateToDevice(d.id)}
          >
            <div class="device-card__info">
              ${d.deviceType ? html`
                <div class="device-card__info-row">
                  <span class="device-card__info-label">类型</span>
                  <span class="device-card__info-value">${d.deviceType}</span>
                </div>
              ` : nothing}
              ${d.protocolType || d.driverName ? html`
                <div class="device-card__info-row">
                  <span class="device-card__info-label">协议</span>
                  <span class="device-card__info-value">${d.protocolType || d.driverName}</span>
                </div>
              ` : nothing}
              ${d.address ? html`
                <div class="device-card__info-row">
                  <span class="device-card__info-label">地址</span>
                  <span class="device-card__info-value">${d.address}</span>
                </div>
              ` : nothing}
            </div>
          </div>

          <!-- Footer -->
          <div class="device-card__footer">
            ${visibleTags.map(t => html`
              <span class="tag-pill">${t.name}</span>
            `)}
            ${hiddenTagCount > 0 ? html`
              <span class="tag-pill tag-pill--muted" title="${deviceTags.slice(3).map(t => t.name).join(', ')}">
                +${hiddenTagCount}
              </span>
            ` : nothing}
            ${deviceTags.length === 0 ? html`<span class="inline-muted" style="font-size: 12px;">无标签</span>` : nothing}
            <button
              class="btn btn--ghost btn--sm tag-btn--edit-card"
              title="管理标签"
              @click=${(e: Event) => { e.stopPropagation(); this.toggleTagEditor(d.id); }}
            >${icons.tag}</button>
          </div>
        </div>

        <!-- Tag editor popover -->
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
      <div class="card detail-header">
        <div class="detail-header__row">
          <div class="detail-header__main">
            <button class="btn btn--ghost btn--sm detail-header__back" @click=${this.backToList}>
              &larr; 返回
            </button>
            <h2 class="detail-header__title">${d.displayName || d.name}</h2>
            <span class="status-badge status-badge--subtle">
              <span class="status-dot status-dot--sm" style="background: ${this.statusColor(d.status)};"></span>
              <span class="status-badge__label">${this.statusLabel(d.status)}</span>
            </span>
            ${d.deviceType ? html`
              <span class="type-tag">${d.deviceType}</span>
            ` : nothing}
          </div>
          <button class="btn btn--ghost btn--sm" @click=${() => this.openEdit(d)}>编辑</button>
        </div>
        ${deviceTags.length > 0 ? html`
          <div class="detail-header__tags">
            ${deviceTags.map((t: Tag) => html`
              <span class="tag-pill">${t.name}</span>
            `)}
          </div>
        ` : nothing}
      </div>

      <!-- Mini stat grid -->
      <div class="detail-stat-grid">
        <div class="stat">
          <div class="stat-label">属性总数</div>
          <div class="stat-value">${ov.totalProperties}</div>
        </div>
        <div class="stat">
          <div class="stat-label">在线属性</div>
          <div class="stat-value ok">${ov.onlineProperties}</div>
        </div>
        <div class="stat">
          <div class="stat-label">命令数</div>
          <div class="stat-value">${ov.totalCommands}</div>
        </div>
        <div class="stat">
          <div class="stat-label">活跃告警</div>
          <div class="stat-value ${ov.activeAlarms > 0 ? 'warn' : ''}">${ov.activeAlarms}</div>
        </div>
      </div>

      <!-- Tab bar -->
      <div class="detail-tabs">
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
    if (!profile) return html`<div class="card empty-center">暂无属性数据</div>`;

    // 从缓存读取（SSE 推送的实时数据），用 profile.properties 的元数据补充缺失字段
    const cached = deviceCache.$devicesMap.get().get(profile.device.id);
    let properties: DeviceProperty[] = [];

    if (cached?.properties?.length) {
      // 有缓存：用 API 属性元数据 + 缓存实时值
      const apiMap = new Map((profile.properties ?? []).map(p => [p.name, p]));
      properties = cached.properties.map(cachedProp => {
        const apiProp = apiMap.get(cachedProp.name);
        return apiProp
          ? { ...apiProp, currentValue: cachedProp.currentValue ?? cachedProp.value, updatedAt: cachedProp.updatedAt }
          : cachedProp;
      });
    } else if (profile.properties?.length) {
      // 无缓存：用 API 属性
      properties = profile.properties;
    }

    if (properties.length === 0) {
      return html`<div class="card empty-center">暂无属性数据</div>`;
    }

    return html`
      <div class="card prop-table-wrap">
        <table class="data-table--compact">
          <thead>
            <tr>
              <th>属性</th>
              <th>名称</th>
              <th>当前值</th>
              <th></th>
              <th>类型</th>
              <th class="cell-actions">读写</th>
              <th>更新时间</th>
            </tr>
          </thead>
          <tbody>
            ${properties.map((p: DeviceProperty) => html`
              <tr>
                <td>${p.name}</td>
                <td>${p.displayName || p.name}</td>
                <td>
                  <span class="prop-value">${p.currentValue ?? p.value ?? "-"}</span>
                  ${p.unit ? html`<span class="prop-unit">${p.unit}</span>` : nothing}
                </td>
                <td class="cell-actions">
                  ${this.isNumericType(p.dataType) ? html`
                    <button
                      class="btn btn--icon btn--xs"
                      title="曲线"
                      aria-label="历史曲线"
                      @click=${() => this.openPropertyHistory(p.name, p.unit || "")}
                    >${icons.trendingUp}</button>
                  ` : nothing}
                </td>
                <td class="prop-type">${p.dataType}</td>
                <td class="cell-actions">
                  <span class="${p.isReadOnly ? 'prop-ro-badge' : 'prop-rw-badge'}">
                    ${p.isReadOnly ? '只读' : '读写'}
                  </span>
                </td>
                <td class="prop-type">${p.updatedAt?.slice(0, 16) || "-"}</td>
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
      return html`<div class="card empty-center">暂无命令</div>`;
    }

    return html`
      <div class="card command-list-wrap">
        <div class="command-list">
          ${profile.commands.map(c => html`
            <div class="command-item">
              <div>
                <div class="command-item__name">${c.name}</div>
                <div class="command-item__desc">${c.description || "无描述"}</div>
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
      return html`<div class="card empty-center">暂无事件记录</div>`;
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
      <div class="card events-list-wrap">
        ${events.map((ev: DeviceEvent) => html`
          <div class="event-item">
            <span class="event-badge ${levelClass(ev.level)}">${levelLabel(ev.level)}</span>
            <div class="event-item__body">
              <div class="event-item__title">${ev.title}</div>
              ${ev.message ? html`<div class="event-item__message">${ev.message}</div>` : nothing}
            </div>
            <span class="event-item__time">${ev.createdAt?.slice(0, 16)}</span>
          </div>
        `)}
      </div>
    `;
  }

  renderDetailAlarms() {
    const profile = this.selectedDevice;
    if (!profile) return nothing;
    const properties = profile.properties || [];

    return html`
      <!-- Active alarms — real-time display -->
      <div class="card" style="margin-top: var(--space-4);">
        <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: var(--space-3);">
          <div>
            <div class="alarm-rules-card__title">实时告警</div>
            <div class="alarm-rules-card__sub">当前活跃的告警，恢复正常后自动消失</div>
          </div>
          <button class="btn btn--ghost btn--xs" @click=${this.loadDeviceAlarms} ?disabled=${this.alarmsLoading}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14" style="vertical-align: -2px;">
              <polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
            </svg>
          </button>
        </div>
        ${this.alarmsLoading
          ? html`<div class="alarm-rules-card__loading"><span class="loading-spinner"></span> 加载中...</div>`
          : this.deviceAlarms.length === 0
            ? html`<div class="alarm-rules-empty">
                <div class="alarm-rules-empty__text" style="color: var(--success);">🎉 无活跃告警</div>
                <div class="alarm-rules-empty__hint">一切正常</div>
              </div>`
            : html`
              <div class="alarm-rules-list">
                ${this.deviceAlarms.map((a: any) => html`
                  <div class="alarm-rule-item" style="animation: ruleFadeIn 0.35s var(--ease-out) both;">
                    <div class="alarm-rule-item__main">
                      <div class="alarm-rule-item__header">
                        <span class="alarm-rule-badge alarm-rule-badge--${(a.alarmLevel || '').toLowerCase()}">${this.levelLabel2(a.alarmLevel || '')}</span>
                        <span class="alarm-rule-item__name">${a.message}</span>
                      </div>
                      <div class="alarm-rule-item__meta">
                        <span>${(a.alarmTime || a.createdAt || '').slice(0, 16)}</span>
                      </div>
                    </div>
                  </div>
                `)}
              </div>
            `
        }
      </div>

      <!-- Alarm rules section -->
      <div class="card alarm-rules-card">
        <div class="alarm-rules-card__header">
          <div>
            <div class="alarm-rules-card__title">告警规则</div>
            <div class="alarm-rules-card__sub">管理设备的自动告警规则</div>
          </div>
          <button class="btn btn--primary btn--sm" @click=${this.openNewRule}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14" class="btn__icon-left">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            添加规则
          </button>
        </div>

        ${this.rulesLoading
          ? html`<div class="empty-center alarm-rules-card__loading"><span class="loading-spinner"></span> 加载中...</div>`
          : this.alarmRules.length === 0
            ? html`
              <div class="alarm-rules-empty">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="40" height="40" class="alarm-rules-empty__icon">
                  <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
                  <line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
                </svg>
                <div class="alarm-rules-empty__text">暂无告警规则</div>
                <div class="alarm-rules-empty__hint">添加规则后，设备数据变化将自动触发告警</div>
              </div>
            `
            : html`
              <div class="alarm-rules-list">
                ${this.alarmRules.map(rule => {
                  const condSummary = this.formatCondition(rule.condition);
                  const propName = properties.find(p => p.name === rule.propertyId)?.displayName || rule.propertyId || "—";
                  return html`
                    <div class="alarm-rule-item ${rule.isEnabled ? '' : 'alarm-rule-item--disabled'}" style="animation: ruleFadeIn 0.35s var(--ease-out) both; animation-delay: ${Math.min(this.alarmRules.indexOf(rule) * 50, 300)}ms;">
                      <div class="alarm-rule-item__main">
                        <div class="alarm-rule-item__header">
                          <span class="alarm-rule-item__name">${rule.name}</span>
                          <span class="alarm-rule-badge alarm-rule-badge--${rule.alarmLevel.toLowerCase()}">${this.levelLabel2(rule.alarmLevel)}</span>
                          ${rule.notificationConfig?.enabled
                            ? html`<span class="alarm-rule-item__notify-icon" title="通知已开启">🔔</span>`
                            : nothing
                          }
                        </div>
                        <div class="alarm-rule-item__meta">
                          <span>属性: ${propName}</span>
                          <span>条件: ${condSummary}</span>
                        </div>
                      </div>
                      <div class="alarm-rule-item__actions">
                        <label class="toggle-switch" title=${rule.isEnabled ? "已启用" : "已禁用"}>
                          <input type="checkbox" .checked=${rule.isEnabled} @change=${() => this.toggleRule(rule)} />
                          <span class="toggle-switch__slider"></span>
                        </label>
                        <button class="btn btn--ghost btn--xs" @click=${() => this.openEditRule(rule)}>编辑</button>
                        <button class="btn btn--ghost btn--xs btn--danger-text" @click=${() => this.deleteRule(rule)}>删除</button>
                      </div>
                    </div>
                  `;
                })}
              </div>
            `
        }
      </div>

      <!-- Rule editor modal -->
      ${this.showRuleModal ? this.renderRuleModal(profile.device.id, properties) : nothing}
    `;
  }

  formatCondition(cond: AlarmCondition): string {
    switch (cond.type) {
      case "threshold": {
        const opLabels: Record<string, string> = {
          greater_than: ">", less_than: "<", greater_than_or_equal: "≥",
          less_than_or_equal: "≤", equal: "=", not_equal: "≠",
        };
        return `${opLabels[cond.operator] || cond.operator} ${cond.value}`;
      }
      case "range": {
        const lo = cond.min != null ? cond.min : "-∞";
        const hi = cond.max != null ? cond.max : "+∞";
        return `${lo} ~ ${hi}`;
      }
      case "change": {
        const dir = cond.changeType === "increase" ? "上升" : cond.changeType === "decrease" ? "下降" : "变化";
        return `${dir} > ${cond.threshold}`;
      }
      case "composite": {
        return `${cond.conditions.length} 个条件 (${cond.operator})`;
      }
      default: return "—";
    }
  }

  levelLabel2(level: string): string {
    const m: Record<string, string> = { Info: "信息", Warning: "警告", Error: "错误", Critical: "严重", info: "信息", warning: "警告", error: "错误", critical: "严重" };
    return m[level] || level;
  }

  statusLabel2(status: string): string {
    const s = status?.toLowerCase();
    const m: Record<string, string> = { active: "活跃", acknowledged: "已确认", resolved: "已解决", suppressed: "已抑制" };
    return m[s] || status;
  }

  renderRuleModal(_deviceId: string, properties: any[]) {
    const isEdit = !!this.editingRule;
    const ruleTypeOptions: { value: RuleType; label: string }[] = [
      { value: "threshold", label: "阈值比较" },
      { value: "range", label: "范围判断" },
      { value: "change", label: "变化检测" },
      { value: "composite", label: "组合条件" },
    ];
    const levelOptions: AlarmLevel[] = ["Info", "Warning", "Error", "Critical"];
    const opOptions: { value: ComparisonOperator; label: string }[] = [
      { value: "greater_than", label: "大于 ( > )" },
      { value: "less_than", label: "小于 ( < )" },
      { value: "greater_than_or_equal", label: "大于等于 ( ≥ )" },
      { value: "less_than_or_equal", label: "小于等于 ( ≤ )" },
      { value: "equal", label: "等于 ( = )" },
      { value: "not_equal", label: "不等于 ( ≠ )" },
    ];
    const channelOptions: { value: NotificationChannelType; label: string }[] = [
      { value: "Email", label: "邮件" },
      { value: "Sms", label: "短信" },
      { value: "Webhook", label: "Webhook" },
      { value: "Sse", label: "SSE" },
    ];

    return html`
      <div class="modal-overlay device-edit-overlay" role="dialog" aria-modal="true"
        aria-label=${isEdit ? "编辑告警规则" : "添加告警规则"}
        @click=${this.closeRuleModal}
        @keydown=${(e: KeyboardEvent) => { if (e.key === "Escape") this.closeRuleModal(); }}>
        <div class="device-edit-dialog" style="max-width: 600px;" @click=${(e: Event) => e.stopPropagation()}>
          <!-- Header -->
          <div class="device-edit-header">
            <div class="device-edit-header__left">
              <span class="device-edit-header__icon" style="background: var(--warning-subtle, rgba(245,158,11,0.1)); color: var(--warning, #f59e0b);">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20">
                  <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
                  <line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
                </svg>
              </span>
              <div>
                <h2 class="device-edit-header__title">${isEdit ? '编辑告警规则' : '添加告警规则'}</h2>
                <span class="device-edit-header__sub">设备: ${this.selectedDevice?.device?.displayName || this.selectedDevice?.device?.name}</span>
              </div>
            </div>
            <button class="device-edit-close" @click=${this.closeRuleModal} aria-label="关闭">&times;</button>
          </div>

          <!-- Body -->
          <div class="device-edit-body rule-modal-body">
            <!-- Basic info -->
            <div class="field-group">
              <div class="field">
                <label class="label">规则名称 <span style="color: var(--danger);">*</span></label>
                <input class="input" type="text" placeholder="例如: 温度过高告警"
                  .value=${this.ruleFormName}
                  @input=${(e: any) => { this.ruleFormName = e.target.value; }} />
              </div>
              <div class="field">
                <label class="label">描述</label>
                <input class="input" type="text" placeholder="可选描述"
                  .value=${this.ruleFormDesc}
                  @input=${(e: any) => { this.ruleFormDesc = e.target.value; }} />
              </div>
              <div class="field">
                <label class="label">关联属性</label>
                <select class="select" .value=${this.ruleFormPropertyId} @change=${(e: any) => { this.ruleFormPropertyId = e.target.value; }}>
                  ${properties.map((p: any) => html`
                    <option value=${p.id}>${p.displayName || p.name} (${p.dataType || "string"})</option>
                  `)}
                </select>
              </div>
            </div>

            <!-- Rule type -->
            <div class="field rule-modal-section">
              <label class="label">规则类型</label>
              <div class="rule-type-tabs">
                ${ruleTypeOptions.map(opt => html`
                  <button class="rule-type-tab ${this.ruleFormType === opt.value ? 'rule-type-tab--active' : ''}"
                    @click=${() => { this.ruleFormType = opt.value; }}>
                    ${opt.label}
                  </button>
                `)}
              </div>
            </div>

            <!-- Condition builder -->
            <div class="condition-builder">
              ${this.ruleFormType === "threshold" ? html`
                <div class="condition-row">
                  <select class="select" .value=${this.ruleFormOperator}
                    @change=${(e: any) => { this.ruleFormOperator = e.target.value; }}>
                    ${opOptions.map(o => html`<option value=${o.value}>${o.label}</option>`)}
                  </select>
                  <input class="input condition-input--value" type="number"
                    .value=${String(this.ruleFormValue)}
                    @input=${(e: any) => { this.ruleFormValue = parseFloat(e.target.value) || 0; }}
                    placeholder="阈值" />
                </div>
              ` : nothing}

              ${this.ruleFormType === "range" ? html`
                <div class="condition-row">
                  <input class="input condition-input--value" type="number"
                    .value=${String(this.ruleFormMin)}
                    @input=${(e: any) => { this.ruleFormMin = parseFloat(e.target.value) || 0; }}
                    placeholder="最小值" />
                  <span class="condition-separator">~</span>
                  <input class="input condition-input--value" type="number"
                    .value=${String(this.ruleFormMax)}
                    @input=${(e: any) => { this.ruleFormMax = parseFloat(e.target.value) || 0; }}
                    placeholder="最大值" />
                </div>
              ` : nothing}

              ${this.ruleFormType === "change" ? html`
                <div class="condition-row condition-row--wrap">
                  <select class="select" .value=${this.ruleFormChangeType}
                    @change=${(e: any) => { this.ruleFormChangeType = e.target.value; }}>
                    <option value="any">任意变化</option>
                    <option value="increase">上升</option>
                    <option value="decrease">下降</option>
                  </select>
                  <span class="condition-label">超过</span>
                  <input class="input condition-input--sm" type="number"
                    .value=${String(this.ruleFormChangeThreshold)}
                    @input=${(e: any) => { this.ruleFormChangeThreshold = parseFloat(e.target.value) || 0; }}
                    placeholder="阈值" />
                  <span class="condition-label">在</span>
                  <input class="input condition-input--xs" type="number"
                    .value=${String(this.ruleFormChangeWindow)}
                    @input=${(e: any) => { this.ruleFormChangeWindow = parseInt(e.target.value) || 0; }}
                    placeholder="秒" />
                  <span class="condition-label">秒内</span>
                </div>
              ` : nothing}

              ${this.ruleFormType === "composite" ? html`
                <div class="composite-header">
                  <select class="select" .value=${this.ruleFormLogicOp}
                    @change=${(e: any) => { this.ruleFormLogicOp = e.target.value; }}>
                    <option value="and">AND (全部满足)</option>
                    <option value="or">OR (任一满足)</option>
                    <option value="not">NOT (取反)</option>
                  </select>
                  <button class="btn btn--ghost btn--xs" @click=${this.addCompositeCondition}>+ 添加子条件</button>
                </div>
                ${this.ruleCompositeConditions.map((cond, i) => html`
                  <div class="condition-row condition-sub-row">
                    <span class="condition-sub-row__index">#${i + 1}</span>
                    <select class="select condition-sub-row__op"
                      .value=${cond.type === "threshold" ? cond.operator : "greater_than"}
                      @change=${(e: any) => {
                        const c = this.ruleCompositeConditions[i];
                        if (c.type === "threshold") {
                          this.updateCompositeCondition(i, { ...c, operator: e.target.value });
                        }
                      }}>
                      ${opOptions.map(o => html`<option value=${o.value}>${o.label}</option>`)}
                    </select>
                    <input class="input condition-input--xs" type="number"
                      .value=${String(cond.type === "threshold" ? cond.value : 0)}
                      @input=${(e: any) => {
                        const c = this.ruleCompositeConditions[i];
                        if (c.type === "threshold") {
                          this.updateCompositeCondition(i, { ...c, value: parseFloat(e.target.value) || 0 });
                        }
                      }} />
                    <button class="btn btn--ghost btn--xs btn--danger-text"
                      @click=${() => this.removeCompositeCondition(i)}>✕</button>
                  </div>
                `)}
              ` : nothing}
            </div>

            <!-- Alarm level -->
            <div class="field rule-modal-section">
              <label class="label">告警级别</label>
              <div class="level-selector">
                ${levelOptions.map(lvl => {
                  const colors: Record<string, string> = {
                    Info: "var(--info, #3498db)", Warning: "var(--warning, #f39c12)",
                    Error: "var(--danger, #e74c3c)", Critical: "var(--critical, #9b59b6)",
                  };
                  return html`
                    <button class="level-chip ${this.ruleFormLevel === lvl ? 'level-chip--active' : ''}"
                      style="--chip-color: ${colors[lvl] || 'var(--muted)'};"
                      @click=${() => { this.ruleFormLevel = lvl; }}>
                      ${this.levelLabel2(lvl)}
                    </button>
                  `;
                })}
              </div>
            </div>

            <!-- Notification config -->
            <div class="field rule-modal-section">
              <label class="label checkbox-label">
                <input type="checkbox" class="checkbox-label__input"
                  .checked=${this.ruleFormNotifyEnabled}
                  @change=${(e: any) => { this.ruleFormNotifyEnabled = e.target.checked; }} />
                启用通知
              </label>
            </div>

            ${this.ruleFormNotifyEnabled ? html`
              <div class="notification-config">
                <div class="field">
                  <label class="label field__label-sm">通知渠道</label>
                  <div class="channel-chips">
                    ${channelOptions.map(ch => html`
                      <button class="channel-chip ${this.ruleFormNotifyChannels.includes(ch.value) ? 'channel-chip--active' : ''}"
                        @click=${() => this.toggleChannel(ch.value)}>
                        ${ch.label}
                      </button>
                    `)}
                  </div>
                </div>
                <div class="field">
                  <label class="label field__label-sm">接收人</label>
                  <input class="input" type="text" placeholder="邮箱或手机号，逗号分隔"
                    .value=${this.ruleFormNotifyRecipients}
                    @input=${(e: any) => { this.ruleFormNotifyRecipients = e.target.value; }} />
                </div>
              </div>
            ` : nothing}
          </div>

          <!-- Footer -->
          <div class="rule-modal-footer">
            <button class="btn btn--ghost" @click=${this.closeRuleModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.ruleSaving} @click=${this.saveRule}>
              ${this.ruleSaving ? "保存中..." : isEdit ? "保存修改" : "创建规则"}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  renderModal() {
    const isEdit = !!this.editingDevice;
    const tabs: { key: 'basic' | 'driver' | 'properties' | 'commands'; label: string }[] = [
      { key: 'basic', label: '基本信息' },
      { key: 'driver', label: '驱动配置' },
      { key: 'properties', label: `属性${this.formProperties.length ? ` (${this.formProperties.length})` : ''}` },
      { key: 'commands', label: `命令${this.formCommands.length ? ` (${this.formCommands.length})` : ''}` },
    ];

    return html`
      <div class="modal-overlay device-edit-overlay" role="dialog" aria-modal="true"
        aria-label="${isEdit ? '编辑设备' : '新建设备'}"
        @click=${this.closeModal}
        @keydown=${(e: KeyboardEvent) => this.handleModalKeydown(e, this.closeModal)}>
        <div class="device-edit-dialog" @click=${(e: Event) => e.stopPropagation()}>
          <!-- Header -->
          <div class="device-edit-header">
            <div class="device-edit-header__left">
              <span class="device-edit-header__icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20">
                  <rect x="2" y="3" width="20" height="14" rx="2" ry="2"/>
                  <line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/>
                </svg>
              </span>
              <div>
                <h2 class="device-edit-header__title">
                  ${isEdit ? html`编辑设备 — <strong>${this.editingDevice!.name}</strong>` : '新建设备'}
                </h2>
                ${isEdit && this.editingDevice!.displayName ? html`<span class="device-edit-header__sub">${this.editingDevice!.displayName}</span>` : nothing}
              </div>
            </div>
            <button class="device-edit-close" @click=${this.closeModal} aria-label="关闭">&times;</button>
          </div>

          <!-- Tabs -->
          <div class="device-edit-tabs">
            ${tabs.map(t => html`
              <button class="device-edit-tab ${this.formModalTab === t.key ? 'active' : ''}"
                @click=${() => { this.formModalTab = t.key; }}>
                ${t.label}
              </button>
            `)}
          </div>

          <!-- Body -->
          <div class="device-edit-body">
            ${this.formModalTab === 'basic' ? this.renderBasicInfoTab() : nothing}
            ${this.formModalTab === 'driver' ? this.renderDriverTab() : nothing}
            ${this.formModalTab === 'properties' ? this.renderPropertiesTab() : nothing}
            ${this.formModalTab === 'commands' ? this.renderCommandsTab() : nothing}
          </div>

          <!-- Footer -->
          <div class="device-edit-footer">
            <button class="btn btn--ghost" @click=${this.closeModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.saving || !this.formName.trim()} @click=${this.saveForm}>
              ${this.saving ? '保存中...' : '保存'}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  private renderBasicInfoTab() {
    return html`
      <div class="edit-section">
        <div class="edit-section__header">
          <span class="edit-section__title">基本信息</span>
          <span class="edit-section__hint">必填字段标记 *</span>
        </div>
        <div class="edit-grid edit-grid--2col">
          <div class="edit-field edit-field--required">
            <label class="edit-field__label">设备名称</label>
            <input type="text" class="edit-field__input"
              placeholder="输入设备名称" .value=${this.formName}
              @input=${(e: any) => { this.formName = e.target.value; }} />
          </div>
          <div class="edit-field">
            <label class="edit-field__label">设备类型</label>
            <input type="text" class="edit-field__input"
              placeholder="如 sensor, gateway" .value=${this.formType}
              @input=${(e: any) => { this.formType = e.target.value; }} />
          </div>
          <div class="edit-field">
            <label class="edit-field__label">地址</label>
            <input type="text" class="edit-field__input"
              placeholder="如 192.168.1.100" .value=${this.formAddress}
              @input=${(e: any) => { this.formAddress = e.target.value; }} />
          </div>
          <div class="edit-field">
            <label class="edit-field__label">端口</label>
            <input type="number" class="edit-field__input"
              placeholder="如 502" .value=${this.formPort}
              @input=${(e: any) => { this.formPort = e.target.value; }} />
          </div>
          <div class="edit-field">
            <label class="edit-field__label">协议</label>
            <input type="text" class="edit-field__input"
              placeholder="如 modbus-tcp, mqtt" .value=${this.formProtocol}
              @input=${(e: any) => { this.formProtocol = e.target.value; }} />
          </div>
          <div class="edit-field">
            <label class="edit-field__label">位置</label>
            <input type="text" class="edit-field__input"
              placeholder="如 机房A-3F" .value=${this.formPosition}
              @input=${(e: any) => { this.formPosition = e.target.value; }} />
          </div>
          <div class="edit-field">
            <label class="edit-field__label">厂商</label>
            <input type="text" class="edit-field__input"
              placeholder="可选" .value=${this.formManufacturer}
              @input=${(e: any) => { this.formManufacturer = e.target.value; }} />
          </div>
          <div class="edit-field">
            <label class="edit-field__label">型号</label>
            <input type="text" class="edit-field__input"
              placeholder="可选" .value=${this.formModel}
              @input=${(e: any) => { this.formModel = e.target.value; }} />
          </div>
        </div>
        <div class="edit-field edit-field--full" style="margin-top:4px">
          <label class="edit-field__label">描述</label>
          <textarea class="edit-field__textarea"
            placeholder="可选的设备描述信息" .value=${this.formDescription}
            @input=${(e: any) => { this.formDescription = e.target.value; }} rows="2"></textarea>
        </div>
        <!-- Tags -->
        <div class="edit-field edit-field--full" style="margin-top:8px">
          <label class="edit-field__label">标签</label>
          <div class="edit-tags-bar">
            ${this.editingDevice?.tags?.map((t: Tag) => html`
              <span class="edit-tag-pill">
                ${t.name}
                <button class="edit-tag-pill__remove" @click=${() => this.toggleTag(this.editingDevice!, t)} title="移除绑定">&times;</button>
              </span>
            `)}
            ${(!this.editingDevice?.tags || this.editingDevice.tags.length === 0) ? html`<span class="edit-hint" style="padding:0">暂无标签</span>` : nothing}
            <button class="edit-tag-add-btn" @click=${() => { this.editingTagsDeviceId = this.editingTagsDeviceId ? null : this.editingDevice?.id || null; }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="12" height="12">
                <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
              </svg>
              添加标签
            </button>
            ${this.editingTagsDeviceId ? html`
              <div class="edit-inline-tag-popover">
                <input type="text" class="tag-popover__search" style="margin:0"
                  placeholder="搜索或输入新标签..."
                  .value=${this.tagSearchKeyword}
                  @input=${(e: Event) => { this.tagSearchKeyword = (e.target as HTMLInputElement).value; }}
                  @keydown=${(e: KeyboardEvent) => {
                    if (e.key === 'Enter') {
                      const kw = this.tagSearchKeyword.trim();
                      if (kw && this.editingDevice && !this.allTags.some(t => t.name.toLowerCase() === kw.toLowerCase())) {
                        this.createAndBindTag(this.editingDevice, kw);
                      }
                    }
                  }} />
                <div style="max-height:120px;overflow-y:auto;display:flex;flex-wrap:wrap;gap:4px;padding:4px 0">
                  ${this.tagSearchKeyword.trim() && !this.allTags.some(t => t.name.toLowerCase() === this.tagSearchKeyword.trim().toLowerCase()) ? html`
                    <button class="btn btn--sm tag-btn tag-btn--create"
                      ?disabled=${this.tagCreating}
                      @click=${() => this.createAndBindTag(this.editingDevice!, this.tagSearchKeyword.trim())}>
                      + 创建「${this.tagSearchKeyword.trim()}」
                    </button>
                  ` : nothing}
                  ${this.allTags.filter(t => !this.tagSearchKeyword || t.name.toLowerCase().includes(this.tagSearchKeyword.toLowerCase())).map(t => {
                    const bound = (this.editingDevice?.tags || []).some(dt => dt.id === t.id);
                    return html`
                      <button class="btn btn--sm tag-btn ${bound ? 'tag-btn--bound' : 'tag-btn--unbound'}"
                        ?disabled=${this.tagSaving}
                        @click=${() => this.toggleTag(this.editingDevice!, t)}>
                        ${bound ? icons.check : icons.plus} ${t.name}
                      </button>
                    `;
                  })}
                </div>
              </div>
            ` : nothing}
          </div>
        </div>
      </div>
    `;
  }

  private renderDriverTab() {
    return html`
      <div class="edit-section">
        <div class="edit-section__header">
          <span class="edit-section__title">驱动配置</span>
          <span class="edit-section__hint">选择驱动后自动加载配置项</span>
        </div>
        <div class="edit-field">
          <label class="edit-field__label">驱动</label>
          <select class="edit-field__select" .value=${this.formDriver} @change=${this.onFormDriverChange}>
            <option value="">不使用驱动</option>
            ${this.driverNames.map(name => html`
              <option value=${name} ?selected=${name === this.formDriver}>${name}</option>
            `)}
          </select>
        </div>
        ${this.formDriverConfigLoading ? html`
          <div class="edit-driver-loading">
            <span class="tag-spinner"></span> 加载驱动配置...
          </div>
        ` : this.formDriverConfigOptions.length > 0 ? html`
          <div class="edit-grid edit-grid--2col" style="margin-top:12px">
            ${this.formDriverConfigOptions.map(opt => html`
              <div class="edit-field ${opt.required ? 'edit-field--required' : ''}">
                <label class="edit-field__label" title=${opt.description || ''}>${opt.label || opt.name}</label>
                ${opt.optionType === 'boolean' ? html`
                  <select class="edit-field__select"
                    .value=${this.formDriverConfig[opt.name] || ''}
                    @change=${(e: any) => { this.formDriverConfig = { ...this.formDriverConfig, [opt.name]: e.target.value }; }}>
                    <option value="true">启用</option>
                    <option value="false">禁用</option>
                  </select>
                ` : html`
                  <input type=${opt.optionType === 'number' ? 'number' : 'text'}
                    class="edit-field__input"
                    placeholder=${opt.defaultValue || ''}
                    .value=${this.formDriverConfig[opt.name] || ''}
                    @input=${(e: any) => { this.formDriverConfig = { ...this.formDriverConfig, [opt.name]: e.target.value }; }} />
                `}
              </div>
            `)}
          </div>
        ` : this.formDriver ? html`
          <div class="edit-hint">该驱动无需额外配置</div>
        ` : html`
          <div class="edit-hint">选择驱动后可配置驱动参数</div>
        `}
      </div>
    `;
  }

  private renderPropertiesTab() {
    return html`
      <div class="edit-section">
        <div class="edit-section__header">
          <span class="edit-section__title">设备属性</span>
          <button class="edit-property-add-btn" @click=${this.addFormProperty}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            添加属性
          </button>
        </div>
        ${this.formProperties.length === 0 ? html`
          <div class="edit-properties-empty">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="32" height="32" opacity="0.3">
              <path d="M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2"/>
              <rect x="9" y="3" width="6" height="4" rx="1"/>
              <path d="M9 14l2 2 4-4"/>
            </svg>
            <span>暂无自定义属性</span>
            <span class="edit-properties-empty__hint">点击「添加属性」来定义设备的数据点</span>
          </div>
        ` : html`
          <div class="edit-properties-list" style="overflow-x:auto">
            <div class="edit-properties-header">
              <span class="edit-properties-header__col edit-properties-header__col--name">属性名</span>
              <span class="edit-properties-header__col edit-properties-header__col--display">显示名</span>
              <span class="edit-properties-header__col edit-properties-header__col--type">类型</span>
              <span class="edit-properties-header__col edit-properties-header__col--value">值</span>
              <span class="edit-properties-header__col edit-properties-header__col--unit">单位</span>
              <span class="edit-properties-header__col edit-properties-header__col--min">最小</span>
              <span class="edit-properties-header__col edit-properties-header__col--max">最大</span>
              <span class="edit-properties-header__col edit-properties-header__col--desc">描述</span>
              <span class="edit-properties-header__col edit-properties-header__col--ro">只读</span>
              <span class="edit-properties-header__col edit-properties-header__col--actions"></span>
            </div>
            ${this.formProperties.map((prop, i) => html`
              <div class="edit-property-row">
                <input type="text" class="edit-property-row__input edit-property-row__input--name"
                  placeholder="属性名" .value=${prop.name}
                  @input=${(e: any) => { this.formProperties[i] = { ...prop, name: e.target.value }; this.requestUpdate(); }} />
                <input type="text" class="edit-property-row__input edit-property-row__input--display"
                  placeholder="显示名" .value=${prop.displayName || ''}
                  @input=${(e: any) => { this.formProperties[i] = { ...prop, displayName: e.target.value }; this.requestUpdate(); }} />
                <select class="edit-property-row__select"
                  .value=${prop.dataType}
                  @change=${(e: any) => { this.formProperties[i] = { ...prop, dataType: e.target.value }; this.requestUpdate(); }}>
                  <option value="number">number</option>
                  <option value="string">string</option>
                  <option value="boolean">boolean</option>
                  <option value="json">json</option>
                </select>
                ${prop.dataType === 'boolean' ? html`
                  <select class="edit-property-row__select"
                    .value=${String(prop.value)}
                    @change=${(e: any) => { this.formProperties[i] = { ...prop, value: e.target.value === 'true' }; this.requestUpdate(); }}>
                    <option value="true">true</option>
                    <option value="false">false</option>
                  </select>
                ` : html`
                  <input type=${prop.dataType === 'number' ? 'number' : 'text'}
                    class="edit-property-row__input edit-property-row__input--value"
                    placeholder="值" .value=${prop.value ?? ''}
                    @input=${(e: any) => { this.formProperties[i] = { ...prop, value: prop.dataType === 'number' ? Number(e.target.value) : e.target.value }; this.requestUpdate(); }} />
                `}
                <input type="text" class="edit-property-row__input edit-property-row__input--unit"
                  placeholder="-" .value=${prop.unit || ''}
                  @input=${(e: any) => { this.formProperties[i] = { ...prop, unit: e.target.value }; this.requestUpdate(); }} />
                <input type="number" class="edit-property-row__input edit-property-row__input--minmax"
                  placeholder="-" .value=${prop.minValue ?? ''}
                  @input=${(e: any) => { this.formProperties[i] = { ...prop, minValue: e.target.value ? Number(e.target.value) : undefined }; this.requestUpdate(); }} />
                <input type="number" class="edit-property-row__input edit-property-row__input--minmax"
                  placeholder="-" .value=${prop.maxValue ?? ''}
                  @input=${(e: any) => { this.formProperties[i] = { ...prop, maxValue: e.target.value ? Number(e.target.value) : undefined }; this.requestUpdate(); }} />
                <input type="text" class="edit-property-row__input edit-property-row__input--desc"
                  placeholder="-" .value=${prop.description || ''}
                  @input=${(e: any) => { this.formProperties[i] = { ...prop, description: e.target.value }; this.requestUpdate(); }} />
                <label class="edit-property-row__checkbox">
                  <input type="checkbox" ?checked=${prop.isReadOnly}
                    @change=${(e: any) => { this.formProperties[i] = { ...prop, isReadOnly: e.target.checked }; this.requestUpdate(); }} />
                </label>
                <button class="edit-property-row__remove" title="删除"
                  @click=${() => { this.formProperties = this.formProperties.filter((_, j) => j !== i); }}>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
                    <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
                  </svg>
                </button>
              </div>
            `)}
          </div>
        `}
      </div>
    `;
  }

  private addFormProperty() {
    this.formProperties = [...this.formProperties, { name: '', displayName: '', value: '', dataType: 'number', unit: '', isReadOnly: false, description: '' }];
    this.requestUpdate();
  }

  private renderCommandsTab() {
    return html`
      <div class="edit-section">
        <div class="edit-section__header">
          <span class="edit-section__title">设备命令</span>
          <button class="edit-property-add-btn" @click=${this.addFormCommand}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            添加命令
          </button>
        </div>
        ${this.formCommands.length === 0 ? html`
          <div class="edit-properties-empty">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="32" height="32" opacity="0.3">
              <polygon points="5 3 19 12 5 21 5 3"/>
            </svg>
            <span>暂无命令定义</span>
            <span class="edit-properties-empty__hint">添加命令以支持远程控制设备</span>
          </div>
        ` : html`
          <div class="edit-properties-list" style="overflow-x:auto">
            <div class="edit-properties-header">
              <span class="edit-properties-header__col edit-properties-header__col--name">命令名</span>
              <span class="edit-properties-header__col" style="grid-column:span 2">描述</span>
              <span class="edit-properties-header__col">参数 (JSON)</span>
              <span class="edit-properties-header__col edit-properties-header__col--actions"></span>
            </div>
            ${this.formCommands.map((cmd, i) => html`
              <div class="edit-property-row" style="grid-template-columns:1fr 1fr 1fr 1fr 26px">
                <input type="text" class="edit-property-row__input edit-property-row__input--name"
                  placeholder="命令名" .value=${cmd.name}
                  @input=${(e: any) => { this.formCommands[i] = { ...cmd, name: e.target.value }; this.requestUpdate(); }} />
                <input type="text" class="edit-property-row__input" style="grid-column:span 2"
                  placeholder="可选描述" .value=${cmd.description || ''}
                  @input=${(e: any) => { this.formCommands[i] = { ...cmd, description: e.target.value }; this.requestUpdate(); }} />
                <input type="text" class="edit-property-row__input"
                  placeholder='{}' .value=${cmd.parameters || ''}
                  @input=${(e: any) => { this.formCommands[i] = { ...cmd, parameters: e.target.value }; this.requestUpdate(); }} />
                <button class="edit-property-row__remove" title="删除"
                  @click=${() => { this.formCommands = this.formCommands.filter((_, j) => j !== i); }}>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
                    <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
                  </svg>
                </button>
              </div>
            `)}
          </div>
        `}
      </div>
    `;
  }

  private addFormCommand() {
    this.formCommands = [...this.formCommands, { name: '', description: '', parameters: '' }];
    this.requestUpdate();
  }

  renderWizard() {
    const isStep1 = this.wizardStep === "template";
    return html`
      <div class="wizard-overlay" role="dialog" aria-modal="true" aria-label="设备创建向导" @click=${(e: Event) => { if ((e.target as HTMLElement).classList.contains('wizard-overlay')) this.closeWizard(); }} @keydown=${(e: KeyboardEvent) => this.handleModalKeydown(e, this.closeWizard)}>
        <div class="wizard-dialog">
          <!-- Header -->
          <div class="wizard-dialog__header">
            <button class="wizard-dialog__back" aria-label="返回" @click=${isStep1 ? this.closeWizard : this.wizardBack}>
              <span class="rotate-90">${icons.arrowDown}</span>
              <span>${isStep1 ? "返回设备列表" : "返回模板选择"}</span>
            </button>
            <span class="wizard-dialog__title">${isStep1 ? "选择设备模板" : "填写设备信息"}</span>
            <button class="modal-close wizard-dialog__close" aria-label="关闭" @click=${this.closeWizard}>✕</button>
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
      <div class="wizard-search">
        <span class="wizard-search__icon">
          ${icons.search}
        </span>
        <input
          type="text"
          class="wizard-search__input"
          placeholder="搜索设备模板..."
          .value=${this.wizTemplateSearch}
          @input=${(e: Event) => { this.wizTemplateSearch = (e.target as HTMLInputElement).value; }}
        />
      </div>

      ${this.wizTemplateLoading ? html`
        <div class="wizard-loading">
          <span class="loading-spinner"></span>
          <span class="wizard-loading__text">加载中...</span>
        </div>
      ` : this.filteredWizardTemplates.length === 0 ? html`
        <div class="wizard-empty">
          <div class="wizard-empty__icon">📦</div>
          <div class="wizard-empty__title">没有找到匹配的模板</div>
          <div class="wizard-empty__hint">尝试调整搜索条件或浏览其他分类</div>
        </div>
      ` : html`
        ${categories.map(cat => html`
          <div class="wizard-category">
            <div class="wizard-category__header">
              <span class="wizard-category__title">${CATEGORY_LABELS[cat] || cat}</span>
              <span class="wizard-category__count">${groups[cat].length} 个模板</span>
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
        class="card template-card"
        @click=${() => this.selectTemplate(t)}
      >
        <div class="template-card__header">
          <span class="template-card__icon">${CATEGORY_ICONS[t.category] || CATEGORY_ICONS.others}</span>
          <div class="template-card__title-wrap">
            <div class="template-card__title">${displayName}</div>
            ${t.manufacturer ? html`<div class="inline-muted">${t.manufacturer}</div>` : nothing}
          </div>
          ${t.isBuiltin ? html`<span class="template-card__badge">内置</span>` : nothing}
        </div>
        <div class="template-card__meta">
          ${t.deviceType ? html`<span>${t.deviceType}</span>` : nothing}
          ${t.protocolType ? html`<span>${t.protocolType}</span>` : nothing}
          ${t.version ? html`<span>v${t.version}</span>` : nothing}
        </div>
        <div class="template-card__stats">
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
        <div class="wizard-split__form wizard-fields">
          <div class="wizard-form-header">
            <div class="wizard-form-header__title">填写设备信息</div>
            <button class="btn btn--ghost btn--sm" @click=${this.wizardBack}>切换模板</button>
          </div>

          <!-- Template summary chip -->
          <div class="template-chip">
            <span class="template-chip__icon">${CATEGORY_ICONS[t.category] || CATEGORY_ICONS.others}</span>
            <div class="template-chip__title-wrap">
              <div class="template-chip__title">${displayName}</div>
              <div class="template-chip__meta">
                ${t.manufacturer ? html`<span>${t.manufacturer} · </span>` : nothing}
                <span>${t.deviceType || t.category}</span>
                ${t.version ? html` · v${t.version}` : nothing}
              </div>
            </div>
            ${t.isBuiltin ? html`<span class="template-chip__badge">内置</span>` : nothing}
          </div>

          <!-- Device name -->
          <div class="field ${hasError('deviceName') ? 'field--error' : ''}">
            <span>设备名称 <span class="form-label-required">*</span></span>
            <input
              type="text"
              placeholder="请输入设备名称"
              .value=${this.wizName}
              @input=${(e: any) => { this.wizName = e.target.value; }}
            />
            ${hasError("deviceName") ? html`<div class="form-error">${getError("deviceName")}</div>` : nothing}
          </div>

          <!-- Device description -->
          <div class="field">
            <span>设备描述 <span class="inline-muted">(可选)</span></span>
            <textarea
              placeholder="请输入设备描述"
              rows="2"
              .value=${this.wizDescription}
              @input=${(e: any) => { this.wizDescription = e.target.value; }}
            ></textarea>
          </div>

          <!-- Device address -->
          <div class="field ${hasError('deviceAddress') ? 'field--error' : ''}">
            <span>设备地址 ${isFieldRequired(t.deviceInfo, "address")
              ? html`<span class="form-label-required">*</span>`
              : html`<span class="inline-muted">(可选)</span>`}</span>
            <input
              type="text"
              placeholder="请输入设备IP地址或连接地址"
              .value=${this.wizAddress}
              @input=${(e: any) => { this.wizAddress = e.target.value; }}
            />
            ${hasError("deviceAddress") ? html`<div class="form-error">${getError("deviceAddress")}</div>` : nothing}
          </div>

          <!-- Device position -->
          <div class="field">
            <span>安装位置 <span class="inline-muted">(可选)</span></span>
            <input
              type="text"
              placeholder="请输入设备安装位置"
              .value=${this.wizPosition}
              @input=${(e: any) => { this.wizPosition = e.target.value; }}
            />
          </div>

          <!-- Driver select -->
          <div class="field">
            <span>设备驱动 <span class="inline-muted">(选择适合的驱动程序)</span></span>
            <select .value=${this.wizDriver} @change=${(e: Event) => this.onWizardDriverSelect((e.target as HTMLSelectElement).value)}>
              <option value="">请选择驱动</option>
              ${this.driverNames.map(name => html`<option value=${name}>${name}</option>`)}
            </select>
            ${t.driverName && this.wizDriver !== t.driverName ? html`
              <div class="form-hint">模板默认驱动: ${t.driverName}</div>
            ` : nothing}
          </div>

          <!-- Driver config -->
          ${this.wizDriver ? html`
            <div class="wizard-form-section">
              <div class="wizard-form-section__header">
                <span class="wizard-form-section__title">驱动配置</span>
                <span class="wizard-form-section__meta">(${this.wizDriver})</span>
              </div>
              ${this.wizConfigLoading ? html`
                <div class="wizard-loading wizard-loading--compact">
                  <span class="loading-spinner"></span>
                  <span class="wizard-loading__text">加载驱动配置参数...</span>
                </div>
              ` : this.wizConfigOptions.length > 0 ? html`
                ${this.wizConfigOptions.map(opt => this.renderWizardConfigField(opt))}
              ` : html`
                <div class="empty-hint--sm">
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
      <div class="field ${hasError ? 'field--error' : ''}">
        <span>
          ${opt.label}
          ${opt.required ? html`<span class="form-label-required">*</span>` : html`<span class="inline-muted">(可选)</span>`}
          ${opt.defaultValue ? html`<span class="inline-muted inline-muted--spaced">· 默认: ${opt.defaultValue}</span>` : nothing}
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
          }} />
        ` : html`
          <input type="text" .value=${value} placeholder=${placeholder} @input=${(e: any) => {
            this.wizDriverConfig = { ...this.wizDriverConfig, [opt.name]: e.target.value };
          }} />
        `}
        ${hasError ? html`<div class="form-error">${errorMsg}</div>` : nothing}
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
      <div class="template-overview__summary">
        <span class="template-overview__icon">${CATEGORY_ICONS[t.category] || CATEGORY_ICONS.others}</span>
        <div class="template-overview__title-wrap">
          <div class="template-overview__title">${displayName}</div>
          <div class="template-overview__meta">
            ${t.manufacturer ? html`${t.manufacturer} · ` : nothing}${t.deviceType || t.category}${t.version ? html` · v${t.version}` : nothing}
          </div>
        </div>
        ${t.isBuiltin ? html`<span class="template-overview__badge">内置</span>` : nothing}
      </div>

      <!-- Description -->
      ${description ? html`
        <div class="template-overview__desc">
          ${description}
        </div>
      ` : nothing}

      <!-- Meta info -->
      <div class="template-overview__meta-tags">
        ${t.protocolType ? html`<span class="template-overview__meta-tag">协议: ${t.protocolType}</span>` : nothing}
        ${t.driverName ? html`<span class="template-overview__meta-tag">驱动: ${t.driverName}</span>` : nothing}
        ${t.category ? html`<span class="template-overview__meta-tag">${CATEGORY_LABELS[t.category] || t.category}</span>` : nothing}
      </div>

      <!-- Tags -->
      ${t.tags && t.tags.length > 0 ? html`
        <div class="template-overview__tags">
          ${t.tags.map(tag => html`<span class="template-overview__tag">${tag}</span>`)}
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
        <ul class="wizard-overview__list template-overview__list">
          ${t.properties.map((p: any) => html`
            <li class="wizard-overview__list-item">
              <div class="template-overview__list-item-inner">
                <span class="wizard-overview__list-item-name">${p.name || p.displayName || "unnamed"}</span>
                ${p.accessMode === "r" || p.accessMode === "R"
                  ? html`<span class="template-overview__list-badge-ro">R</span>`
                  : html`<span class="template-overview__list-badge-rw">RW</span>`
                }
              </div>
              <span class="wizard-overview__list-item-meta">
                ${p.dataType || ""}${p.unit ? ` ${p.unit}` : ""}
                ${p.minValue != null || p.maxValue != null
                  ? html` <span class="template-overview__range">[${p.minValue ?? '–'}~${p.maxValue ?? '–'}]</span>`
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
        <ul class="wizard-overview__list template-overview__list--commands">
          ${t.commands.map((c: any) => html`
            <li class="wizard-overview__list-item">
              <div class="template-overview__list-item-inner">
                <span class="wizard-overview__list-item-name">${c.name || "unnamed"}</span>
                ${c.parameters && c.parameters.length > 0
                  ? html`<span class="template-overview__param-count">${c.parameters.length} 参数</span>`
                  : nothing
                }
              </div>
              <span class="wizard-overview__list-item-meta">${c.description || ""}</span>
            </li>
          `)}
        </ul>
      ` : nothing}

      ${totalProps === 0 && totalCmds === 0 ? html`
        <div class="empty-hint--sm">
          该模板暂无属性和命令定义
        </div>
      ` : nothing}
    `;
  }
}
