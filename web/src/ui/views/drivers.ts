import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { driverApi } from "../../api/drivers.js";
import { success, error as toastError } from "../components/toast.js";

interface OptionDescriptor {
  label?: string;
  name: string;
  default_value?: string;
  option_type?: string;
  required?: boolean;
  description?: string | null;
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

function parseJsonField<T>(jsonString: any, fallback: T): T {
  if (!jsonString) return fallback;
  if (typeof jsonString !== "string") return jsonString;
  try {
    return JSON.parse(jsonString);
  } catch {
    return fallback;
  }
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

function formatDate(dateStr: string): string {
  if (!dateStr) return "-";
  return dateStr.replace(" ", "T").slice(0, 16);
}

@customElement("view-drivers")
export class DriversView extends LitElement {
  @state() loading = true;
  @state() error = "";
  @state() drivers: ProcessedDriver[] = [];
  @state() page = 1;
  @state() pageSize = 20;
  @state() totalPages = 0;
  @state() totalCount = 0;
  @state() searchKeyword = "";

  @state() showModal = false;
  @state() editingDriver: ProcessedDriver | null = null;
  @state() saving = false;
  @state() formName = "";
  @state() formVersion = "";
  @state() formDescription = "";

  @state() selectedDriver: ProcessedDriver | null = null;

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
    } finally {
      this.loading = false;
    }
  }

  get filteredDrivers(): ProcessedDriver[] {
    if (!this.searchKeyword) return this.drivers;
    const kw = this.searchKeyword.toLowerCase();
    return this.drivers.filter(d =>
      d.name.toLowerCase().includes(kw) ||
      d.className.toLowerCase().includes(kw) ||
      d.description.toLowerCase().includes(kw)
    );
  }

  openCreate() {
    this.editingDriver = null;
    this.formName = "";
    this.formVersion = "";
    this.formDescription = "";
    this.showModal = true;
  }

