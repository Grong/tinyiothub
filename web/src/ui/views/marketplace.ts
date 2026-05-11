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

function getItemId(item: any): string | undefined {
  if (!item || typeof item !== "object") return undefined;
  return item.id ?? item._id ?? item.slug ?? item.templateId ?? item.template_id;
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

  // detail modal
  @state() detailTemplate: any | null = null;
  @state() detailLoading = false;

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

  async openDetail(id: string) {
    this.detailLoading = true;
    this.detailTemplate = null;
    try {
      const res = await marketplaceApi.getTemplate(id);
      const data = res.result;
      if (data && typeof data === "object") {
        this.detailTemplate = data;
      } else {
        toastError("模板详情格式错误");
      }
    } catch (e: any) {
      toastError(e.message || "获取模板详情失败");
    } finally {
      this.detailLoading = false;
    }
  }

  closeDetail = () => {
    this.detailTemplate = null;
    this.detailLoading = false;
  };

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
      ${this.detailTemplate || this.detailLoading ? this.renderDetailModal() : nothing}
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
            <div style="display: flex; justify-content: flex-end; gap: 8px;">
              ${(() => {
                const id = getItemId(t);
                return id ? html`
                  <button
                    class="btn btn--sm"
                    @click=${() => this.openDetail(id)}
                  >
                    详情
                  </button>
                  <button
                    class="btn primary btn--sm"
                    ?disabled=${this.installingId === id}
                    @click=${() => this.installTemplate(id)}
                  >
                    ${this.installingId === id ? "安装中..." : "安装"}
                  </button>
                ` : html`<span style="font-size: 11px; color: var(--warn);">ID 缺失</span>
                `;
              })()}
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

  renderDetailModal() {
    return html`
      <div class="modal-overlay" @click=${this.closeDetail}>
        <div class="modal-box modal--wide" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <h3>模板详情</h3>
            <button class="modal-close" @click=${this.closeDetail}>×</button>
          </div>
          <div class="modal-body">
            ${this.detailLoading
              ? html`<div>加载中...</div>`
              : this.renderDetailContent()}
          </div>
          <div class="modal-footer">
            <button class="btn" @click=${this.closeDetail}>关闭</button>
            ${this.detailTemplate?.id ? html`
              <button
                class="btn primary"
                ?disabled=${this.installingId === this.detailTemplate.id}
                @click=${() => {
                  this.installTemplate(this.detailTemplate.id);
                  this.closeDetail();
                }}
              >
                ${this.installingId === this.detailTemplate.id ? "安装中..." : "安装"}
              </button>
            ` : nothing}
          </div>
        </div>
      </div>
    `;
  }

  renderDetailContent() {
    const t = this.detailTemplate;
    if (!t || typeof t !== "object" || Array.isArray(t)) {
      return html`<div style="color: var(--muted);">暂无数据</div>`;
    }

    const tags = Array.isArray(t.tags) ? t.tags : [];
    const metaItems = [
      { label: "版本", value: safeString(t.version) },
      { label: "分类", value: safeString(t.category, "-") },
      { label: "设备类型", value: safeString(t.deviceType, "-") },
      { label: "协议", value: safeString(t.protocolType, "-") },
      { label: "作者", value: safeString(t.author, "-") },
      { label: "评分", value: typeof t.rating === "number" ? String(t.rating) : "-" },
      { label: "下载", value: typeof t.downloadCount === "number" ? String(t.downloadCount) : "-" },
    ];

    const knownScalarFields = new Set([
      "id", "name", "version", "description", "category", "author",
      "tags", "deviceType", "protocolType", "driverName", "rating", "downloadCount",
    ]);
    const extraFields = Object.entries(t).filter(
      ([k, v]) => !knownScalarFields.has(k) && v != null
    );

    const extraFieldsHtml = (() => {
      if (extraFields.length === 0) return nothing;
      try {
        return html`
          <div style="margin-top: 12px;">
            <div style="font-size: 13px; font-weight: 600; margin-bottom: 8px;">扩展数据</div>
            ${extraFields.map(([k, v]) => {
              let json = "无法序列化";
              try {
                json = JSON.stringify(v, null, 2);
              } catch {
                json = String(v);
              }
              return html`
                <div style="margin-bottom: 10px;">
                  <div style="font-size: 12px; color: var(--muted); margin-bottom: 4px; text-transform: capitalize;">${k}</div>
                  <pre style="margin: 0; padding: 10px; background: var(--bg-elevated); border-radius: var(--radius-md); font-size: 12px; overflow-x: auto; border: 1px solid var(--border);"
                  ><code>${json}</code></pre>
                </div>
              `;
            })}
          </div>
        `;
      } catch {
        return nothing;
      }
    })();

    return html`
      <div style="margin-bottom: 12px;">
        <div style="font-size: 18px; font-weight: 600; margin-bottom: 4px;">${safeString(t.name)}</div>
        <div style="color: var(--muted); font-size: 13px;">${safeString(t.description, "暂无描述")}</div>
      </div>

      ${tags.length > 0 ? html`
        <div style="display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 12px;">
          ${tags.map((tag: any) => html`
            <span style="font-size: 11px; color: var(--muted); background: var(--bg-muted); padding: 2px 8px; border-radius: var(--radius-sm);">${safeString(tag)}</span>
          `)}
        </div>
      ` : nothing}

      <div style="display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 8px; margin-bottom: 16px;">
        ${metaItems.map((item) => html`
          <div style="background: var(--bg-muted); padding: 8px 12px; border-radius: var(--radius-md);">
            <div style="font-size: 11px; color: var(--muted); margin-bottom: 2px;">${item.label}</div>
            <div style="font-size: 13px; font-weight: 500;">${item.value}</div>
          </div>
        `)}
      </div>

      ${extraFieldsHtml}
    `;
  }

  static styles = [];
}
