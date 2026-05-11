import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { marketplaceApi, type MarketplaceTemplate, type MarketplaceDriver } from "../../api/marketplace.js";
import { templateApi } from "../../api/templates.js";
import { success, error as toastError } from "../components/toast.js";

type Tab = "templates" | "drivers";

function resolveLocalized(value: any): string {
  if (value == null) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (typeof value === "object" && !Array.isArray(value)) {
    const zh = value.zh;
    const en = value.en;
    if (typeof zh === "string" && zh) return zh;
    if (typeof en === "string" && en) return en;
  }
  return "";
}

function safeString(value: any, fallback = "-"): string {
  const s = resolveLocalized(value);
  return s || fallback;
}

function getTemplateKey(t: MarketplaceTemplate): string {
  return t.name;
}

function getDriverKey(d: MarketplaceDriver): string {
  return d.id;
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
  @state() detailItem: MarketplaceTemplate | null = null;
  @state() detailLoading = false;
  @state() modalVisible = false;
  @state() detailTab: "basic" | "properties" | "commands" | "deviceInfo" = "basic";

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadTemplates();
    this.loadLocalTemplates();
  }

  private normalizeTemplate(raw: any): MarketplaceTemplate {
    return {
      ...raw,
      name: resolveLocalized(raw.name),
      description: resolveLocalized(raw.description),
      category: resolveLocalized(raw.category),
      author: resolveLocalized(raw.author),
      deviceType: resolveLocalized(raw.deviceType),
      protocolType: resolveLocalized(raw.protocolType),
      driverName: resolveLocalized(raw.driverName),
    };
  }

  private normalizeDriver(raw: any): MarketplaceDriver {
    return {
      ...raw,
      name: resolveLocalized(raw.name),
      description: resolveLocalized(raw.description),
      protocolType: resolveLocalized(raw.protocolType),
    };
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
        this.templates = result.map((t: any) => this.normalizeTemplate(t));
        this.totalPages = 1;
        this.totalCount = result.length;
      } else {
        const data = result?.data ?? [];
        this.templates = data.map((t: any) => this.normalizeTemplate(t));
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
        this.drivers = result.map((d: any) => this.normalizeDriver(d));
        this.totalPages = 1;
        this.totalCount = result.length;
      } else {
        const data = result?.data ?? [];
        this.drivers = data.map((d: any) => this.normalizeDriver(d));
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

  openDetail(item: MarketplaceTemplate) {
    this.detailItem = item;
    this.detailLoading = false;
    this.detailTab = "basic";
    this.modalVisible = true;
  }

  closeDetail = () => {
    this.modalVisible = false;
    setTimeout(() => {
      this.detailItem = null;
      this.detailLoading = false;
    }, 300);
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
    this.searchKeyword = "";
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
      <style>
        .mp-toolbar {
          display: flex;
          flex-direction: column;
          gap: var(--space-3);
          margin-bottom: var(--space-5);
        }
        .mp-toolbar-row {
          display: flex;
          align-items: center;
          gap: var(--space-3);
          flex-wrap: wrap;
        }
        .mp-search {
          flex: 1;
          min-width: 240px;
          max-width: 400px;
        }
        .mp-search input {
          width: 100%;
          padding: var(--space-2) var(--space-3);
          border: none;
          border-radius: var(--radius-md);
          background: var(--bg-elevated);
          color: var(--text);
          font-size: 14px;
          transition: box-shadow var(--duration-fast) ease;
          box-shadow: var(--shadow-sm);
        }
        .mp-search input:focus {
          outline: none;
          box-shadow: var(--focus-glow);
        }
        .mp-search input::placeholder {
          color: var(--muted-strong);
        }
        .mp-tabs {
          display: inline-flex;
          background: var(--bg-elevated);
          border-radius: var(--radius-md);
          padding: 2px;
          gap: 2px;
        }
        .mp-tab {
          padding: var(--space-2) var(--space-4);
          border: none;
          border-radius: var(--radius-sm);
          background: transparent;
          color: var(--muted);
          font-size: 13px;
          font-weight: 500;
          cursor: pointer;
          transition: all var(--duration-fast) var(--ease-out);
        }
        .mp-tab:hover {
          color: var(--text);
          background: var(--bg-subtle);
        }
        .mp-tab.active {
          background: var(--accent);
          color: var(--text-inverse);
          box-shadow: 0 2px 4px var(--accent-glow);
        }
        .mp-grid {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
          gap: var(--space-4);
        }
        .mp-card {
          position: relative;
          display: flex;
          flex-direction: column;
          box-shadow: var(--glass-shadow);
        }
        .mp-card:hover {
          box-shadow: var(--glass-shadow-hover);
        }
        .mp-card--installing::after {
          content: "";
          position: absolute;
          inset: 0;
          background: var(--overlay-hover);
          border-radius: inherit;
          z-index: 10;
          pointer-events: all;
        }
        .mp-card-header {
          display: flex;
          align-items: flex-start;
          justify-content: space-between;
          gap: var(--space-3);
          margin-bottom: var(--space-2);
        }
        .mp-card-title {
          font-size: 16px;
          font-weight: 600;
          letter-spacing: -0.02em;
          color: var(--text-strong);
          line-height: 1.3;
          word-break: break-word;
        }
        .mp-version {
          flex-shrink: 0;
          font-size: 11px;
          font-weight: 600;
          color: var(--accent);
          padding: 2px 8px;
          border-radius: var(--radius-full);
          background: var(--accent-subtle);
          letter-spacing: 0.02em;
        }
        .mp-meta {
          display: flex;
          align-items: center;
          gap: var(--space-2);
          flex-wrap: wrap;
          margin-bottom: var(--space-3);
        }
        .mp-meta-item {
          font-size: 12px;
          color: var(--muted);
          font-weight: 500;
        }
        .mp-meta-sep {
          color: var(--border-strong);
        }
        .mp-desc {
          color: var(--text);
          font-size: 13px;
          line-height: 1.6;
          margin-bottom: var(--space-4);
          flex: 1;
          display: -webkit-box;
          -webkit-line-clamp: 3;
          -webkit-box-orient: vertical;
          overflow: hidden;
        }
        .mp-actions {
          display: flex;
          justify-content: space-between;
          align-items: center;
          gap: var(--space-2);
          padding-top: var(--space-3);
          margin-top: auto;
        }
        @keyframes mp-spin {
          to { transform: rotate(360deg); }
        }
        .mp-spinner {
          width: 14px;
          height: 14px;
          border: 2px solid currentColor;
          border-top-color: transparent;
          border-radius: 50%;
          animation: mp-spin 0.6s linear infinite;
          display: inline-block;
          vertical-align: middle;
          margin-right: 6px;
        }
        .mp-modal-overlay {
          position: fixed;
          inset: 0;
          z-index: var(--z-modal-backdrop);
          background: var(--overlay-backdrop);
          display: flex;
          align-items: center;
          justify-content: center;
          padding: var(--space-4);
          opacity: 0;
          transition: opacity 0.2s var(--ease-out);
          backdrop-filter: blur(4px);
          -webkit-backdrop-filter: blur(4px);
        }
        .mp-modal-overlay.visible {
          opacity: 1;
        }
        .mp-modal-box {
          background: var(--card);
          border-radius: var(--radius-lg);
          width: 100%;
          max-width: 640px;
          max-height: 85vh;
          display: flex;
          flex-direction: column;
          box-shadow: var(--shadow-xl);
          transform: scale(0.96) translateY(10px);
          transition: transform 0.25s var(--ease-out);
        }
        .mp-modal-overlay.visible .mp-modal-box {
          transform: scale(1) translateY(0);
        }
        .mp-modal-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: var(--space-4) var(--space-5);
        }
        .mp-modal-header h3 {
          font-size: 16px;
          font-weight: 600;
          margin: 0;
          color: var(--text-strong);
        }
        .mp-modal-close {
          background: none;
          border: none;
          color: var(--muted);
          font-size: 22px;
          cursor: pointer;
          width: 32px;
          height: 32px;
          display: flex;
          align-items: center;
          justify-content: center;
          border-radius: var(--radius-md);
          transition: all var(--duration-fast) ease;
        }
        .mp-modal-close:hover {
          background: var(--bg-hover);
          color: var(--text);
        }
        .mp-modal-body {
          flex: 1;
          overflow-y: auto;
          padding: var(--space-5);
        }
        .mp-modal-footer {
          display: flex;
          justify-content: flex-end;
          gap: var(--space-3);
          padding: var(--space-3) var(--space-5);
        }
        .mp-detail-title {
          font-size: 20px;
          font-weight: 600;
          color: var(--text-strong);
          margin-bottom: var(--space-1);
          letter-spacing: -0.02em;
        }
        .mp-detail-desc {
          color: var(--muted);
          font-size: 14px;
          line-height: 1.6;
          margin-bottom: var(--space-4);
        }
        .mp-tags {
          display: flex;
          gap: var(--space-2);
          flex-wrap: wrap;
          margin-bottom: var(--space-4);
        }
        .mp-tag {
          font-size: 11px;
          font-weight: 500;
          color: var(--muted);
          background: var(--bg-muted);
          padding: 3px 10px;
          border-radius: var(--radius-full);
        }
        .mp-meta-grid {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
          gap: var(--space-3);
          margin-bottom: var(--space-5);
        }
        .mp-meta-cell {
          background: var(--bg-muted);
          padding: var(--space-3);
          border-radius: var(--radius-md);
        }
        .mp-meta-label {
          font-size: 11px;
          color: var(--muted);
          font-weight: 500;
          text-transform: uppercase;
          letter-spacing: 0.04em;
          margin-bottom: 4px;
        }
        .mp-meta-value {
          font-size: 13px;
          font-weight: 600;
          color: var(--text-strong);
        }
        .mp-meta-value.empty {
          color: var(--muted);
          font-weight: 400;
        }
        .mp-extra-section {
          margin-top: var(--space-4);
        }
        .mp-extra-title {
          font-size: 13px;
          font-weight: 600;
          color: var(--text-strong);
          margin-bottom: var(--space-3);
        }
        .mp-extra-field {
          margin-bottom: var(--space-3);
        }
        .mp-extra-key {
          font-size: 12px;
          color: var(--muted);
          margin-bottom: var(--space-1);
          text-transform: capitalize;
          font-weight: 500;
        }
        .mp-extra-pre {
          margin: 0;
          padding: var(--space-3);
          background: var(--bg-elevated);
          border-radius: var(--radius-md);
          font-size: 12px;
          overflow-x: auto;
          max-height: 300px;
          overflow-y: auto;
        }
        .mp-extra-pre code {
          font-family: var(--mono);
          color: var(--text);
        }
        .mp-empty {
          text-align: center;
          padding: var(--space-8) var(--space-4);
          color: var(--muted);
          font-size: 14px;
        }
        .mp-publish-section {
          margin-top: var(--space-6);
        }
        .mp-publish-grid {
          display: flex;
          gap: var(--space-2);
          flex-wrap: wrap;
          margin-top: var(--space-3);
        }
        .mp-detail-tabs {
          display: flex;
          gap: var(--space-1);
          margin-bottom: var(--space-4);
        }
        .mp-detail-tab {
          padding: var(--space-2) var(--space-3);
          border: none;
          border-radius: var(--radius-md);
          background: transparent;
          color: var(--muted);
          font-size: 13px;
          font-weight: 500;
          cursor: pointer;
          transition: all var(--duration-fast) var(--ease-out);
        }
        .mp-detail-tab:hover {
          color: var(--text);
          background: var(--bg-subtle);
        }
        .mp-detail-tab.active {
          color: var(--accent);
          background: var(--accent-subtle);
        }
        .mp-data-table {
          width: 100%;
          border-collapse: separate;
          border-spacing: 0 4px;
          font-size: 13px;
        }
        .mp-data-table th {
          text-align: left;
          padding: var(--space-2) var(--space-3);
          color: var(--muted);
          font-weight: 600;
          font-size: 11px;
          text-transform: uppercase;
          letter-spacing: 0.04em;
          white-space: nowrap;
        }
        .mp-data-table td {
          padding: var(--space-3);
          color: var(--text);
          vertical-align: top;
          background: var(--bg-muted);
          border-radius: 0;
        }
        .mp-data-table tbody tr td:first-child {
          border-radius: var(--radius-md) 0 0 var(--radius-md);
        }
        .mp-data-table tbody tr td:last-child {
          border-radius: 0 var(--radius-md) var(--radius-md) 0;
        }
        .mp-data-table tbody tr:hover td {
          background: var(--bg-hover);
        }
        .mp-bool-yes {
          color: var(--ok);
          font-weight: 600;
          font-size: 12px;
        }
        .mp-bool-no {
          color: var(--muted);
          font-size: 12px;
        }
        .mp-type-badge {
          display: inline-block;
          font-size: 11px;
          font-weight: 600;
          color: var(--accent);
          background: var(--accent-subtle);
          padding: 1px 8px;
          border-radius: var(--radius-full);
        }
        .mp-section-title {
          font-size: 13px;
          font-weight: 600;
          color: var(--text-strong);
          margin-bottom: var(--space-3);
          margin-top: var(--space-4);
        }
        .mp-section-title:first-child {
          margin-top: 0;
        }
        .mp-dt-list {
          display: grid;
          gap: var(--space-2);
        }
        .mp-dt-item {
          display: grid;
          grid-template-columns: 120px 1fr;
          gap: var(--space-3);
          padding: var(--space-3);
          background: var(--bg-muted);
          border-radius: var(--radius-md);
        }
        .mp-dt-label {
          font-size: 12px;
          color: var(--muted);
          font-weight: 500;
        }
        .mp-dt-value {
          font-size: 13px;
          color: var(--text);
          word-break: break-word;
        }
      </style>

      <div class="mp-toolbar">
        <div class="mp-toolbar-row">
          <div class="mp-search">
            <input
              type="text"
              placeholder="搜索名称、分类、协议..."
              .value=${this.searchKeyword}
              @input=${(e: InputEvent) => { this.searchKeyword = (e.target as HTMLInputElement).value; }}
            />
          </div>
        </div>
        <div class="mp-toolbar-row">
          <div class="mp-tabs">
            <button
              class="mp-tab ${this.activeTab === "templates" ? "active" : ""}"
              @click=${() => this.switchTab("templates")}
            >
              模板
            </button>
            <button
              class="mp-tab ${this.activeTab === "drivers" ? "active" : ""}"
              @click=${() => this.switchTab("drivers")}
            >
              驱动
            </button>
          </div>
        </div>
      </div>

      ${this.activeTab === "templates"
        ? this.renderTemplatesTab()
        : this.renderDriversTab()}

      ${this.localTemplates.length > 0 ? this.renderPublishSection() : nothing}
      ${this.renderDetailModal()}
    `;
  }

  renderTemplatesTab() {
    if (this.loading) return html`<div class="card">加载中...</div>`;
    const items = this.filteredTemplates;
    if (items.length === 0) {
      return html`<div class="mp-empty">暂无模板</div>`;
    }
    return html`
      <div class="mp-grid">
        ${items.map((t, i) => {
          const key = getTemplateKey(t);
          const isInstalling = this.installingId === key;
          return html`
            <div
              class="card mp-card ${isInstalling ? "mp-card--installing" : ""}"
              style="animation-delay: ${i * 50}ms;"
            >
              <div class="mp-card-header">
                <div class="mp-card-title">${safeString(t.name)}</div>
                <span class="mp-version">${safeString(t.version)}</span>
              </div>
              <div class="mp-meta">
                <span class="mp-meta-item">${safeString(t.category, "其他")}</span>
                <span class="mp-meta-sep">·</span>
                <span class="mp-meta-item">${safeString(t.deviceType, "通用设备")}</span>
              </div>
              <div class="mp-desc">${safeString(t.description, "暂无描述")}</div>
              <div class="mp-actions">
                <button
                  class="btn btn--sm"
                  ?disabled=${isInstalling}
                  @click=${() => this.openDetail(t)}
                >
                  详情
                </button>
                <button
                  class="btn primary btn--sm"
                  ?disabled=${isInstalling}
                  @click=${() => this.installTemplate(key)}
                >
                  ${isInstalling
                    ? html`<span class="mp-spinner"></span>安装中...`
                    : "安装"}
                </button>
              </div>
            </div>
          `;
        })}
      </div>
      ${this.renderPagination()}
    `;
  }

  renderDriversTab() {
    if (this.loading) return html`<div class="card">加载中...</div>`;
    const items = this.filteredDrivers;
    if (items.length === 0) {
      return html`<div class="mp-empty">暂无驱动</div>`;
    }
    return html`
      <div class="mp-grid">
        ${items.map((d, i) => {
          const key = getDriverKey(d);
          const isInstalling = this.installingId === key;
          return html`
            <div
              class="card mp-card ${isInstalling ? "mp-card--installing" : ""}"
              style="animation-delay: ${i * 50}ms;"
            >
              <div class="mp-card-header">
                <div class="mp-card-title">${safeString(d.name)}</div>
                <span class="mp-version">${safeString(d.version)}</span>
              </div>
              <div class="mp-meta">
                <span class="mp-meta-item">${safeString(d.protocolType, "通用协议")}</span>
              </div>
              <div class="mp-desc">${safeString(d.description, "暂无描述")}</div>
              <div class="mp-actions">
                <div></div>
                <button
                  class="btn primary btn--sm"
                  ?disabled=${isInstalling}
                  @click=${() => this.installDriver(key)}
                >
                  ${isInstalling
                    ? html`<span class="mp-spinner"></span>安装中...`
                    : "安装"}
                </button>
              </div>
            </div>
          `;
        })}
      </div>
      ${this.renderPagination()}
    `;
  }

  renderPagination() {
    if (this.totalPages <= 1) return nothing;

    const pages: (number | string)[] = [];
    const total = this.totalPages;
    const current = this.page;

    if (total <= 7) {
      for (let i = 1; i <= total; i++) pages.push(i);
    } else {
      pages.push(1);
      if (current > 3) pages.push("...");
      for (let i = Math.max(2, current - 1); i <= Math.min(total - 1, current + 1); i++) {
        pages.push(i);
      }
      if (current < total - 2) pages.push("...");
      pages.push(total);
    }

    return html`
      <div class="pagination">
        <button
          class="btn btn--sm pagination__btn pagination__btn--arrow"
          ?disabled=${this.page <= 1}
          @click=${() => this.goToPage(1)}
          title="首页"
        >
          «
        </button>
        <button
          class="btn btn--sm pagination__btn pagination__btn--arrow"
          ?disabled=${this.page <= 1}
          @click=${() => this.goToPage(this.page - 1)}
        >
          ‹
        </button>

        <div class="pagination__pages">
          ${pages.map((p) => {
            if (p === "...") {
              return html`<span class="pagination__ellipsis">…</span>`;
            }
            return html`
              <button
                class="btn btn--sm pagination__btn ${p === current ? "pagination__btn--active" : ""}"
                @click=${() => this.goToPage(p as number)}
              >
                ${p}
              </button>
            `;
          })}
        </div>

        <button
          class="btn btn--sm pagination__btn pagination__btn--arrow"
          ?disabled=${this.page >= this.totalPages}
          @click=${() => this.goToPage(this.page + 1)}
        >
          ›
        </button>
        <button
          class="btn btn--sm pagination__btn pagination__btn--arrow"
          ?disabled=${this.page >= this.totalPages}
          @click=${() => this.goToPage(this.totalPages)}
          title="末页"
        >
          »
        </button>

        <span class="pagination__meta">共 ${this.totalCount} 条</span>
      </div>
    `;
  }

  renderPublishSection() {
    return html`
      <div class="card mp-publish-section">
        <div class="card-title">发布本地模板到市场</div>
        <div class="mp-publish-grid">
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
    const show = this.detailItem || this.detailLoading;
    if (!show) return nothing;

    return html`
      <div
        class="mp-modal-overlay ${this.modalVisible ? "visible" : ""}"
        @click=${this.closeDetail}
      >
        <div class="mp-modal-box" @click=${(e: Event) => e.stopPropagation()}>
          <div class="mp-modal-header">
            <h3>模板详情</h3>
            <button class="mp-modal-close" @click=${this.closeDetail}>×</button>
          </div>
          <div class="mp-modal-body">
            ${this.detailLoading
              ? html`<div style="padding: var(--space-8); text-align: center; color: var(--muted);">加载中...</div>`
              : this.renderDetailContent()}
          </div>
          <div class="mp-modal-footer">
            <button class="btn" @click=${this.closeDetail}>关闭</button>
            ${this.detailItem?.name ? html`
              <button
                class="btn primary"
                ?disabled=${this.installingId === this.detailItem.name}
                @click=${() => {
                  this.installTemplate(this.detailItem!.name);
                  this.closeDetail();
                }}
              >
                ${this.installingId === this.detailItem.name
                  ? html`<span class="mp-spinner"></span>安装中...`
                  : "安装"}
              </button>
            ` : nothing}
          </div>
        </div>
      </div>
    `;
  }

  private getAvailableDetailTabs(): { key: "basic" | "properties" | "commands" | "deviceInfo"; label: string }[] {
    const t = this.detailItem;
    if (!t) return [];
    const tabs: { key: "basic" | "properties" | "commands" | "deviceInfo"; label: string }[] = [
      { key: "basic", label: "基本信息" }
    ];
    if (Array.isArray(t.properties) && t.properties.length > 0) {
      tabs.push({ key: "properties", label: "设备属性" });
    }
    if (Array.isArray(t.commands) && t.commands.length > 0) {
      tabs.push({ key: "commands", label: "设备命令" });
    }
    if (t.device_info && Object.values(t.device_info).some(v => v != null && (Array.isArray(v) ? v.length > 0 : true))) {
      tabs.push({ key: "deviceInfo", label: "设备信息" });
    }
    return tabs;
  }

  renderDetailContent() {
    const t = this.detailItem;
    if (!t || typeof t !== "object" || Array.isArray(t)) {
      return html`<div style="color: var(--muted); text-align: center; padding: var(--space-8);">暂无数据</div>`;
    }

    const tabs = this.getAvailableDetailTabs();
    const hasMultipleTabs = tabs.length > 1;

    return html`
      ${hasMultipleTabs ? html`
        <div class="mp-detail-tabs">
          ${tabs.map(tab => html`
            <button
              class="mp-detail-tab ${this.detailTab === tab.key ? "active" : ""}"
              @click=${() => { this.detailTab = tab.key; }}
            >
              ${tab.label}
            </button>
          `)}
        </div>
      ` : nothing}

      ${this.detailTab === "basic" ? this.renderBasicTab(t)
        : this.detailTab === "properties" ? this.renderPropertiesTab(t)
        : this.detailTab === "commands" ? this.renderCommandsTab(t)
        : this.detailTab === "deviceInfo" ? this.renderDeviceInfoTab(t)
        : this.renderBasicTab(t)}
    `;
  }

  renderBasicTab(t: MarketplaceTemplate) {
    const tags = Array.isArray(t.tags) ? t.tags : [];
    const metaItems = [
      { label: "版本", value: safeString(t.version) },
      { label: "分类", value: safeString(t.category, "") },
      { label: "设备类型", value: safeString(t.deviceType, "") },
      { label: "协议", value: safeString(t.protocolType, "") },
      { label: "驱动", value: safeString(t.driverName, "") },
      { label: "制造商", value: safeString(t.manufacturer, "") },
      { label: "作者", value: safeString(t.author, "") },
      { label: "评分", value: typeof t.rating === "number" ? String(t.rating) : "" },
      { label: "下载", value: typeof t.downloadCount === "number" ? String(t.downloadCount) : "" },
    ];

    return html`
      <div class="mp-detail-title">${safeString(t.name)}</div>
      <div class="mp-detail-desc">${safeString(t.description, "暂无描述")}</div>

      ${tags.length > 0 ? html`
        <div class="mp-tags">
          ${tags.map((tag: any) => html`
            <span class="mp-tag">${safeString(tag)}</span>
          `)}
        </div>
      ` : nothing}

      <div class="mp-meta-grid">
        ${metaItems.map((item) => html`
          <div class="mp-meta-cell">
            <div class="mp-meta-label">${item.label}</div>
            <div class="mp-meta-value ${!item.value ? "empty" : ""}">${item.value || "—"}</div>
          </div>
        `)}
      </div>
    `;
  }

  renderPropertiesTab(t: MarketplaceTemplate) {
    const props = t.properties ?? [];
    if (props.length === 0) {
      return html`<div class="mp-empty">暂无设备属性</div>`;
    }
    return html`
      <div class="mp-section-title">设备属性 (${props.length})</div>
      <table class="mp-data-table">
        <thead>
          <tr>
            <th>名称</th>
            <th>数据类型</th>
            <th>单位</th>
            <th>默认值</th>
            <th>范围</th>
            <th>读写</th>
            <th>必填</th>
          </tr>
        </thead>
        <tbody>
          ${props.map(p => html`
            <tr>
              <td>
                <div style="font-weight: 600;">${safeString(p.display_name || p.name)}</div>
                ${p.description ? html`<div style="font-size: 11px; color: var(--muted); margin-top: 2px;">${safeString(p.description)}</div>` : nothing}
              </td>
              <td><span class="mp-type-badge">${safeString(p.data_type)}</span></td>
              <td>${safeString(p.unit, "—")}</td>
              <td>${safeString(p.default_value, "—")}</td>
              <td>
                ${p.min_value != null || p.max_value != null
                  ? html`${p.min_value != null ? String(p.min_value) : "∞"} ~ ${p.max_value != null ? String(p.max_value) : "∞"}`
                  : "—"}
              </td>
              <td>
                <span class="${p.is_read_only ? "mp-bool-no" : "mp-bool-yes"}">
                  ${p.is_read_only ? "只读" : "读写"}
                </span>
              </td>
              <td>
                <span class="${p.is_required ? "mp-bool-yes" : "mp-bool-no"}">
                  ${p.is_required ? "是" : "否"}
                </span>
              </td>
            </tr>
          `)}
        </tbody>
      </table>
    `;
  }

  renderCommandsTab(t: MarketplaceTemplate) {
    const cmds = t.commands ?? [];
    if (cmds.length === 0) {
      return html`<div class="mp-empty">暂无设备命令</div>`;
    }
    return html`
      <div class="mp-section-title">设备命令 (${cmds.length})</div>
      <table class="mp-data-table">
        <thead>
          <tr>
            <th>名称</th>
            <th>参数</th>
            <th>必填</th>
          </tr>
        </thead>
        <tbody>
          ${cmds.map(c => {
            let paramsParsed: any[] = [];
            if (c.parameters) {
              try { paramsParsed = JSON.parse(c.parameters); } catch { paramsParsed = []; }
            }
            return html`
              <tr>
                <td>
                  <div style="font-weight: 600;">${safeString(c.display_name || c.name)}</div>
                  ${c.description ? html`<div style="font-size: 11px; color: var(--muted); margin-top: 2px;">${safeString(c.description)}</div>` : nothing}
                  ${c.name !== safeString(c.display_name || c.name) ? html`<div style="font-size: 11px; color: var(--muted); font-family: var(--mono);">${c.name}</div>` : nothing}
                </td>
                <td>
                  ${paramsParsed.length > 0 ? html`
                    <div class="mp-dt-list">
                      ${paramsParsed.map((param: any) => html`
                        <div class="mp-dt-item" style="border-bottom: none; padding: 2px 0;">
                          <div class="mp-dt-label">${safeString(param.name || param.displayName || "参数")}</div>
                          <div class="mp-dt-value">
                            <span class="mp-type-badge">${safeString(param.dataType || param.data_type || "—")}</span>
                            ${param.required ? html`<span class="mp-bool-yes" style="margin-left: 6px;">必填</span>` : nothing}
                            ${param.description || param.desc ? html`<div style="font-size: 11px; color: var(--muted); margin-top: 2px;">${safeString(param.description || param.desc)}</div>` : nothing}
                          </div>
                        </div>
                      `)}
                    </div>
                  ` : html`<span class="mp-bool-no">无参数</span>`}
                </td>
                <td>
                  <span class="${c.is_required ? "mp-bool-yes" : "mp-bool-no"}">
                    ${c.is_required ? "是" : "否"}
                  </span>
                </td>
              </tr>
            `;
          })}
        </tbody>
      </table>
    `;
  }

  renderDeviceInfoTab(t: MarketplaceTemplate) {
    const info = t.device_info;
    if (!info) {
      return html`<div class="mp-empty">暂无设备信息</div>`;
    }
    return html`
      <div class="mp-section-title">设备信息</div>
      <div class="mp-dt-list">
        ${info.default_name_pattern ? html`
          <div class="mp-dt-item">
            <div class="mp-dt-label">默认命名规则</div>
            <div class="mp-dt-value" style="font-family: var(--mono);">${info.default_name_pattern}</div>
          </div>
        ` : nothing}
        ${info.default_display_name_pattern ? html`
          <div class="mp-dt-item">
            <div class="mp-dt-label">默认显示名</div>
            <div class="mp-dt-value">${safeString(info.default_display_name_pattern)}</div>
          </div>
        ` : nothing}
        ${info.default_description ? html`
          <div class="mp-dt-item">
            <div class="mp-dt-label">默认描述</div>
            <div class="mp-dt-value">${safeString(info.default_description)}</div>
          </div>
        ` : nothing}
        ${info.required_fields && info.required_fields.length > 0 ? html`
          <div class="mp-dt-item">
            <div class="mp-dt-label">必填字段</div>
            <div class="mp-dt-value">
              <div class="mp-tags" style="margin-bottom: 0;">
                ${info.required_fields.map(f => html`<span class="mp-tag">${f}</span>`)}
              </div>
            </div>
          </div>
        ` : nothing}
      </div>
    `;
  }

  static styles = [];
}
