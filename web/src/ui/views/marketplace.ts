import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { marketplaceApi, type MarketplaceTemplate, type MarketplaceDriver } from "../../api/marketplace.js";
import { templateApi } from "../../api/templates.js";
import { success, error as toastError } from "../components/toast.js";

type Tab = "templates" | "drivers";

function safeString(value: any, fallback = "-"): string {
  if (value == null) return fallback;
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return fallback;
}

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

  // pagination
  @state() page = 1;
  @state() pageSize = 12;
  @state() totalPages = 0;
  @state() totalCount = 0;

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadTemplates();
    this.loadLocalTemplates();
  }

  async loadTemplates() {
    this.loading = true;
    try {
      const res = await marketplaceApi.getTemplates({
        page: this.page,
        pageSize: this.pageSize,
      });
      const result = res.result;
      if (Array.isArray(result)) {
        this.templates = result;
        this.totalPages = 1;
        this.totalCount = result.length;
      } else {
        this.templates = result?.data ?? [];
        this.totalPages = result?.pagination?.totalPages ?? 0;
        this.totalCount = result?.pagination?.totalCount ?? 0;
      }
    } catch (e: any) {
      toastError(e.message || "加载市场模板失败");
    } finally {
      this.loading = false;
    }
  }

  async loadDrivers() {
    this.loading = true;
    try {
      const res = await marketplaceApi.getDrivers({
        page: this.page,
        pageSize: this.pageSize,
      });
      const result = res.result;
      if (Array.isArray(result)) {
        this.drivers = result;
        this.totalPages = 1;
        this.totalCount = result.length;
      } else {
        this.drivers = result?.data ?? [];
        this.totalPages = result?.pagination?.totalPages ?? 0;
        this.totalCount = result?.pagination?.totalCount ?? 0;
      }
    } catch (e: any) {
      toastError(e.message || "加载市场驱动失败");
    } finally {
      this.loading = false;
    }
  }

  async loadLocalTemplates() {
    try {
      const res = await templateApi.getTemplates({ pageSize: 100 });
      const data = res.result;
      const templates = Array.isArray(data) ? data : (data?.data ?? []);
      this.localTemplates = templates.map((t: any) => ({ id: t.id, name: t.name }));
    } catch {
      // ignore
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
    this.page = 1;
    if (tab === "templates") this.loadTemplates();
    else this.loadDrivers();
  }

  goToPage(p: number) {
    if (p < 1 || p > this.totalPages) return;
    this.page = p;
    if (this.activeTab === "templates") this.loadTemplates();
    else this.loadDrivers();
  }

  private get filteredTemplates() {
    if (!this.searchKeyword) return this.templates;
    const kw = this.searchKeyword.toLowerCase();
    return this.templates.filter(
      (t) =>
        safeString(t.name, "").toLowerCase().includes(kw) ||
        safeString(t.description, "").toLowerCase().includes(kw) ||
        safeString(t.category, "").toLowerCase().includes(kw)
    );
  }

  private get filteredDrivers() {
    if (!this.searchKeyword) return this.drivers;
    const kw = this.searchKeyword.toLowerCase();
    return this.drivers.filter(
      (d) =>
        safeString(d.name, "").toLowerCase().includes(kw) ||
        safeString(d.description, "").toLowerCase().includes(kw)
    );
  }

  render() {
    return html`
      <div style="display: flex; gap: 10px; margin-bottom: 16px; align-items: center; flex-wrap: wrap;">
        <div class="field" style="flex: 1; max-width: 280px; min-width: 160px;">
          <input
            type="text"
            placeholder="搜索名称、分类、协议..."
            .value=${this.searchKeyword}
            @input=${(e: InputEvent) => { this.searchKeyword = (e.target as HTMLInputElement).value; }}
          />
        </div>
        <div class="detail-tabs">
          <button
            class="btn ${this.activeTab === "templates" ? "active" : ""}"
            @click=${() => this.switchTab("templates")}
          >
            模板
          </button>
          <button
            class="btn ${this.activeTab === "drivers" ? "active" : ""}"
            @click=${() => this.switchTab("drivers")}
          >
            驱动
          </button>
        </div>
      </div>

      ${this.activeTab === "templates"
        ? this.renderTemplatesTab()
        : this.renderDriversTab()}

      ${this.localTemplates.length > 0 ? this.renderPublishSection() : nothing}
    `;
  }

  renderTemplatesTab() {
    if (this.loading) return html`<div class="card">加载中...</div>`;
    const items = this.filteredTemplates;
    if (items.length === 0) return html`<div class="card" style="color: var(--muted);">暂无模板</div>`;
    return html`
      <div style="display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 12px;">
        ${items.map((t) => html`
          <div class="card">
            <div style="display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 8px;">
              <div class="card-title">${safeString(t.name)}</div>
              <span style="font-size: 11px; color: var(--muted); background: var(--bg-muted); padding: 2px 8px; border-radius: var(--radius-sm);">${safeString(t.version)}</span>
            </div>
            <div class="card-sub" style="margin-top: 0; margin-bottom: 8px;">
              ${safeString(t.category, "其他")} · ${safeString(t.deviceType, "-")}
            </div>
            <div style="color: var(--text); font-size: 13px; line-height: 1.5; margin-bottom: 12px; min-height: 40px;">
              ${safeString(t.description, "暂无描述")}
            </div>
            <div style="display: flex; justify-content: flex-end;">
              <button
                class="btn primary btn--sm"
                ?disabled=${this.installingId === t.id}
                @click=${() => this.installTemplate(t.id)}
              >
                ${this.installingId === t.id ? "安装中..." : "安装"}
              </button>
            </div>
          </div>
        `)}
      </div>
      ${this.renderPagination()}
    `;
  }

  renderDriversTab() {
    if (this.loading) return html`<div class="card">加载中...</div>`;
    const items = this.filteredDrivers;
    if (items.length === 0) return html`<div class="card" style="color: var(--muted);">暂无驱动</div>`;
    return html`
      <div style="display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 12px;">
        ${items.map((d) => html`
          <div class="card">
            <div style="display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 8px;">
              <div class="card-title">${safeString(d.name)}</div>
              <span style="font-size: 11px; color: var(--muted); background: var(--bg-muted); padding: 2px 8px; border-radius: var(--radius-sm);">${safeString(d.version)}</span>
            </div>
            <div class="card-sub" style="margin-top: 0; margin-bottom: 8px;">
              ${safeString(d.protocolType, "-")}
            </div>
            <div style="color: var(--text); font-size: 13px; line-height: 1.5; margin-bottom: 12px; min-height: 40px;">
              ${safeString(d.description, "暂无描述")}
            </div>
            <div style="display: flex; justify-content: flex-end;">
              <button
                class="btn primary btn--sm"
                ?disabled=${this.installingId === d.id}
                @click=${() => this.installDriver(d.id)}
              >
                ${this.installingId === d.id ? "安装中..." : "安装"}
              </button>
            </div>
          </div>
        `)}
      </div>
      ${this.renderPagination()}
    `;
  }

  renderPagination() {
    if (this.totalPages <= 1) return nothing;
    return html`
      <div class="pagination" style="margin-top: 16px;">
        <button
          class="btn btn--sm"
          ?disabled=${this.page <= 1}
          @click=${() => this.goToPage(this.page - 1)}
        >
          上一页
        </button>
        <span class="pagination-info">第 ${this.page} / ${this.totalPages} 页，共 ${this.totalCount} 条</span>
        <button
          class="btn btn--sm"
          ?disabled=${this.page >= this.totalPages}
          @click=${() => this.goToPage(this.page + 1)}
        >
          下一页
        </button>
      </div>
    `;
  }

  renderPublishSection() {
    return html`
      <div class="card" style="margin-top: 24px;">
        <div class="card-title">发布本地模板到市场</div>
        <div style="display: flex; gap: 12px; flex-wrap: wrap; margin-top: 12px;">
          ${this.localTemplates.map((t) => html`
            <button
              class="btn btn--sm"
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
