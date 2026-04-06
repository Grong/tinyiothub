import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { driverApi } from "../../api/drivers.js";
import { success, error as toastError } from "../components/toast.js";

interface Driver {
  id: string;
  name: string;
  displayName?: string;
  protocolType?: string;
  version?: string;
  description?: string;
  isEnabled?: boolean;
  createdAt?: string;
}

@customElement("view-drivers")
export class DriversView extends LitElement {
  @state() loading = true;
  @state() error = "";
  @state() drivers: Driver[] = [];
  @state() page = 1;
  @state() pageSize = 20;
  @state() totalPages = 0;
  @state() totalCount = 0;
  @state() searchKeyword = "";

  @state() showModal = false;
  @state() editingDriver: Driver | null = null;
  @state() saving = false;
  @state() formName = "";
  @state() formDisplayName = "";
  @state() formProtocolType = "";
  @state() formVersion = "";
  @state() formDescription = "";

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
      const res = await driverApi.getDrivers({ page: this.page, pageSize: this.pageSize });
      const data = res.result;
      if (Array.isArray(data)) {
        this.drivers = data;
      } else if (data?.data) {
        this.drivers = data.data;
        this.totalPages = data.pagination?.totalPages || 0;
        this.totalCount = data.pagination?.totalCount || 0;
      }
    } catch (err: any) {
      this.error = err.message || "加载驱动列表失败";
    } finally {
      this.loading = false;
    }
  }

  get filteredDrivers(): Driver[] {
    if (!this.searchKeyword) return this.drivers;
    const kw = this.searchKeyword.toLowerCase();
    return this.drivers.filter(d =>
      (d.displayName || d.name).toLowerCase().includes(kw) ||
      d.name.toLowerCase().includes(kw) ||
      (d.protocolType || "").toLowerCase().includes(kw)
    );
  }

  openCreate() {
    this.editingDriver = null;
    this.formName = "";
    this.formDisplayName = "";
    this.formProtocolType = "";
    this.formVersion = "";
    this.formDescription = "";
    this.showModal = true;
  }

  openEdit(d: Driver) {
    this.editingDriver = d;
    this.formName = d.name;
    this.formDisplayName = d.displayName || "";
    this.formProtocolType = d.protocolType || "";
    this.formVersion = d.version || "";
    this.formDescription = d.description || "";
    this.showModal = true;
  }

  closeModal() {
    this.showModal = false;
    this.editingDriver = null;
  }

  async saveForm() {
    if (!this.formName.trim()) return;
    this.saving = true;
    try {
      const payload: any = {
        name: this.formName,
        displayName: this.formDisplayName || undefined,
        protocolType: this.formProtocolType || undefined,
        version: this.formVersion || undefined,
        description: this.formDescription || undefined,
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

  async deleteDriver(d: Driver) {
    if (!confirm(`确定要删除驱动 "${d.displayName || d.name}" 吗？`)) return;
    try {
      await driverApi.deleteDriver(d.id);
      success("驱动已删除");
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
          placeholder="搜索驱动名称、协议..."
          .value=${this.searchKeyword}
          @input=${(e: Event) => { this.searchKeyword = (e.target as HTMLInputElement).value; }}
          style="flex: 1; max-width: 300px;"
        />
        <button class="btn btn--primary" @click=${this.openCreate}>新建驱动</button>
      </div>
      <div class="card" style="overflow: hidden;">
        <table style="width: 100%; border-collapse: collapse;">
          <thead>
            <tr style="border-bottom: 1px solid var(--border);">
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">驱动名称</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">协议类型</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">版本</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">状态</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">描述</th>
              <th style="padding: 12px 16px; text-align: right; font-size: 13px; color: var(--muted); font-weight: 500;">操作</th>
            </tr>
          </thead>
          <tbody>
            ${this.filteredDrivers.length === 0
              ? html`<tr><td colspan="6" style="padding: 40px; text-align: center; color: var(--muted);">暂无驱动</td></tr>`
              : this.filteredDrivers.map(d => html`
                <tr style="border-bottom: 1px solid var(--border);">
                  <td style="padding: 12px 16px;">
                    <div style="font-weight: 500;">${d.displayName || d.name}</div>
                    <div style="font-size: 12px; color: var(--muted);">${d.name}</div>
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px;">${d.protocolType || "-"}</td>
                  <td style="padding: 12px 16px; font-size: 13px;">${d.version || "-"}</td>
                  <td style="padding: 12px 16px;">
                    <span style="display: inline-flex; align-items: center; gap: 6px; font-size: 13px;">
                      <span style="width: 8px; height: 8px; border-radius: 50%; background: ${d.isEnabled !== false ? 'var(--success)' : 'var(--muted)'};"></span>
                      ${d.isEnabled !== false ? "已启用" : "已禁用"}
                    </span>
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px; color: var(--muted); max-width: 300px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${d.description || "-"}</td>
                  <td style="padding: 12px 16px; text-align: right;">
                    <button class="btn btn--ghost btn--sm" style="font-size: 12px;" @click=${() => this.openEdit(d)}>编辑</button>
                    <button class="btn btn--ghost btn--sm" style="font-size: 12px; color: var(--danger);" @click=${() => this.deleteDriver(d)}>删除</button>
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
          <div class="modal-header">${this.editingDriver ? "编辑驱动" : "新建驱动"}</div>
          <div class="modal-body">
            <div class="field">
              <span>驱动名称（标识符）</span>
              <input type="text" placeholder="如 modbus-tcp" .value=${this.formName} @input=${(e: any) => { this.formName = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>显示名称</span>
              <input type="text" placeholder="如 Modbus TCP" .value=${this.formDisplayName} @input=${(e: any) => { this.formDisplayName = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>协议类型</span>
              <input type="text" placeholder="如 modbus, mqtt, onvif" .value=${this.formProtocolType} @input=${(e: any) => { this.formProtocolType = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>版本</span>
              <input type="text" placeholder="如 1.0.0" .value=${this.formVersion} @input=${(e: any) => { this.formVersion = e.target.value; }} />
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
}
