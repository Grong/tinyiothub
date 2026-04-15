import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { templateApi } from "../../api/templates.js";
import type { CreateTemplateRequest } from "../../types/index.js";
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

@customElement("view-templates")
export class TemplatesView extends LitElement {
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
  @state() formName = "";
  @state() formDisplayName = "";
  @state() formCategory = "";
  @state() formVersion = "";
  @state() formDescription = "";
  @state() formProtocolType = "";
  @state() formManufacturer = "";
  @state() formDeviceType = "";
  @state() formDriverName = "";

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadData();
  }

  async loadData() {
    this.loading = true;
    this.error = "";
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
    } finally {
      this.loading = false;
    }
  }

  openCreate() {
    this.editingTemplate = null;
    this.formName = "";
    this.formDisplayName = "";
    this.formCategory = "";
    this.formVersion = "";
    this.formDescription = "";
    this.formProtocolType = "";
    this.formManufacturer = "";
    this.formDeviceType = "";
    this.formDriverName = "";
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
    this.showModal = true;
  }

  closeModal() {
    this.showModal = false;
    this.editingTemplate = null;
  }

  async saveForm() {
    if (!this.formName.trim() || !this.formCategory.trim() || !this.formVersion.trim()) return;
    this.saving = true;
    try {
      const payload: CreateTemplateRequest = {
        name: this.formName,
        category: this.formCategory,
        version: this.formVersion,
        displayName: this.formDisplayName ? { "": this.formDisplayName } as any : undefined,
        description: this.formDescription ? { "": this.formDescription } as any : undefined,
        protocolType: this.formProtocolType || undefined,
        manufacturer: this.formManufacturer || undefined,
        deviceType: this.formDeviceType || undefined,
        driverName: this.formDriverName || undefined,
      };
      if (this.editingTemplate) {
        await templateApi.updateTemplate(this.editingTemplate.id, payload as any);
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

  goToPage(p: number) {
    this.page = p;
    this.loadData();
  }

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
          <button class="btn btn--primary" @click=${this.loadData}>重试</button>
        </div>
      `;
    }

    return html`
      <div class="templates-toolbar">
        <div class="field templates-toolbar__search">
          <input
            type="text"
            placeholder="搜索模板名称、分类、协议..."
            .value=${this.searchKeyword}
            @keydown=${(e: KeyboardEvent) => { if (e.key === "Enter") { this.page = 1; this.loadData(); } }}
            @input=${(e: Event) => { this.searchKeyword = (e.target as HTMLInputElement).value; }}
          />
        </div>
        <button class="btn btn--primary" @click=${this.openCreate}>新建模板</button>
      </div>
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
              ? html`<tr><td colspan="7" class="templates-empty">暂无模板，点击「新建模板」创建</td></tr>`
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
      ${this.totalCount > 0 ? html`
        <div class="templates-pagination">
          <button class="btn btn--ghost btn--sm" ?disabled=${this.page <= 1} @click=${() => this.goToPage(this.page - 1)}>上一页</button>
          <span class="page-meta">第 ${this.page} / ${this.totalPages} 页，共 ${this.totalCount} 条</span>
          <button class="btn btn--ghost btn--sm" ?disabled=${this.page >= this.totalPages} @click=${() => this.goToPage(this.page + 1)}>下一页</button>
        </div>
      ` : ""}
      ${this.showModal ? this.renderModal() : nothing}
      ${this.selectedTemplate ? this.renderDetailModal() : nothing}
    `;
  }

  renderModal() {
    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label=${this.editingTemplate ? "编辑模板" : "新建模板"} @click=${this.closeModal}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">${this.editingTemplate ? "编辑模板" : "新建模板"}</div>
          <div class="modal-body">
            <div class="field">
              <span>模板名称（标识符）</span>
              <input type="text" placeholder="如 temperature-sensor" .value=${this.formName} @input=${(e: any) => { this.formName = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>显示名称</span>
              <input type="text" placeholder="如 温度传感器" .value=${this.formDisplayName} @input=${(e: any) => { this.formDisplayName = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>分类</span>
              <input type="text" placeholder="如 sensor, actuator, gateway" .value=${this.formCategory} @input=${(e: any) => { this.formCategory = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>版本</span>
              <input type="text" placeholder="如 1.0.0" .value=${this.formVersion} @input=${(e: any) => { this.formVersion = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>协议类型</span>
              <input type="text" placeholder="如 modbus, mqtt" .value=${this.formProtocolType} @input=${(e: any) => { this.formProtocolType = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>厂商</span>
              <input type="text" placeholder="可选" .value=${this.formManufacturer} @input=${(e: any) => { this.formManufacturer = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>设备类型</span>
              <input type="text" placeholder="可选" .value=${this.formDeviceType} @input=${(e: any) => { this.formDeviceType = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>驱动名称</span>
              <input type="text" placeholder="可选" .value=${this.formDriverName} @input=${(e: any) => { this.formDriverName = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>描述</span>
              <input type="text" placeholder="可选描述" .value=${this.formDescription} @input=${(e: any) => { this.formDescription = e.target.value; }} />
            </div>
          </div>
          <div class="modal-footer">
            <button class="btn btn--ghost" @click=${this.closeModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.saving || !this.formName.trim() || !this.formCategory.trim() || !this.formVersion.trim()} @click=${this.saveForm}>
              ${this.saving ? "保存中..." : "保存"}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  renderDetailModal() {
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
      <div class="modal-overlay template-detail-overlay" role="dialog" aria-modal="true" aria-label="模板详情" @click=${() => this.selectedTemplate = null}>
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
              ${t.driverName ? html`<span class="tdc-chip">驱动: ${t.driverName}</span>` : nothing}
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
            ${!t.isBuiltin ? html`<button class="btn btn--primary btn--sm" @click=${() => { this.openEdit(t); this.selectedTemplate = null; }}>编辑模板</button>` : nothing}
          </div>
        </div>
      </div>
    `;
  }
}
