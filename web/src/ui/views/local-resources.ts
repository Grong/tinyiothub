import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { templateApi } from "../../api/templates.js";
import { driverApi } from "../../api/drivers.js";
import { marketplaceApi } from "../../api/marketplace.js";
import { tagApi } from "../../api/tags.js";
import type { CreateTemplateRequest, Tag } from "../../types/index.js";
import { success, error as toastError } from "../components/toast.js";

interface ProcessedTemplate {
  id: string;
  name: string;
  displayName: Record<string, string>;
  description: Record<string, string> | null;
  category: string;
  version: string;
  manufacturer: string | null;
  deviceType: string;
  protocolType: string;
  driverName: string;
  tags: string[];
  deviceInfo: Record<string, unknown>;
  properties: unknown[];
  commands: unknown[];
  isBuiltin: boolean;
}

interface ProcessedDriver {
  id: string;
  name: string;
  version: string;
  className: string;
  deviceNum: number;
  description: string;
  optionsDescriptors: OptionDescriptor[];
  location: string | null;
  createdAt: string;
  updatedAt: string;
}

interface OptionDescriptor {
  label?: string;
  name: string;
  default_value?: string;
  option_type?: string;
  required?: boolean;
  description?: string | null;
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
    deviceInfo: parseJsonField(raw.deviceInfo, { defaultNamePattern: raw.name, requiredFields: [] }),
    properties: parseJsonField(raw.properties, []),
    commands: parseJsonField(raw.commands, []),
    isBuiltin: raw.isBuiltin === 1 || raw.isBuiltin === true,
  };
}

function getLocalizedText(obj: Record<string, string> | null | undefined, fallback: string): string {
  if (!obj || typeof obj !== "object") return fallback;
  return obj["zh"] || obj["en"] || Object.values(obj)[0] || fallback;
}

function formatDate(dateStr: string): string {
  if (!dateStr) return "-";
  return dateStr.replace(" ", "T").slice(0, 16);
}

