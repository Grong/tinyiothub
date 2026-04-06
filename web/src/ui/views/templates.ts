import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { templateApi } from "../../api/templates.js";
import type { Template, CreateTemplateRequest } from "../../types/index.js";
import { success, error as toastError } from "../components/toast.js";

@customElement("view-templates")
export class TemplatesView extends LitElement {
  @state() loading = true;
  @state() error = "";
  @state() templates: Template[] = [];
  @state() page = 1;
  @state() pageSize = 20;
  @state() totalPages = 0;
  @state() totalCount = 0;
  @state() searchKeyword = "";

  @state() showModal = false;
  @state() editingTemplate: Template | null = null;
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
      if (data) {
        this.templates = data.data || [];
        this.totalPages = data.pagination?.totalPages || 0;
        this.totalCount = data.pagination?.totalCount || 0;
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

  openEdit(t: Template) {
    this.editingTemplate = t;
    this.formName = t.name;
    this.formDisplayName = typeof t.displayName === "string" ? t.displayName : "";
    this.formCategory = t.category || "";
    this.formVersion = t.version || "";
    this.formDescription = typeof t.description === "string" ? t.description : "";
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

  async deleteTemplate(t: Template) {
    if (!confirm(`确定要删除模板 "${t.displayName || t.name}" 吗？`)) return;
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
      <div style="display: flex; gap: 12px; margin-bottom: 16px; align-items: center;">
        <input
          type="text"
          placeholder="搜索模板名称、分类、协议..."
          .value=${this.searchKeyword}
          @keydown=${(e: KeyboardEvent) => { if (e.key === "Enter") { this.page = 1; this.loadData(); } }}
          @input=${(e: Event) => { this.searchKeyword = (e.target as HTMLInputElement).value; }}
          style="flex: 1; max-width: 300px;"
        />
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
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">厂商</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">属性数</th>
              <th style="padding: 12px 16px; text-align: right; font-size: 13px; color: var(--muted); font-weight: 500;">操作</th>
            </tr>
          </thead>
          <tbody>
            ${this.templates.length === 0
              ? html`<tr><td colspan="7" style="padding: 40px; text-align: center; color: var(--muted);">暂无模板</td></tr>`
              : this.templates.map(t => html`
                <tr style="border-bottom: 1px solid var(--border);">
                  <td style="padding: 12px 16px;">
                    <div style="font-weight: 500;">${t.displayName || t.name}</div>
                    <div style="font-size: 12px; color: var(--muted);">${t.name}</div>
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px;">${t.category || "-"}</td>
                  <td style="padding: 12px 16px; font-size: 13px;">${t.version}</td>
                  <td style="padding: 12px 16px; font-size: 13px;">${t.protocolType || "-"}</td>
                  <td style="padding: 12px 16px; font-size: 13px;">${t.manufacturer || "-"}</td>
                  <td style="padding: 12px 16px; font-size: 13px;">${t.properties?.length ?? 0}</td>
                  <td style="padding: 12px 16px; text-align: right;">
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
    `;
  }

  renderModal() {
    return html`
      <div class="modal-overlay" @click=${this.closeModal}>
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
}
