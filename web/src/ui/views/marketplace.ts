import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { marketplaceApi, type MarketplaceTemplate, type MarketplaceDriver } from "../../api/marketplace.js";
import { templateApi } from "../../api/templates.js";
import { success, error as toastError } from "../components/toast.js";

type Tab = "templates" | "drivers";

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
      const res = await marketplaceApi.getTemplates({ pageSize: 100 });
      this.templates = res.result?.data ?? [];
    } catch (e: any) {
      toastError(e.message || "加载市场模板失败");
    } finally {
      this.loading = false;
    }
  }

  async loadDrivers() {
    this.loading = true;
    try {
      const res = await marketplaceApi.getDrivers({ pageSize: 100 });
      this.drivers = res.result?.data ?? [];
    } catch (e: any) {
      toastError(e.message || "加载市场驱动失败");
    } finally {
      this.loading = false;
    }
  }

  async loadLocalTemplates() {
    try {
      const res = await templateApi.getTemplates({ pageSize: 100 });
      this.localTemplates = (res.result?.data ?? []).map((t: any) => ({ id: t.id, name: t.name }));
    } catch {
      // ignore — publish section just won't show templates
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
    if (tab === "templates") this.loadTemplates();
    else this.loadDrivers();
  }

  private get filteredTemplates() {
    if (!this.searchKeyword) return this.templates;
    const kw = this.searchKeyword.toLowerCase();
    return this.templates.filter(
      (t) =>
        t.name?.toLowerCase().includes(kw) ||
        t.description?.toLowerCase().includes(kw) ||
        t.category?.toLowerCase().includes(kw)
    );
  }

  private get filteredDrivers() {
    if (!this.searchKeyword) return this.drivers;
    const kw = this.searchKeyword.toLowerCase();
    return this.drivers.filter(
      (d) => d.name?.toLowerCase().includes(kw) || d.description?.toLowerCase().includes(kw)
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
            class="detail-tab ${this.activeTab === "templates" ? "active" : ""}"
            @click=${() => this.switchTab("templates")}
          >
            模板
          </button>
          <button
            class="detail-tab ${this.activeTab === "drivers" ? "active" : ""}"
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
    if (items.length === 0) return html`<div class="card empty-hint">暂无模板</div>`;
    return html`
      <div class="card-grid">
        ${items.map((t) => html`
          <div class="card card--hover">
            <div class="card__header">
              <h3 class="card__title">${t.name}</h3>
              <span class="badge">${t.version}</span>
            </div>
            <p class="card__meta">${t.category || "其他"} · ${t.deviceType || "-"}</p>
            <p class="card__desc">${t.description || "暂无描述"}</p>
            <div class="card__footer">
              <button
                class="btn btn--primary btn--sm"
                ?disabled=${this.installingId === t.id}
                @click=${() => this.installTemplate(t.id)}
              >
                ${this.installingId === t.id ? "安装中..." : "安装"}
              </button>
            </div>
          </div>
        `)}
      </div>
    `;
  }

  renderDriversTab() {
    if (this.loading) return html`<div class="card">加载中...</div>`;
    const items = this.filteredDrivers;
    if (items.length === 0) return html`<div class="card empty-hint">暂无驱动</div>`;
    return html`
      <div class="card-grid">
        ${items.map((d) => html`
          <div class="card card--hover">
            <div class="card__header">
              <h3 class="card__title">${d.name}</h3>
              <span class="badge">${d.version}</span>
            </div>
            <p class="card__meta">${d.protocolType || "-"}</p>
            <p class="card__desc">${d.description || "暂无描述"}</p>
            <div class="card__footer">
              <button
                class="btn btn--primary btn--sm"
                ?disabled=${this.installingId === d.id}
                @click=${() => this.installDriver(d.id)}
              >
                ${this.installingId === d.id ? "安装中..." : "安装"}
              </button>
            </div>
          </div>
        `)}
      </div>
    `;
  }

  renderPublishSection() {
    return html`
      <div class="card" style="margin-top: 24px;">
        <h3 class="card__title">发布本地模板到市场</h3>
        <div style="display: flex; gap: 12px; flex-wrap: wrap; margin-top: 12px;">
          ${this.localTemplates.map((t) => html`
            <button
              class="btn btn--secondary btn--sm"
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