function transformDriver(raw: any): ProcessedDriver {
  return {
    id: raw.id,
    name: raw.name,
    version: raw.version || "",
    className: raw.class_name || "",
    deviceNum: raw.device_num || 0,
    description: raw.description || "",
    optionsDescriptors: parseJsonField<OptionDescriptor[]>(raw.options_descriptors, []),
    location: raw.location || null,
    createdAt: raw.created_at || "",
    updatedAt: raw.updated_at || "",
  };
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

@customElement("view-local-resources")
export class LocalResourcesView extends LitElement {
  @state() activeTab: "templates" | "drivers" | "tags" = "templates";
  @state() loading = true;
  @state() error = "";
  @state() templates: ProcessedTemplate[] = [];
  @state() page = 1;
  @state() pageSize = 20;
  @state() totalPages = 0;
  @state() totalCount = 0;
  @state() searchKeyword = "";

  @state() showModal = false;
  @state() editingTemplate: ProcessedTemplate | null = null;
  @state() selectedTemplate: ProcessedTemplate | null = null;
  @state() detailTab = "props";
  @state() saving = false;
  @state() publishing = false;
  @state() formName = "";
  @state() formDisplayName = "";
  @state() formCategory = "";
  @state() formVersion = "";
  @state() formDescription = "";
  @state() formProtocolType = "";
  @state() formManufacturer = "";
  @state() formDeviceType = "";
  @state() formDriverName = "";
  @state() formTags = "";
  @state() formDefaultNamePattern = "";
  @state() formRequiredFields = "";
  @state() formProperties: any[] = [];
  @state() formCommands: any[] = [];
  @state() modalTab: "basic" | "properties" | "commands" = "basic";
  // Property inline editor
  @state() editingPropIndex = -1;
  @state() propName = "";
  @state() propDisplayName = "";
  @state() propDataType = "number";
  @state() propUnit = "";
  @state() propDefaultValue = "";
  @state() propMinValue = "";
  @state() propMaxValue = "";
  @state() propIsReadOnly = false;
  @state() propIsRequired = true;
  @state() propDesc = "";
  // Command inline editor
  @state() editingCmdIndex = -1;
  @state() cmdName = "";
  @state() cmdDisplayName = "";
  @state() cmdDesc = "";
  @state() cmdIsRequired = true;
  @state() cmdParams: any[] = [];
  // Command param inline editor
  @state() editingParamIndex = -1;
  @state() paramName = "";
  @state() paramDisplayName = "";
  @state() paramDataType = "string";
  @state() paramDefaultValue = "";
  @state() paramRequired = false;

  // ─── Drivers state ────────────────────────────────────────────

  @state() drivers: ProcessedDriver[] = [];
  @state() selectedDriver: ProcessedDriver | null = null;
  @state() editingDriver: ProcessedDriver | null = null;
  @state() dFormName = "";
  @state() dFormVersion = "";
  @state() dFormDescription = "";

  // ─── Tags state ──────────────────────────────────────────────

  @state() tags: Tag[] = [];
  @state() editingTag: Tag | null = null;
  @state() savingTag = false;
  @state() tagFormName = "";
  @state() tagFormType = "";
  @state() tagFormDescription = "";
  @state() tagFormColor = "";

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadData();
  }

  switchTab(tab: "templates" | "drivers" | "tags") {
    this.activeTab = tab;
    this.page = 1;
    this.searchKeyword = "";
    this.selectedTemplate = null;
    this.selectedDriver = null;
    this.editingTag = null;
    this.loadData();
  }

  async loadData() {
    this.loading = true;
    this.error = "";
    if (this.activeTab === "templates") {
      await this.loadTemplates();
    } else if (this.activeTab === "drivers") {
      await this.loadDrivers();
    } else {
      await this.loadTags();
    }
    this.loading = false;
  }

  async loadTemplates() {
    try {
      const params: any = { page: this.page, pageSize: this.pageSize };
      if (this.searchKeyword) params.keyword = this.searchKeyword;
      const res = await templateApi.getTemplates(params);
      const data = res.result;
      if (Array.isArray(data)) {
        this.templates = data.map(transformTemplate);
        this.totalPages = 1;
        this.totalCount = data.length;
      } else if (data?.data) {
        this.templates = (data.data || []).map(transformTemplate);
        this.totalPages = data.pagination?.totalPages || 1;
        this.totalCount = data.pagination?.totalCount || data.data.length;
      }
    } catch (err: any) {
      this.error = err.message || "加载设备模板失败";
    }
  }

  async loadDrivers() {
    try {
      const res = await driverApi.getDrivers({ page: this.page, pageSize: this.pageSize });
      const data = res.result;
      if (Array.isArray(data)) {
        this.drivers = data.map(transformDriver);
        this.totalPages = 1;
        this.totalCount = data.length;
      } else if (data?.data) {
        this.drivers = (data.data || []).map(transformDriver);
        this.totalPages = data.pagination?.totalPages || 1;
        this.totalCount = data.pagination?.totalCount || data.data.length;
      }
    } catch (err: any) {
      this.error = err.message || "加载驱动列表失败";
    }
  }

  async loadTags() {
    try {
      const res = await tagApi.getTags();
      const data = res.result;
      if (Array.isArray(data)) {
        this.tags = data;
      } else if (data?.data) {
        this.tags = data.data;
      }
    } catch (err: any) {
      this.error = err.message || "加载标签失败";
    }
  }

  // ─── Tags CRUD ──────────────────────────────────────────────

  openTagCreate() {
    this.editingTag = null;
    this.tagFormName = "";
    this.tagFormType = "";
    this.tagFormDescription = "";
    this.tagFormColor = "#3b82f6";
    this.showModal = true;
  }

  openTagEdit(tag: Tag) {
    this.editingTag = tag;
    this.tagFormName = tag.name;
    this.tagFormType = tag.type;
    this.tagFormDescription = tag.description || "";
    this.tagFormColor = tag.color || "#3b82f6";
    this.showModal = true;
  }

  closeModal() {
    this.showModal = false;
    this.editingTemplate = null;
    this.editingTag = null;
    this.editingDriver = null;
  }

  async saveTagForm() {
    if (!this.tagFormName.trim() || !this.tagFormType.trim()) return;
    this.savingTag = true;
    try {
      if (this.editingTag) {
        await tagApi.updateTag(this.editingTag.id, { name: this.tagFormName });
        success("标签已更新");
      } else {
        await tagApi.createTag({ name: this.tagFormName, type: this.tagFormType, description: this.tagFormDescription || undefined, color: this.tagFormColor || undefined });
        success("标签已创建");
      }
      this.closeModal();
      await this.loadTags();
    } catch (err: any) {
      toastError(err.message || "操作失败");
    } finally {
      this.savingTag = false;
    }
  }

  async deleteTag(tag: Tag) {
    if (!confirm(`确定要删除标签 "${tag.name}" 吗？`)) return;
    try {
      await tagApi.deleteTag(tag.id);
      success("标签已删除");
      await this.loadTags();
    } catch (err: any) {
      toastError(err.message || "删除失败");
    }
  }

  openCreate() {
    this.editingTemplate = null;
    this.formName = "";
    this.formDisplayName = "";
    this.formCategory = "";
    this.formVersion = "1.0.0";
    this.formDescription = "";
    this.formProtocolType = "";
    this.formManufacturer = "";
    this.formDeviceType = "";
    this.formDriverName = "";
    this.formTags = "";
    this.formDefaultNamePattern = "";
    this.formRequiredFields = "";
    this.formProperties = [];
    this.formCommands = [];
    this.modalTab = "basic";
    this.resetPropForm();
    this.resetCmdForm();
    this.showModal = true;
  }

  openEdit(t: ProcessedTemplate) {
    this.editingTemplate = t;
    this.formName = t.name;
    this.formDisplayName = getLocalizedText(t.displayName, "");
    this.formCategory = t.category || "";
    this.formVersion = t.version || "";
    this.formDescription = getLocalizedText(t.description, "");
    this.formProtocolType = t.protocolType || "";
    this.formManufacturer = t.manufacturer || "";
    this.formDeviceType = t.deviceType || "";
    this.formDriverName = t.driverName || "";
    this.formTags = (t.tags || []).join(", ");
    this.formDefaultNamePattern = (t.deviceInfo as any)?.defaultNamePattern || (t.deviceInfo as any)?.default_name_pattern || "";
    this.formRequiredFields = ((t.deviceInfo as any)?.requiredFields || (t.deviceInfo as any)?.required_fields || []).join(", ");
    this.formProperties = JSON.parse(JSON.stringify(t.properties || []));
    this.formCommands = JSON.parse(JSON.stringify(t.commands || []));
    this.modalTab = "basic";
    this.resetPropForm();
    this.resetCmdForm();
    this.showModal = true;
  }

  // ─── Driver CRUD ───────────────────────────────────────────────

  openDriverCreate() {
    this.editingDriver = null;
    this.dFormName = "";
    this.dFormVersion = "";
    this.dFormDescription = "";
    this.showModal = true;
  }

  openDriverEdit(d: ProcessedDriver) {
    this.editingDriver = d;
    this.dFormName = d.name;
    this.dFormVersion = d.version || "";
    this.dFormDescription = d.description || "";
    this.showModal = true;
  }

  async saveDriverForm() {
    if (!this.dFormName.trim()) return;
    this.saving = true;
    try {
      const payload: any = {
        name: this.dFormName.trim(),
        version: this.dFormVersion || undefined,
        description: this.dFormDescription || undefined,
      };
      if (this.editingDriver) {
        await driverApi.updateDriver(this.editingDriver.id, payload);
        success("驱动已更新");
      } else {
        await driverApi.createDriver(payload);
        success("驱动已创建");
      }
      this.closeModal();
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "操作失败");
    } finally {
      this.saving = false;
    }
  }

  async deleteDriver(d: ProcessedDriver) {
    if (!confirm(`确定要删除驱动 "${d.name}" 吗？`)) return;
    try {
      await driverApi.deleteDriver(d.id);
      success("驱动已删除");
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "删除失败");
    }
  }

  navigateTo(route: string) {
    window.history.pushState({}, "", `/${route}`);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }

  // ─── Property helpers ────────────────────────────────────────

  resetPropForm() {
    this.editingPropIndex = -1;
    this.propName = "";
    this.propDisplayName = "";
    this.propDataType = "number";
    this.propUnit = "";
    this.propDefaultValue = "";
    this.propMinValue = "";
    this.propMaxValue = "";
    this.propIsReadOnly = false;
    this.propIsRequired = true;
    this.propDesc = "";
  }

  startEditProp(index: number) {
    const p = this.formProperties[index];
    if (!p) return;
    this.editingPropIndex = index;
    this.propName = p.name || "";
    this.propDisplayName = getLocalizedText(p.displayName, "");
    this.propDataType = p.dataType || "number";
    this.propUnit = p.unit || "";
    this.propDefaultValue = p.defaultValue ?? "";
    this.propMinValue = p.minValue ?? "";
    this.propMaxValue = p.maxValue ?? "";
    this.propIsReadOnly = p.isReadOnly === true;
    this.propIsRequired = p.isRequired !== false;
    this.propDesc = getLocalizedText(p.description, "");
  }

  saveProp() {
    if (!this.propName.trim()) return;
    const prop: any = {
      name: this.propName.trim(),
      displayName: { zh: this.propDisplayName || this.propName },
      dataType: this.propDataType,
      isReadOnly: this.propIsReadOnly,
      isRequired: this.propIsRequired,
    };
    if (this.propUnit) prop.unit = this.propUnit;
    if (this.propDefaultValue !== "" && this.propDefaultValue !== undefined)
      prop.defaultValue = this.propDefaultValue;
    if (this.propMinValue !== "" && this.propMinValue !== undefined)
      prop.minValue = Number(this.propMinValue);
    if (this.propMaxValue !== "" && this.propMaxValue !== undefined)
      prop.maxValue = Number(this.propMaxValue);
    if (this.propDesc) prop.description = { zh: this.propDesc };

    if (this.editingPropIndex >= 0) {
      this.formProperties[this.editingPropIndex] = prop;
    } else {
      this.formProperties = [...this.formProperties, prop];
    }
    this.resetPropForm();
    this.requestUpdate();
  }

  removeProp(index: number) {
    this.formProperties = this.formProperties.filter((_, i) => i !== index);
    if (this.editingPropIndex === index) this.resetPropForm();
    if (this.editingPropIndex > index) this.editingPropIndex--;
    this.requestUpdate();
  }

  // ─── Command helpers ─────────────────────────────────────────

  resetCmdForm() {
    this.editingCmdIndex = -1;
    this.cmdName = "";
    this.cmdDisplayName = "";
    this.cmdDesc = "";
    this.cmdIsRequired = true;
    this.cmdParams = [];
    this.resetParamForm();
  }

  startEditCmd(index: number) {
    const c = this.formCommands[index];
    if (!c) return;
    this.editingCmdIndex = index;
    this.cmdName = c.name || "";
    this.cmdDisplayName = getLocalizedText(c.displayName, "");
    this.cmdDesc = getLocalizedText(c.description, "");
    this.cmdIsRequired = c.isRequired !== false;
    this.cmdParams = JSON.parse(JSON.stringify(c.parameters || []));
    this.resetParamForm();
  }

  saveCmd() {
    if (!this.cmdName.trim()) return;
    const cmd: any = {
      name: this.cmdName.trim(),
      displayName: { zh: this.cmdDisplayName || this.cmdName },
      isRequired: this.cmdIsRequired,
    };
    if (this.cmdDesc) cmd.description = { zh: this.cmdDesc };
    if (this.cmdParams.length > 0) cmd.parameters = this.cmdParams;

    if (this.editingCmdIndex >= 0) {
      this.formCommands[this.editingCmdIndex] = cmd;
    } else {
      this.formCommands = [...this.formCommands, cmd];
    }
    this.resetCmdForm();
    this.requestUpdate();
  }

  removeCmd(index: number) {
    this.formCommands = this.formCommands.filter((_, i) => i !== index);
    if (this.editingCmdIndex === index) this.resetCmdForm();
    if (this.editingCmdIndex > index) this.editingCmdIndex--;
    this.requestUpdate();
  }

  // ─── Command param helpers ───────────────────────────────────

  resetParamForm() {
    this.editingParamIndex = -1;
    this.paramName = "";
    this.paramDisplayName = "";
    this.paramDataType = "string";
    this.paramDefaultValue = "";
    this.paramRequired = false;
  }

  startEditParam(index: number) {
    const p = this.cmdParams[index];
    if (!p) return;
    this.editingParamIndex = index;
    this.paramName = p.name || "";
    this.paramDisplayName = getLocalizedText(p.displayName, "");
    this.paramDataType = p.dataType || "string";
    this.paramDefaultValue = p.defaultValue ?? "";
    this.paramRequired = p.isRequired === true;
  }

  saveParam() {
    if (!this.paramName.trim()) return;
    const param: any = {
      name: this.paramName.trim(),
      displayName: { zh: this.paramDisplayName || this.paramName },
      dataType: this.paramDataType,
      isRequired: this.paramRequired,
    };
    if (this.paramDefaultValue !== "" && this.paramDefaultValue !== undefined)
      param.defaultValue = this.paramDefaultValue;

    if (this.editingParamIndex >= 0) {
      this.cmdParams[this.editingParamIndex] = param;
    } else {
      this.cmdParams = [...this.cmdParams, param];
    }
    this.resetParamForm();
    this.requestUpdate();
  }

  removeParam(index: number) {
    this.cmdParams = this.cmdParams.filter((_, i) => i !== index);
    if (this.editingParamIndex === index) this.resetParamForm();
    if (this.editingParamIndex > index) this.editingParamIndex--;
    this.requestUpdate();
  }

  async saveForm() {
    if (!this.formName.trim() || !this.formCategory.trim() || !this.formVersion.trim()) return;
    this.saving = true;
    try {
      const tags = this.formTags
        ? this.formTags.split(",").map(s => s.trim()).filter(Boolean)
        : [];
      const requiredFields = this.formRequiredFields
        ? this.formRequiredFields.split(",").map(s => s.trim()).filter(Boolean)
        : [];
      const deviceInfo: any = {};
      if (this.formDefaultNamePattern) deviceInfo.defaultNamePattern = this.formDefaultNamePattern;
      if (requiredFields.length > 0) deviceInfo.requiredFields = requiredFields;

      const payload: CreateTemplateRequest = {
        name: this.formName,
        category: this.formCategory,
        version: this.formVersion,
        displayName: { zh: this.formDisplayName } as any,
        description: this.formDescription ? { zh: this.formDescription } as any : undefined,
        protocolType: this.formProtocolType || undefined,
        manufacturer: this.formManufacturer || undefined,
        deviceType: this.formDeviceType || undefined,
        driverName: this.formDriverName || undefined,
        tags: tags.length > 0 ? tags : undefined,
        deviceInfo: Object.keys(deviceInfo).length > 0 ? deviceInfo : undefined,
        properties: this.formProperties.length > 0 ? this.formProperties : undefined,
        commands: this.formCommands.length > 0 ? this.formCommands : undefined,
      } as any;
      if (this.editingTemplate) {
        await templateApi.updateTemplate(this.editingTemplate.id, payload);
        success("模板已更新");
      } else {
        await templateApi.createTemplate(payload);
        success("模板已创建");
      }
      this.closeModal();
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "操作失败");
    } finally {
      this.saving = false;
    }
  }

  async deleteTemplate(t: ProcessedTemplate) {
    if (!confirm(`确定要删除模板 "${getLocalizedText(t.displayName, t.name)}" 吗？`)) return;
    try {
      await templateApi.deleteTemplate(t.id);
      success("模板已删除");
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "删除失败");
    }
  }

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

  goToPage(p: number) {
    this.page = p;
    this.loadData();
  }

  render() {
    if (this.loading) {
      return html`
        <div class="settings-page lr-page">
          <div class="settings-tabs">
            <button class="settings-tab active">设备模板</button>
            <button class="settings-tab">驱动管理</button>
            <button class="settings-tab">标签管理</button>
          </div>
          <div class="settings-content">
            ${this.renderSkeleton()}
          </div>
        </div>
      `;
    }

    if (this.error) {
      return html`
        <div class="settings-page lr-page">
          <div class="settings-tabs">
            <button class="settings-tab active">设备模板</button>
            <button class="settings-tab">驱动管理</button>
            <button class="settings-tab">标签管理</button>
          </div>
          <div class="settings-content">
            <div class="page-error">
              <div class="page-error__message">${this.error}</div>
              <button class="btn btn--primary" @click=${this.loadData}>重试</button>
            </div>
          </div>
        </div>
      `;
    }

    const tab = this.activeTab;
    const templateCount = this.templates.length;
    const driverCount = this.drivers.length;
    const tagCount = this.tags.length;

    const toolbarSearchPlaceholder: Record<string, string> = {
      templates: "搜索模板名称、分类、协议...",
      drivers: "搜索驱动名称、类名...",
      tags: "搜索标签名称、类型...",
    };

    const toolbarNewLabel: Record<string, string> = {
      templates: "新建模板",
      drivers: "新建驱动",
      tags: "新建标签",
    };

    const toolbarNewAction: Record<string, () => void> = {
      templates: this.openCreate,
      drivers: this.openDriverCreate,
      tags: this.openTagCreate,
    };

    const showPagination = tab !== "tags" && this.totalCount > 0;

    return html`
      <div class="settings-page lr-page">
        <!-- Left sidebar tabs -->
        <div class="settings-tabs">
          <button class="settings-tab ${tab === "templates" ? 'active' : ''}" @click=${() => this.switchTab("templates")}>
            设备模板
            ${templateCount > 0 ? html`<span style="font-size: 11px; color: var(--muted); margin-left: 4px;">${templateCount}</span>` : nothing}
          </button>
          <button class="settings-tab ${tab === "drivers" ? 'active' : ''}" @click=${() => this.switchTab("drivers")}>
            驱动管理
            ${driverCount > 0 ? html`<span style="font-size: 11px; color: var(--muted); margin-left: 4px;">${driverCount}</span>` : nothing}
          </button>
          <button class="settings-tab ${tab === "tags" ? 'active' : ''}" @click=${() => this.switchTab("tags")}>
            标签管理
            ${tagCount > 0 ? html`<span style="font-size: 11px; color: var(--muted); margin-left: 4px;">${tagCount}</span>` : nothing}
          </button>
        </div>

        <!-- Right content area -->
        <div class="settings-content">
          <div class="settings-section">
            <div class="toolbar">
              <div class="field filter-bar__search">
                <input
                  type="text"
                  placeholder=${toolbarSearchPlaceholder[tab]}
                  .value=${this.searchKeyword}
                  @keydown=${(e: KeyboardEvent) => { if (e.key === "Enter") { this.page = 1; this.loadData(); } }}
                  @input=${(e: Event) => { this.searchKeyword = (e.target as HTMLInputElement).value; }}
                />
              </div>
              <button class="btn btn--primary" @click=${toolbarNewAction[tab]}>
                ${toolbarNewLabel[tab]}
              </button>
            </div>

            ${tab === "templates" ? this.renderTemplatesTable() : tab === "drivers" ? this.renderDriversTable() : this.renderTagsTable()}

            ${showPagination ? html`
              <div class="templates-pagination">
                <button class="btn btn--ghost btn--sm" ?disabled=${this.page <= 1} @click=${() => this.goToPage(this.page - 1)}>上一页</button>
                <span class="page-meta">第 ${this.page} / ${this.totalPages} 页，共 ${this.totalCount} 条</span>
                <button class="btn btn--ghost btn--sm" ?disabled=${this.page >= this.totalPages} @click=${() => this.goToPage(this.page + 1)}>下一页</button>
              </div>
            ` : ""}
          </div>
        </div>
      </div>
      ${this.showModal ? (tab === "templates" ? this.renderTemplateModal() : tab === "drivers" ? this.renderDriverModal() : this.renderTagModal()) : nothing}
      ${this.selectedTemplate ? this.renderTemplateDetailModal() : nothing}
      ${this.selectedDriver ? this.renderDriverDetailModal() : nothing}
    `;
  }

  renderSkeleton() {
    return html`
      <div class="settings-section">
        <div class="toolbar">
          <div class="field filter-bar__search">
            <input type="text" placeholder="搜索..." disabled />
          </div>
          <button class="btn btn--primary" disabled>加载中...</button>
        </div>
        <div class="card templates-card">
          <table class="templates-table">
          <thead>
            <tr><th>名称</th><th>分类</th><th>版本</th><th>协议</th><th>标签</th><th>属性</th><th>操作</th></tr>
          </thead>
          <tbody>
            ${[1, 2, 3, 4, 5].map(() => html`
              <tr class="lr-skeleton-row">
                <td><div class="lr-skeleton" style="width: 140px; height: 16px;"></div><div class="lr-skeleton" style="width: 80px; height: 12px; margin-top: 6px;"></div></td>
                <td><div class="lr-skeleton" style="width: 48px; height: 14px;"></div></td>
                <td><div class="lr-skeleton" style="width: 36px; height: 14px;"></div></td>
                <td><div class="lr-skeleton" style="width: 56px; height: 14px;"></div></td>
                <td><div class="lr-skeleton" style="width: 72px; height: 14px;"></div></td>
                <td><div class="lr-skeleton" style="width: 24px; height: 14px;"></div></td>
                <td><div class="lr-skeleton" style="width: 80px; height: 14px;"></div></td>
              </tr>
            `)}
          </tbody>
        </table>
      </div>
    `;
  }

  // ─── Templates table ──────────────────────────────────────────

  renderTemplatesTable() {
    return html`
      <div class="card templates-card">
        <table class="templates-table">
          <thead>
            <tr>
              <th>模板名称</th>
              <th>分类</th>
              <th>版本</th>
              <th>协议</th>
              <th>标签</th>
              <th>属性</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            ${this.templates.length === 0
              ? html`<tr><td colspan="7" class="empty-center" style="padding: var(--space-9) var(--space-4);">
                <div class="empty-center__icon" style="font-size: 36px; opacity: 1;">📦</div>
                <div class="empty-center__text" style="font-weight: 600; margin-bottom: 4px;">暂无模板</div>
                <div class="empty-center__text" style="margin-bottom: var(--space-3);">创建模板来定义设备属性和命令</div>
                <button class="btn btn--primary btn--sm" @click=${this.openCreate}>新建模板</button>
              </td></tr>`
              : this.templates.map(t => html`
                <tr @click=${() => this.selectedTemplate = t}>
                  <td>
                    <div class="templates-table__name-primary">
                      <span class="templates-table__icon">${CATEGORY_ICONS[t.category] || "📦"}</span>
                      ${getLocalizedText(t.displayName, t.name)}
                    </div>
                    <div class="templates-table__name-sub">${t.name}</div>
                  </td>
                  <td>${CATEGORY_LABELS[t.category] || t.category || "—"}</td>
                  <td>v${t.version}</td>
                  <td>${t.protocolType || "—"}</td>
                  <td>
                    ${t.tags && t.tags.length > 0
                      ? t.tags.slice(0, 3).map(tag => html`<span class="tag-pill" style="font-size: 11px;">${tag}</span>`)
                      : html`<span style="color: var(--muted);">—</span>`
                    }
                  </td>
                  <td>${t.properties?.length ?? 0}</td>
                  <td class="templates-table__actions" @click=${(e: Event) => e.stopPropagation()}>
                    ${!t.isBuiltin ? html`
                      <button class="btn btn--ghost btn--sm" @click=${() => this.openEdit(t)}>编辑</button>
                      <button class="btn btn--ghost btn--sm" style="color: var(--danger);" @click=${() => this.deleteTemplate(t)}>删除</button>
                    ` : html`<span style="font-size: 12px; color: var(--muted);">内置</span>`}
                  </td>
                </tr>
              `)}
          </tbody>
        </table>
      </div>
    `;
  }

  // ─── Drivers table ────────────────────────────────────────────

  renderDriversTable() {
    return html`
      <div class="card templates-card">
        <table class="templates-table">
          <thead>
            <tr>
              <th>驱动名称</th>
              <th>版本</th>
              <th>关联设备</th>
              <th>描述</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            ${this.drivers.length === 0
              ? html`<tr><td colspan="5" class="empty-center" style="padding: var(--space-9) var(--space-4);">
                <div class="empty-center__icon" style="font-size: 36px; opacity: 1;">⚙️</div>
                <div class="empty-center__text" style="font-weight: 600; margin-bottom: 4px;">暂无驱动</div>
                <div class="empty-center__text" style="margin-bottom: var(--space-3);">创建协议驱动来连接设备</div>
                <button class="btn btn--primary btn--sm" @click=${this.openDriverCreate}>新建驱动</button>
              </td></tr>`
              : this.drivers.map(d => html`
                <tr @click=${() => this.selectedDriver = d}>
                  <td>
                    <div class="templates-table__name-primary">
                      <span class="templates-table__icon">⚙️</span>
                      ${d.name}
                    </div>
                    <div class="templates-table__name-sub">${d.className}</div>
                  </td>
                  <td>${d.version || "-"}</td>
                  <td>
                    ${d.deviceNum > 0
                      ? html`<span style="color: var(--success);">${d.deviceNum} 台设备</span>`
                      : html`<span style="color: var(--muted);">未关联</span>`
                    }
                  </td>
                  <td style="max-width: 280px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${d.description || "-"}</td>
                  <td class="templates-table__actions" @click=${(e: Event) => e.stopPropagation()}>
                    <button class="btn btn--ghost btn--sm" @click=${() => this.openDriverEdit(d)}>编辑</button>
                    <button class="btn btn--ghost btn--sm" style="color: var(--danger);" @click=${() => this.deleteDriver(d)}>删除</button>
                  </td>
                </tr>
              `)}
          </tbody>
        </table>
      </div>
    `;
  }

  // ─── Tags table ──────────────────────────────────────────────

  renderTagsTable() {
    const filtered = this.searchKeyword
      ? this.tags.filter(t =>
          t.name.toLowerCase().includes(this.searchKeyword.toLowerCase()) ||
          t.type.toLowerCase().includes(this.searchKeyword.toLowerCase()) ||
          (t.description || "").toLowerCase().includes(this.searchKeyword.toLowerCase()))
      : this.tags;

    return html`
      <div class="card templates-card">
        <table class="templates-table">
          <thead>
            <tr>
              <th>标签名称</th>
              <th>类型</th>
              <th>颜色</th>
              <th>绑定数</th>
              <th>创建时间</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            ${filtered.length === 0
              ? html`<tr><td colspan="6" class="empty-center" style="padding: var(--space-9) var(--space-4);">
                <div class="empty-center__icon" style="font-size: 36px; opacity: 1;">🏷️</div>
                <div class="empty-center__text" style="font-weight: 600; margin-bottom: 4px;">暂无标签</div>
                <div class="empty-center__text" style="margin-bottom: var(--space-3);">创建标签来分类和管理资源</div>
                <button class="btn btn--primary btn--sm" @click=${this.openTagCreate}>新建标签</button>
              </td></tr>`
              : filtered.map(t => html`
                <tr>
                  <td>
                    <div style="font-weight: 500;">${t.name}</div>
                    ${t.description ? html`<div style="font-size: 12px; color: var(--muted); margin-top: 2px;">${t.description}</div>` : nothing}
                  </td>
                  <td>${t.type}</td>
                  <td>
                    ${t.color ? html`
                      <span style="display: inline-block; width: 16px; height: 16px; border-radius: 4px; background: ${t.color}; vertical-align: middle;"></span>
                    ` : html`<span style="color: var(--muted);">—</span>`}
                  </td>
                  <td>${t.bindingCount ?? 0}</td>
                  <td style="color: var(--muted);">${(t.createdAt || "").replace(" ", "T").slice(0, 16)}</td>
                  <td class="templates-table__actions" @click=${(e: Event) => e.stopPropagation()}>
                    <button class="btn btn--ghost btn--sm" @click=${() => this.openTagEdit(t)}>编辑</button>
                    <button class="btn btn--ghost btn--sm" style="color: var(--danger);" @click=${() => this.deleteTag(t)}>删除</button>
                  </td>
                </tr>
              `)}
          </tbody>
        </table>
      </div>
    `;
  }

  renderTemplateModal() {
    const isEdit = !!this.editingTemplate;
    const propCount = this.formProperties.length;
    const cmdCount = this.formCommands.length;

    return html`
      <div class="modal-overlay template-detail-overlay" role="dialog" aria-modal="true" aria-label=${isEdit ? "编辑模板" : "新建模板"}>
        <div class="template-detail-card" @click=${(e: Event) => e.stopPropagation()}>
          <div class="tdc-header">
            <div class="tdc-header__info">
              <div class="tdc-header__title">${isEdit ? "编辑模板" : "新建模板"}</div>
            </div>
          </div>

          <!-- Tabs -->
          <div class="tdc-tabs" style="margin: 0 var(--space-5);">
            <button class="tdc-tab ${this.modalTab === 'basic' ? 'active' : ''}" @click=${() => { this.modalTab = 'basic'; }}>
              基本信息
            </button>
            <button class="tdc-tab ${this.modalTab === 'properties' ? 'active' : ''}" @click=${() => { this.modalTab = 'properties'; }}>
              属性 ${propCount > 0 ? html`<span class="tdc-tab__count">${propCount}</span>` : nothing}
            </button>
            <button class="tdc-tab ${this.modalTab === 'commands' ? 'active' : ''}" @click=${() => { this.modalTab = 'commands'; }}>
              命令 ${cmdCount > 0 ? html`<span class="tdc-tab__count">${cmdCount}</span>` : nothing}
            </button>
          </div>

          <div class="tdc-body">
            ${this.modalTab === 'basic' ? this.renderBasicTab() : nothing}
            ${this.modalTab === 'properties' ? this.renderPropertiesTab() : nothing}
            ${this.modalTab === 'commands' ? this.renderCommandsTab() : nothing}
          </div>

          <div class="tdc-footer">
            <button class="btn btn--ghost" @click=${this.closeModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.saving || !this.formName.trim() || !this.formCategory.trim() || !this.formVersion.trim()} @click=${this.saveForm}>
              ${this.saving ? "保存中..." : "保存"}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  renderBasicTab() {
    return html`
      <div class="tem-form-grid">
        <div class="field">
          <label>模板名称（标识符）<span style="color: var(--danger);">*</span></label>
          <input type="text" placeholder="如 temperature_humidity_sensor" .value=${this.formName} @input=${(e: any) => { this.formName = e.target.value; }} />
        </div>
        <div class="field">
          <label>显示名称</label>
          <input type="text" placeholder="如 温湿度传感器" .value=${this.formDisplayName} @input=${(e: any) => { this.formDisplayName = e.target.value; }} />
        </div>
        <div class="field">
          <label>分类<span style="color: var(--danger);">*</span></label>
          <input type="text" placeholder="如 sensors, cameras, controllers" .value=${this.formCategory} @input=${(e: any) => { this.formCategory = e.target.value; }} />
        </div>
        <div class="field">
          <label>版本<span style="color: var(--danger);">*</span></label>
          <input type="text" placeholder="如 1.0.0" .value=${this.formVersion} @input=${(e: any) => { this.formVersion = e.target.value; }} />
        </div>
        <div class="field">
          <label>协议类型</label>
          <input type="text" placeholder="如 modbus, mqtt, onvif" .value=${this.formProtocolType} @input=${(e: any) => { this.formProtocolType = e.target.value; }} />
        </div>
        <div class="field">
          <label>驱动名称</label>
          <input type="text" placeholder="如 modbus_rtu" .value=${this.formDriverName} @input=${(e: any) => { this.formDriverName = e.target.value; }} />
        </div>
        <div class="field">
          <label>厂商</label>
          <input type="text" placeholder="可选" .value=${this.formManufacturer} @input=${(e: any) => { this.formManufacturer = e.target.value; }} />
        </div>
        <div class="field">
          <label>设备类型</label>
          <input type="text" placeholder="如 sensor, controller, camera" .value=${this.formDeviceType} @input=${(e: any) => { this.formDeviceType = e.target.value; }} />
        </div>
        <div class="field">
          <label>描述</label>
          <input type="text" placeholder="可选" .value=${this.formDescription} @input=${(e: any) => { this.formDescription = e.target.value; }} />
        </div>
        <div class="field">
          <label>标签（逗号分隔）</label>
          <input type="text" placeholder="如 temperature, humidity, sensor" .value=${this.formTags} @input=${(e: any) => { this.formTags = e.target.value; }} />
        </div>
      </div>
      <div class="tem-section-divider">
        <span>设备信息</span>
      </div>
      <div class="tem-form-grid">
        <div class="field">
          <label>默认名称模式</label>
          <input type="text" placeholder="如 th_sensor_{index}" .value=${this.formDefaultNamePattern} @input=${(e: any) => { this.formDefaultNamePattern = e.target.value; }} />
        </div>
        <div class="field">
          <label>必填字段（逗号分隔）</label>
          <input type="text" placeholder="如 name, address" .value=${this.formRequiredFields} @input=${(e: any) => { this.formRequiredFields = e.target.value; }} />
        </div>
      </div>
    `;
  }

  renderPropertiesTab() {
    return html`
      <div class="tem-section-header">
        <span>属性定义（${this.formProperties.length}）</span>
        <button class="btn btn--sm btn--primary" @click=${() => { this.resetPropForm(); this.modalTab = 'properties'; }} ?disabled=${this.editingPropIndex === -2}>
          + 添加属性
        </button>
      </div>

      ${this.renderPropForm()}

      ${this.formProperties.length > 0 ? html`
        <div class="tem-item-list">
          ${this.formProperties.map((p: any, i: number) => html`
            <div class="tem-item ${this.editingPropIndex === i ? 'tem-item--active' : ''}">
              <div class="tem-item__info" @click=${() => this.startEditProp(i)}>
                <span class="tem-item__name">${getLocalizedText(p.displayName, p.name)}</span>
                <span class="tem-item__meta">
                  <span class="tag-pill" style="font-size: 10px;">${p.dataType}</span>
                  ${p.unit ? html`<span class="tag-pill" style="font-size: 10px;">${p.unit}</span>` : nothing}
                  ${p.isReadOnly ? html`<span class="tag-pill" style="font-size: 10px; background: var(--warn-subtle);">只读</span>` : html`<span class="tag-pill" style="font-size: 10px; background: var(--ok-subtle);">可写</span>`}
                  ${p.isRequired ? html`<span class="tag-pill" style="font-size: 10px;">必填</span>` : nothing}
                </span>
              </div>
              <button class="btn btn--ghost btn--sm" style="color: var(--danger); flex-shrink: 0;" @click=${() => this.removeProp(i)}>删除</button>
            </div>
          `)}
        </div>
      ` : html`<div class="tem-empty">暂无属性，点击上方按钮添加</div>`}
    `;
  }

  renderPropForm() {
    // Show form when adding new (editingPropIndex === -2) or editing existing (editingPropIndex >= 0)
    // We use a special flag: -2 means "show empty form for adding"
    const showForm = this.editingPropIndex >= 0;
    if (!showForm && this.editingPropIndex !== -2) {
      // Show "add" trigger
      return html`
        <div class="tem-inline-add" @click=${() => { this.resetPropForm(); this.editingPropIndex = -2; }}>
          <span>+ 点击添加新属性</span>
        </div>
      `;
    }

    const isEditing = this.editingPropIndex >= 0;
    return html`
      <div class="tem-inline-form">
        <div class="tem-inline-form__title">${isEditing ? "编辑属性" : "新增属性"}</div>
        <div class="tem-form-grid tem-form-grid--compact">
          <div class="field">
            <label>属性名<span style="color: var(--danger);">*</span></label>
            <input type="text" placeholder="如 temperature" .value=${this.propName} @input=${(e: any) => { this.propName = e.target.value; }} />
          </div>
          <div class="field">
            <label>显示名</label>
            <input type="text" placeholder="如 温度" .value=${this.propDisplayName} @input=${(e: any) => { this.propDisplayName = e.target.value; }} />
          </div>
          <div class="field">
            <label>数据类型</label>
            <select .value=${this.propDataType} @change=${(e: any) => { this.propDataType = e.target.value; }}>
              <option value="number">number</option>
              <option value="string">string</option>
              <option value="boolean">boolean</option>
              <option value="integer">integer</option>
            </select>
          </div>
          <div class="field">
            <label>单位</label>
            <input type="text" placeholder="如 °C, V, A" .value=${this.propUnit} @input=${(e: any) => { this.propUnit = e.target.value; }} />
          </div>
          <div class="field">
            <label>默认值</label>
            <input type="text" placeholder="可选" .value=${this.propDefaultValue} @input=${(e: any) => { this.propDefaultValue = e.target.value; }} />
          </div>
          <div class="field">
            <label>最小值</label>
            <input type="number" placeholder="可选" .value=${this.propMinValue} @input=${(e: any) => { this.propMinValue = e.target.value; }} />
          </div>
          <div class="field">
            <label>最大值</label>
            <input type="number" placeholder="可选" .value=${this.propMaxValue} @input=${(e: any) => { this.propMaxValue = e.target.value; }} />
          </div>
          <div class="field">
            <label>描述</label>
            <input type="text" placeholder="可选" .value=${this.propDesc} @input=${(e: any) => { this.propDesc = e.target.value; }} />
          </div>
        </div>
        <div class="tem-inline-form__checks">
          <label class="checkbox-label"><input type="checkbox" .checked=${this.propIsReadOnly} @change=${(e: any) => { this.propIsReadOnly = e.target.checked; }} /> 只读</label>
          <label class="checkbox-label"><input type="checkbox" .checked=${this.propIsRequired} @change=${(e: any) => { this.propIsRequired = e.target.checked; }} /> 必填</label>
        </div>
        <div class="tem-inline-form__actions">
          <button class="btn btn--sm btn--ghost" @click=${this.resetPropForm}>取消</button>
          <button class="btn btn--sm btn--primary" ?disabled=${!this.propName.trim()} @click=${this.saveProp}>
            ${isEditing ? "更新" : "添加"}
          </button>
        </div>
      </div>
    `;
  }

  renderCommandsTab() {
    return html`
      <div class="tem-section-header">
        <span>命令定义（${this.formCommands.length}）</span>
        <button class="btn btn--sm btn--primary" @click=${() => { this.resetCmdForm(); this.editingCmdIndex = -2; }} ?disabled=${this.editingCmdIndex === -2}>
          + 添加命令
        </button>
      </div>

      ${this.renderCmdForm()}

      ${this.formCommands.length > 0 ? html`
        <div class="tem-item-list">
          ${this.formCommands.map((c: any, i: number) => html`
            <div class="tem-item ${this.editingCmdIndex === i ? 'tem-item--active' : ''}">
              <div class="tem-item__info" @click=${() => this.startEditCmd(i)}>
                <span class="tem-item__name">${getLocalizedText(c.displayName, c.name)}</span>
                <span class="tem-item__meta">
                  ${c.isRequired ? html`<span class="tag-pill" style="font-size: 10px;">必填</span>` : nothing}
                  ${(c.parameters || []).length > 0 ? html`<span class="tag-pill" style="font-size: 10px; background: var(--accent-subtle);">${(c.parameters || []).length} 参数</span>` : html`<span class="tag-pill" style="font-size: 10px;">无参数</span>`}
                </span>
              </div>
              <button class="btn btn--ghost btn--sm" style="color: var(--danger); flex-shrink: 0;" @click=${() => this.removeCmd(i)}>删除</button>
            </div>
          `)}
        </div>
      ` : html`<div class="tem-empty">暂无命令，点击上方按钮添加</div>`}
    `;
  }

  renderCmdForm() {
    const showForm = this.editingCmdIndex >= 0;
    if (!showForm && this.editingCmdIndex !== -2) {
      return html`
        <div class="tem-inline-add" @click=${() => { this.resetCmdForm(); this.editingCmdIndex = -2; }}>
          <span>+ 点击添加新命令</span>
        </div>
      `;
    }

    const isEditing = this.editingCmdIndex >= 0;
    return html`
      <div class="tem-inline-form">
        <div class="tem-inline-form__title">${isEditing ? "编辑命令" : "新增命令"}</div>
        <div class="tem-form-grid tem-form-grid--compact">
          <div class="field">
            <label>命令名<span style="color: var(--danger);">*</span></label>
            <input type="text" placeholder="如 read_all" .value=${this.cmdName} @input=${(e: any) => { this.cmdName = e.target.value; }} />
          </div>
          <div class="field">
            <label>显示名</label>
            <input type="text" placeholder="如 读取全部" .value=${this.cmdDisplayName} @input=${(e: any) => { this.cmdDisplayName = e.target.value; }} />
          </div>
          <div class="field">
            <label>描述</label>
            <input type="text" placeholder="可选" .value=${this.cmdDesc} @input=${(e: any) => { this.cmdDesc = e.target.value; }} />
          </div>
        </div>
        <label class="checkbox-label"><input type="checkbox" .checked=${this.cmdIsRequired} @change=${(e: any) => { this.cmdIsRequired = e.target.checked; }} /> 必填</label>

        <!-- Parameters sub-section -->
        <div class="tem-params-section">
          <div class="tem-params__header">
            <span>参数（${this.cmdParams.length}）</span>
            <button class="btn btn--sm btn--ghost" @click=${() => { this.resetParamForm(); this.editingParamIndex = -2; }}>
              + 添加参数
            </button>
          </div>

          ${this.renderParamForm()}

          ${this.cmdParams.length > 0 ? html`
            <div class="tem-item-list tem-item-list--sm">
              ${this.cmdParams.map((p: any, i: number) => html`
                <div class="tem-item tem-item--sm ${this.editingParamIndex === i ? 'tem-item--active' : ''}">
                  <div class="tem-item__info" @click=${() => this.startEditParam(i)}>
                    <span class="tem-item__name">${getLocalizedText(p.displayName, p.name)}</span>
                    <span class="tem-item__meta">
                      <span class="tag-pill" style="font-size: 10px;">${p.dataType}</span>
                      ${p.isRequired ? html`<span class="tag-pill" style="font-size: 10px;">必填</span>` : nothing}
                    </span>
                  </div>
                  <button class="btn btn--ghost btn--sm" style="color: var(--danger);" @click=${() => this.removeParam(i)}>×</button>
                </div>
              `)}
            </div>
          ` : nothing}
        </div>

        <div class="tem-inline-form__actions">
          <button class="btn btn--sm btn--ghost" @click=${this.resetCmdForm}>取消</button>
          <button class="btn btn--sm btn--primary" ?disabled=${!this.cmdName.trim()} @click=${this.saveCmd}>
            ${isEditing ? "更新" : "添加"}
          </button>
        </div>
      </div>
    `;
  }

  renderParamForm() {
    const showForm = this.editingParamIndex >= 0;
    if (!showForm && this.editingParamIndex !== -2) return nothing;

    const isEditing = this.editingParamIndex >= 0;
    return html`
      <div class="tem-inline-form tem-inline-form--param">
        <div class="tem-form-grid tem-form-grid--compact">
          <div class="field">
            <label>参数名<span style="color: var(--danger);">*</span></label>
            <input type="text" placeholder="如 x" .value=${this.paramName} @input=${(e: any) => { this.paramName = e.target.value; }} />
          </div>
          <div class="field">
            <label>显示名</label>
            <input type="text" placeholder="可选" .value=${this.paramDisplayName} @input=${(e: any) => { this.paramDisplayName = e.target.value; }} />
          </div>
          <div class="field">
            <label>数据类型</label>
            <select .value=${this.paramDataType} @change=${(e: any) => { this.paramDataType = e.target.value; }}>
              <option value="string">string</option>
              <option value="number">number</option>
              <option value="boolean">boolean</option>
              <option value="integer">integer</option>
            </select>
          </div>
          <div class="field">
            <label>默认值</label>
            <input type="text" placeholder="可选" .value=${this.paramDefaultValue} @input=${(e: any) => { this.paramDefaultValue = e.target.value; }} />
          </div>
        </div>
        <label class="checkbox-label"><input type="checkbox" .checked=${this.paramRequired} @change=${(e: any) => { this.paramRequired = e.target.checked; }} /> 必填</label>
        <div class="tem-inline-form__actions">
          <button class="btn btn--sm btn--ghost" @click=${() => { this.editingParamIndex = -1; }}>取消</button>
          <button class="btn btn--sm btn--primary" ?disabled=${!this.paramName.trim()} @click=${this.saveParam}>
            ${isEditing ? "更新" : "添加"}
          </button>
        </div>
      </div>
    `;
  }

  renderTemplateDetailModal() {
    const t = this.selectedTemplate!;
    const displayName = getLocalizedText(t.displayName, t.name);
    const description = getLocalizedText(t.description ?? undefined, "");
    const props = parseJsonField(t.properties, []);
    const cmds = parseJsonField(t.commands, []);
    const totalProps = props.length;
    const totalCmds = cmds.length;
    const readonlyProps = props.filter((p: any) => p.isReadOnly === true || p.accessMode === "r" || p.accessMode === "R").length;
    const writableProps = totalProps - readonlyProps;

    return html`
      <div class="modal-overlay template-detail-overlay" role="dialog" aria-modal="true" aria-label="模板详情">
        <div class="template-detail-card" @click=${(e: Event) => e.stopPropagation()}>
          <!-- Fixed Header -->
          <div class="tdc-header">
            <div class="tdc-header__icon">${CATEGORY_ICONS[t.category] || "📦"}</div>
            <div class="tdc-header__info">
              <div class="tdc-header__title">${displayName}</div>
              <div class="tdc-header__meta">
                ${t.manufacturer ? html`<span>${t.manufacturer}</span>` : nothing}
                ${t.manufacturer && (t.deviceType || CATEGORY_LABELS[t.category]) ? html`<span class="tdc-dot">·</span>` : nothing}
                <span>${t.deviceType || CATEGORY_LABELS[t.category] || t.category}</span>
                ${t.version ? html`<span class="tdc-dot">·</span><span>v${t.version}</span>` : nothing}
              </div>
            </div>
            ${t.isBuiltin ? html`<span class="tdc-badge tdc-badge--builtin">内置</span>` : nothing}
          </div>

          <!-- Scrollable Body -->
          <div class="tdc-body">
            <!-- Chips -->
            <div class="tdc-chips">
              ${t.protocolType ? html`<span class="tdc-chip">协议: ${t.protocolType}</span>` : nothing}
              ${t.driverName ? html`<span class="tdc-chip tdc-chip--link" @click=${() => { this.selectedTemplate = null; this.switchTab('drivers'); }}>驱动: ${t.driverName} →</span>` : nothing}
              ${t.category ? html`<span class="tdc-chip">${CATEGORY_LABELS[t.category] || t.category}</span>` : nothing}
              ${t.tags && t.tags.length > 0 ? t.tags.map(tag => html`<span class="tdc-chip tdc-chip--accent">${tag}</span>`) : nothing}
            </div>

            ${description ? html`<div class="tdc-desc">${description}</div>` : nothing}

            <!-- Stats -->
            <div class="tdc-stats">
              <div class="tdc-stat">
                <span class="tdc-stat__num">${totalProps}</span>
                <span class="tdc-stat__label">属性</span>
              </div>
              <div class="tdc-stat">
                <span class="tdc-stat__num">${totalCmds}</span>
                <span class="tdc-stat__label">命令</span>
              </div>
              <div class="tdc-stat tdc-stat--ok">
                <span class="tdc-stat__num">${writableProps}</span>
                <span class="tdc-stat__label">可写</span>
              </div>
              <div class="tdc-stat tdc-stat--muted">
                <span class="tdc-stat__num">${readonlyProps}</span>
                <span class="tdc-stat__label">只读</span>
              </div>
            </div>

            <!-- Tab bar -->
            <div class="tdc-tabs">
              <button
                class="tdc-tab ${this.detailTab === 'props' ? 'active' : ''}"
                @click=${() => { this.detailTab = 'props'; this.requestUpdate(); }}
              >
                属性
                ${totalProps > 0 ? html`<span class="tdc-tab__count">${totalProps}</span>` : nothing}
              </button>
              <button
                class="tdc-tab ${this.detailTab === 'cmds' ? 'active' : ''}"
                @click=${() => { this.detailTab = 'cmds'; this.requestUpdate(); }}
              >
                命令
                ${totalCmds > 0 ? html`<span class="tdc-tab__count">${totalCmds}</span>` : nothing}
              </button>
            </div>

            <!-- Tab content -->
            <div class="tdc-tab-content">
              ${this.detailTab === 'props' ? html`
                ${totalProps > 0 ? html`
                  <div class="tdc-props">
                    ${props.map((p: any) => html`
                      <div class="tdc-prop">
                        <div class="tdc-prop__name">${getLocalizedText(p.displayName, p.name || "unnamed")}</div>
                        <div class="tdc-prop__meta">
                          <span class="tdc-prop__type">${p.dataType || "—"}</span>
                          ${p.unit ? html`<span class="tdc-prop__unit">${p.unit}</span>` : nothing}
                          ${p.defaultValue !== undefined ? html`<span class="tdc-prop__default">=${p.defaultValue}</span>` : nothing}
                        </div>
                        <span class="tdc-prop__badge ${p.isReadOnly === true || p.accessMode === "r" || p.accessMode === "R" ? 'tdc-prop__badge--ro' : 'tdc-prop__badge--rw'}">
                          ${p.isReadOnly === true || p.accessMode === "r" || p.accessMode === "R" ? 'R' : 'RW'}
                        </span>
                      </div>
                    `)}
                  </div>
                ` : html`<div class="tdc-empty-inline">无属性定义</div>`}
              ` : html`
                ${totalCmds > 0 ? html`
                  <div class="tdc-props">
                    ${cmds.map((c: any) => {
                      const params = parseJsonField(c.parameters, []);
                      return html`
                        <div class="tdc-prop">
                          <div class="tdc-prop__name">${getLocalizedText(c.displayName, c.name || "unnamed")}</div>
                          <div class="tdc-prop__meta">
                            ${params.length > 0 ? params.map((param: any) => html`
                              <span class="tdc-prop__type">${param.name}</span>
                              <span class="tdc-prop__unit">${param.dataType}</span>
                            `) : html`<span class="tdc-prop__type" style="opacity: 0.4;">无参数</span>`}
                          </div>
                          <span class="tdc-prop__badge tdc-prop__badge--cmd">→</span>
                        </div>
                      `;})}
                  </div>
                ` : html`<div class="tdc-empty-inline">无命令定义</div>`}
              `}
            </div>
          </div>

          <!-- Fixed Footer -->
          <div class="tdc-footer">
            <button class="btn btn--ghost" @click=${() => this.selectedTemplate = null}>关闭</button>
            <div style="display: flex; gap: 8px;">
              ${!t.isBuiltin ? html`
                <button class="btn btn--primary btn--sm" @click=${() => { this.openEdit(t); this.selectedTemplate = null; }}>编辑模板</button>
                <button
                  class="btn btn--secondary btn--sm"
                  ?disabled=${this.publishing}
                  @click=${() => this.publishToMarketplace(t)}
                >
                  ${this.publishing ? "发布中..." : "发布到市场"}
                </button>
              ` : nothing}
            </div>
          </div>
        </div>
      </div>
    `;
  }

  // ─── Driver modal ──────────────────────────────────────────────

  renderDriverModal() {
    const isEdit = !!this.editingDriver;
    return html`
      <div class="modal-overlay template-detail-overlay" role="dialog" aria-modal="true" aria-label=${isEdit ? "编辑驱动" : "新建驱动"}>
        <div class="template-detail-card" @click=${(e: Event) => e.stopPropagation()}>
          <div class="tdc-header">
            <div class="tdc-header__icon">⚙️</div>
            <div class="tdc-header__info">
              <div class="tdc-header__title">${isEdit ? "编辑驱动" : "新建驱动"}</div>
              ${isEdit ? html`<div class="tdc-header__meta">${this.editingDriver!.name}</div>` : nothing}
            </div>
          </div>

          <div class="tdc-body">
            <div class="tem-form-grid">
              <div class="field">
                <label>驱动名称（标识符）<span style="color: var(--danger);">*</span></label>
                <input type="text" placeholder="如 ModbusDriver" .value=${this.dFormName} @input=${(e: any) => { this.dFormName = e.target.value; }} />
              </div>
              <div class="field">
                <label>版本</label>
                <input type="text" placeholder="如 1.0.0" .value=${this.dFormVersion} @input=${(e: any) => { this.dFormVersion = e.target.value; }} />
              </div>
            </div>
            <div class="field" style="margin-top: 12px;">
              <label>描述</label>
              <input type="text" placeholder="可选描述" .value=${this.dFormDescription} @input=${(e: any) => { this.dFormDescription = e.target.value; }} />
            </div>
          </div>

          <div class="tdc-footer">
            <button class="btn btn--ghost" @click=${this.closeModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.saving || !this.dFormName.trim()} @click=${this.saveDriverForm}>
              ${this.saving ? "保存中..." : "保存"}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  renderTagModal() {
    return html`
      <div class="modal-overlay template-detail-overlay" role="dialog" aria-modal="true" aria-label=${this.editingTag ? "编辑标签" : "新建标签"}>
        <div class="template-detail-card" @click=${(e: Event) => e.stopPropagation()}>
          <div class="tdc-header">
            <div class="tdc-header__info">
              <div class="tdc-header__title">${this.editingTag ? "编辑标签" : "新建标签"}</div>
            </div>
          </div>
          <div class="tdc-body">
            <div class="field">
              <label>名称</label>
              <input type="text" placeholder="标签名称" .value=${this.tagFormName} @input=${(e: any) => { this.tagFormName = e.target.value; }} />
            </div>
            <div class="field">
              <label>类型</label>
              <input type="text" placeholder="如: location, device, custom" .value=${this.tagFormType} @input=${(e: any) => { this.tagFormType = e.target.value; }} />
            </div>
            <div class="field">
              <label>描述</label>
              <input type="text" placeholder="可选描述" .value=${this.tagFormDescription} @input=${(e: any) => { this.tagFormDescription = e.target.value; }} />
            </div>
            <div class="field">
              <label>颜色</label>
              <div style="display: flex; align-items: center; gap: 8px;">
                <input type="color" .value=${this.tagFormColor} @input=${(e: any) => { this.tagFormColor = e.target.value; }} style="width: 40px; height: 32px; padding: 0; border: none; cursor: pointer;" />
                <input type="text" .value=${this.tagFormColor} @input=${(e: any) => { this.tagFormColor = e.target.value; }} style="flex: 1;" />
              </div>
            </div>
          </div>
          <div class="tdc-footer">
            <button class="btn btn--ghost" @click=${this.closeModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.savingTag || !this.tagFormName.trim() || !this.tagFormType.trim()} @click=${this.saveTagForm}>
              ${this.savingTag ? "保存中..." : "保存"}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  renderDriverDetailModal() {
    const d = this.selectedDriver!;
    return html`
      <div class="modal-overlay template-detail-overlay" role="dialog" aria-modal="true" aria-label="驱动详情">
        <div class="template-detail-card" @click=${(e: Event) => e.stopPropagation()}>
          <div class="tdc-header">
            <div class="tdc-header__icon">⚙️</div>
            <div class="tdc-header__info">
              <div class="tdc-header__title">${d.name}</div>
              <div class="tdc-header__meta">
                <span style="font-family: monospace; font-size: 11px;">${d.className}</span>
                ${d.version ? html`<span class="tdc-dot">·</span><span>v${d.version}</span>` : nothing}
              </div>
            </div>
          </div>

          <div class="tdc-body">
            ${d.description ? html`<div class="tdc-desc">${d.description}</div>` : nothing}

            <div class="tdc-stats">
              <div class="tdc-stat">
                <span class="tdc-stat__num">${d.deviceNum}</span>
                <span class="tdc-stat__label">关联设备</span>
              </div>
              <div class="tdc-stat">
                <span class="tdc-stat__num">${d.optionsDescriptors.length}</span>
                <span class="tdc-stat__label">配置参数</span>
              </div>
            </div>
            <div class="tdc-header__meta" style="padding: 0;">
              <span>创建: ${formatDate(d.createdAt)}</span>
              <span class="tdc-dot">·</span>
              <span>更新: ${formatDate(d.updatedAt)}</span>
              ${d.location ? html`<span class="tdc-dot">·</span><span style="font-family: monospace; font-size: 11px;">${d.location}</span>` : nothing}
            </div>

            ${d.optionsDescriptors.length > 0 ? html`
              <div class="tem-section-header">
                <span>配置参数（${d.optionsDescriptors.length}）</span>
              </div>
              <div style="overflow-x: auto;">
                <table class="templates-table">
                  <thead>
                    <tr>
                      <th>参数名</th>
                      <th>显示名</th>
                      <th>类型</th>
                      <th>默认值</th>
                      <th>必填</th>
                    </tr>
                  </thead>
                  <tbody>
                    ${d.optionsDescriptors.map(opt => html`
                      <tr>
                        <td style="font-family: monospace; font-size: 12px;">${opt.name}</td>
                        <td>${opt.label || "-"}</td>
                        <td style="color: var(--muted);">${opt.option_type || "string"}</td>
                        <td style="color: var(--muted); font-family: monospace; font-size: 12px;">${opt.default_value ?? "-"}</td>
                        <td>
                          ${opt.required
                            ? html`<span class="tdc-prop__badge tdc-prop__badge--ro">是</span>`
                            : html`<span style="font-size: 10px; color: var(--muted);">否</span>`
                          }
                        </td>
                      </tr>
                    `)}
                  </tbody>
                </table>
              </div>
            ` : html`<div class="tdc-empty-inline">该驱动暂无配置参数</div>`}
          </div>

          <div class="tdc-footer">
            <button class="btn btn--ghost" @click=${() => this.selectedDriver = null}>关闭</button>
            <div style="display: flex; gap: 8px;">
              <button class="btn btn--primary btn--sm" @click=${() => { this.openDriverEdit(d); this.selectedDriver = null; }}>编辑驱动</button>
            </div>
          </div>
        </div>
      </div>
    `;
  }
}
