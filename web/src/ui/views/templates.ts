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
      <div style="display: flex; gap: 10px; margin-bottom: 16px; align-items: center; flex-wrap: wrap;">
        <div class="field" style="flex: 1; max-width: 280px; min-width: 160px;">
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
      <div class="card" style="overflow: hidden;">
        <table style="width: 100%; border-collapse: collapse;">
          <thead>
            <tr style="border-bottom: 1px solid var(--border);">
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">模板名称</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">分类</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">版本</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">协议</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">标签</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">属性数</th>
              <th style="padding: 12px 16px; text-align: right; font-size: 13px; color: var(--muted); font-weight: 500;">操作</th>
            </tr>
          </thead>
          <tbody>
            ${this.templates.length === 0
              ? html`<tr><td colspan="8" style="padding: 40px; text-align: center; color: var(--muted);">暂无模板</td></tr>`
              : this.templates.map(t => html`
                <tr style="border-bottom: 1px solid var(--border); cursor: pointer;" @click=${() => this.selectedTemplate = t}>
                  <td style="padding: 12px 16px;">
                    <div style="font-weight: 500;">${getLocalizedText(t.displayName, t.name)}</div>
                    <div style="font-size: 12px; color: var(--muted);">${t.name}</div>
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px;">${t.category || "-"}</td>
                  <td style="padding: 12px 16px; font-size: 13px;">${t.version}</td>
                  <td style="padding: 12px 16px; font-size: 13px;">${t.protocolType || "-"}</td>
                  <td style="padding: 12px 16px;">
                    ${t.tags && t.tags.length > 0
                      ? t.tags.slice(0, 3).map(tag => html`<span style="font-size: 11px; padding: 2px 7px; border-radius: 4px; background: var(--bg); color: var(--text); border: 1px solid var(--border); margin-right: 4px; display: inline-block;">${tag}</span>`)
                      : html`<span style="color: var(--muted);">-</span>`
                    }
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px;">${t.properties?.length ?? 0}</td>
                  <td style="padding: 12px 16px; text-align: right;" @click=${(e: Event) => e.stopPropagation()}>
                    ${!t.isBuiltin ? html`
                      <button class="btn btn--ghost btn--sm" style="font-size: 12px;" @click=${() => this.openEdit(t)}>编辑</button>
                      <button class="btn btn--ghost btn--sm" style="font-size: 12px; color: var(--danger);" @click=${() => this.deleteTemplate(t)}>删除</button>
                    ` : html`<span style="font-size: 12px; color: var(--muted);">内置</span>`}
                  </td>
                </tr>
              `)}
          </tbody>
        </table>
      </div>
      ${this.totalPages > 1 ? html`
        <div class="pagination">
          <button class="btn btn--ghost btn--sm" ?disabled=${this.page <= 1} @click=${() => this.goToPage(this.page - 1)}>上一页</button>
          <span class="pagination-info">第 ${this.page} / ${this.totalPages} 页，共 ${this.totalCount} 条</span>
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
    const totalProps = t.properties.length;
    const totalCmds = t.commands.length;
    const readonlyProps = t.properties.filter((p: any) => p.accessMode === "r" || p.accessMode === "R").length;
    const writableProps = totalProps - readonlyProps;

    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label="模板详情" @click=${() => this.selectedTemplate = null}>
        <div class="modal" style="max-width: 640px; max-height: 80vh; overflow-y: auto;" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">${displayName}</div>
          <div class="modal-body" style="padding: 16px 20px;">
            <!-- Header info -->
            <div style="display: flex; align-items: center; gap: 12px; margin-bottom: 16px;">
              <span style="font-size: 28px;">${CATEGORY_ICONS[t.category] || "📦"}</span>
              <div style="flex: 1; min-width: 0;">
                <div style="font-size: 12px; color: var(--muted);">
                  ${t.manufacturer ? html`${t.manufacturer} · ` : nothing}${t.deviceType || t.category}${t.version ? html` · v${t.version}` : nothing}
                </div>
              </div>
              ${t.isBuiltin ? html`<span style="font-size: 10px; padding: 2px 8px; border-radius: 4px; background: var(--bg); color: var(--muted);">内置</span>` : nothing}
            </div>

            ${description ? html`
              <div style="font-size: 13px; color: var(--muted); line-height: 1.5; margin-bottom: 14px; padding: 10px 12px; background: var(--bg); border-radius: 8px;">
                ${description}
              </div>
            ` : nothing}

            <!-- Meta chips -->
            <div style="display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 16px;">
              ${t.protocolType ? html`<span style="font-size: 11px; padding: 3px 8px; border-radius: 6px; background: var(--bg); color: var(--muted);">协议: ${t.protocolType}</span>` : nothing}
              ${t.driverName ? html`<span style="font-size: 11px; padding: 3px 8px; border-radius: 6px; background: var(--bg); color: var(--muted);">驱动: ${t.driverName}</span>` : nothing}
              ${t.category ? html`<span style="font-size: 11px; padding: 3px 8px; border-radius: 6px; background: var(--bg); color: var(--muted);">${CATEGORY_LABELS[t.category] || t.category}</span>` : nothing}
            </div>

            <!-- Tags -->
            ${t.tags && t.tags.length > 0 ? html`
              <div style="display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 16px;">
                ${t.tags.map(tag => html`<span style="font-size: 11px; padding: 2px 8px; border-radius: 4px; background: var(--accent); color: var(--accent-foreground); opacity: 0.85;">${tag}</span>`)}
              </div>
            ` : nothing}

            <!-- Stats -->
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

            <!-- Properties table -->
            ${totalProps > 0 ? html`
              <div class="wizard-overview__section-title">属性列表</div>
              <div style="overflow-x: auto; margin-bottom: 16px;">
                <table style="width: 100%; border-collapse: collapse; font-size: 13px;">
                  <thead>
                    <tr style="border-bottom: 1px solid var(--border);">
                      <th style="padding: 6px 10px; text-align: left; color: var(--muted); font-weight: 500; font-size: 12px;">名称</th>
                      <th style="padding: 6px 10px; text-align: left; color: var(--muted); font-weight: 500; font-size: 12px;">类型</th>
                      <th style="padding: 6px 10px; text-align: left; color: var(--muted); font-weight: 500; font-size: 12px;">单位</th>
                      <th style="padding: 6px 10px; text-align: left; color: var(--muted); font-weight: 500; font-size: 12px;">默认值</th>
                      <th style="padding: 6px 10px; text-align: left; color: var(--muted); font-weight: 500; font-size: 12px;">访问</th>
                    </tr>
                  </thead>
                  <tbody>
                    ${t.properties.map((p: any) => html`
                      <tr style="border-bottom: 1px solid var(--border);">
                        <td style="padding: 7px 10px;">${p.displayName || p.name || "unnamed"}</td>
                        <td style="padding: 7px 10px; color: var(--muted);">${p.dataType || "-"}</td>
                        <td style="padding: 7px 10px; color: var(--muted);">${p.unit || "-"}</td>
                        <td style="padding: 7px 10px; color: var(--muted);">${p.defaultValue ?? "-"}</td>
                        <td style="padding: 7px 10px;">
                          ${p.accessMode === "r" || p.accessMode === "R"
                            ? html`<span style="font-size: 10px; padding: 1px 5px; border-radius: 3px; background: var(--bg); color: var(--muted);">只读</span>`
                            : html`<span style="font-size: 10px; padding: 1px 5px; border-radius: 3px; background: var(--accent); color: var(--accent-foreground); opacity: 0.8;">读写</span>`
                          }
                        </td>
                      </tr>
                    `)}
                  </tbody>
                </table>
              </div>
            ` : nothing}

            <!-- Commands list -->
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
          </div>
          <div class="modal-footer">
            <button class="btn btn--ghost" @click=${() => this.selectedTemplate = null}>关闭</button>
          </div>
        </div>
      </div>
    `;
  }
}