  openEdit(d: ProcessedDriver) {
    this.editingDriver = d;
    this.formName = d.name;
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
            placeholder="搜索驱动名称、类名..."
            .value=${this.searchKeyword}
            @input=${(e: Event) => { this.searchKeyword = (e.target as HTMLInputElement).value; }}
          />
        </div>
        <button class="btn btn--primary" @click=${this.openCreate}>新建驱动</button>
      </div>
      <div class="card" style="overflow: hidden;">
        <table style="width: 100%; border-collapse: collapse;">
          <thead>
            <tr style="border-bottom: 1px solid var(--border);">
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">驱动名称</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">版本</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">关联设备</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">描述</th>
              <th style="padding: 12px 16px; text-align: right; font-size: 13px; color: var(--muted); font-weight: 500;">操作</th>
            </tr>
          </thead>
          <tbody>
            ${this.filteredDrivers.length === 0
              ? html`<tr><td colspan="5" style="padding: 40px; text-align: center; color: var(--muted);">暂无驱动</td></tr>`
              : this.filteredDrivers.map(d => html`
                <tr style="border-bottom: 1px solid var(--border); cursor: pointer;" @click=${() => this.selectedDriver = d}>
                  <td style="padding: 12px 16px;">
                    <div style="font-weight: 500;">${d.name}</div>
                    <div style="font-size: 12px; color: var(--muted); font-family: monospace;">${d.className}</div>
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px;">${d.version || "-"}</td>
                  <td style="padding: 12px 16px; font-size: 13px;">
                    ${d.deviceNum > 0
                      ? html`<span style="color: var(--success);">${d.deviceNum} 台设备</span>`
                      : html`<span style="color: var(--muted);">未关联</span>`
                    }
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px; color: var(--muted); max-width: 280px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${d.description || "-"}</td>
                  <td style="padding: 12px 16px; text-align: right;" @click=${(e: Event) => e.stopPropagation()}>
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
      ${this.selectedDriver ? this.renderDetailModal() : nothing}
    `;
  }

  renderModal() {
    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label=${this.editingDriver ? "编辑驱动" : "新建驱动"} @click=${this.closeModal}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">${this.editingDriver ? "编辑驱动" : "新建驱动"}</div>
          <div class="modal-body">
            <div class="field">
              <span>驱动名称（标识符）</span>
              <input type="text" placeholder="如 ModbusDriver" .value=${this.formName} @input=${(e: any) => { this.formName = e.target.value; }} />
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

  renderDetailModal() {
    const d = this.selectedDriver!;
    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label="驱动详情" @click=${() => this.selectedDriver = null}>
        <div class="modal" style="max-width: 580px; max-height: 80vh; overflow-y: auto;" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">${d.name}</div>
          <div class="modal-body" style="padding: 16px 20px;">
            <!-- Header -->
            <div style="display: flex; align-items: flex-start; gap: 12px; margin-bottom: 16px;">
              <span style="font-size: 32px;">⚙️</span>
              <div style="flex: 1; min-width: 0;">
                <div style="font-family: monospace; font-size: 11px; color: var(--muted); background: var(--bg); padding: 4px 8px; border-radius: 4px; word-break: break-all;">${d.className}</div>
                <div style="margin-top: 8px; display: flex; flex-wrap: wrap; gap: 8px;">
                  ${d.version ? html`<span style="font-size: 11px; padding: 3px 8px; border-radius: 6px; background: var(--bg); color: var(--muted);">v${d.version}</span>` : nothing}
                  ${d.deviceNum > 0
                    ? html`<span style="font-size: 11px; padding: 3px 8px; border-radius: 6px; background: var(--success); color: white; opacity: 0.85;">${d.deviceNum} 台设备</span>`
                    : html`<span style="font-size: 11px; padding: 3px 8px; border-radius: 6px; background: var(--bg); color: var(--muted);">未关联</span>`
                  }
                </div>
              </div>
            </div>

            ${d.description ? html`
              <div style="font-size: 13px; color: var(--muted); line-height: 1.5; margin-bottom: 16px; padding: 10px 12px; background: var(--bg); border-radius: 8px;">
                ${d.description}
              </div>
            ` : nothing}

            <!-- Meta -->
            <div style="font-size: 12px; color: var(--muted); margin-bottom: 16px;">
              ${d.location ? html`<div style="margin-bottom: 4px;">位置: <span style="font-family: monospace;">${d.location}</span></div>` : nothing}
              <div>创建: ${formatDate(d.createdAt)}</div>
              <div>更新: ${formatDate(d.updatedAt)}</div>
            </div>

            <!-- Config options -->
            ${d.optionsDescriptors.length > 0 ? html`
              <div class="wizard-overview__section-title">配置参数</div>
              <div style="overflow-x: auto;">
                <table style="width: 100%; border-collapse: collapse; font-size: 13px; margin-bottom: 16px;">
                  <thead>
                    <tr style="border-bottom: 1px solid var(--border);">
                      <th style="padding: 6px 10px; text-align: left; color: var(--muted); font-weight: 500; font-size: 12px;">参数名</th>
                      <th style="padding: 6px 10px; text-align: left; color: var(--muted); font-weight: 500; font-size: 12px;">显示名</th>
                      <th style="padding: 6px 10px; text-align: left; color: var(--muted); font-weight: 500; font-size: 12px;">类型</th>
                      <th style="padding: 6px 10px; text-align: left; color: var(--muted); font-weight: 500; font-size: 12px;">默认值</th>
                      <th style="padding: 6px 10px; text-align: left; color: var(--muted); font-weight: 500; font-size: 12px;">必填</th>
                    </tr>
                  </thead>
                  <tbody>
                    ${d.optionsDescriptors.map(opt => html`
                      <tr style="border-bottom: 1px solid var(--border);">
                        <td style="padding: 7px 10px; font-family: monospace; font-size: 12px;">${opt.name}</td>
                        <td style="padding: 7px 10px;">${opt.label || "-"}</td>
                        <td style="padding: 7px 10px; color: var(--muted);">${opt.option_type || "string"}</td>
                        <td style="padding: 7px 10px; color: var(--muted); font-family: monospace; font-size: 12px;">${opt.default_value ?? "-"}</td>
                        <td style="padding: 7px 10px;">
                          ${opt.required
                            ? html`<span style="font-size: 10px; padding: 1px 5px; border-radius: 3px; background: var(--danger); color: white; opacity: 0.8;">是</span>`
                            : html`<span style="font-size: 10px; padding: 1px 5px; border-radius: 3px; background: var(--bg); color: var(--muted);">否</span>`
                          }
                        </td>
                      </tr>
                    `)}
                  </tbody>
                </table>
              </div>
            ` : html`
              <div style="text-align: center; padding: 24px; color: var(--muted); font-size: 13px;">
                该驱动暂无配置参数
              </div>
            `}
          </div>
          <div class="modal-footer">
            <button class="btn btn--ghost" @click=${() => this.selectedDriver = null}>关闭</button>
          </div>
        </div>
      </div>
    `;
  }
}
