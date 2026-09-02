import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { marketplaceApi, sceneApi, type MarketplaceTemplate, type MarketplaceDriver, type ThingTemplateItem, type SceneTemplateDetail, type SceneParameter, type InstantiateResult } from "../../api/marketplace.js";
import { templateApi } from "../../api/templates.js";
import { driverApi } from "../../api/drivers.js";
import { success, error as toastError } from "../components/toast.js";
import { i18n } from "../../i18n/index.js";

type Tab = "templates" | "drivers" | "scenes";

function resolveLocalized(value: any): string {
  if (value == null) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (typeof value === "object" && !Array.isArray(value)) {
    const zh = value.zh;
    const en = value.en;
    const locale = i18n.getLocale();
    if (locale.startsWith("zh")) {
      if (typeof zh === "string" && zh) return zh;
      if (typeof en === "string" && en) return en;
    } else {
      if (typeof en === "string" && en) return en;
      if (typeof zh === "string" && zh) return zh;
    }
    return (Object.values(value).find(v => typeof v === "string" && v) as string) || "";
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
  @state() installedTemplateNames: Set<string> = new Set();
  @state() installedDriverIds: Set<string> = new Set();

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

  // scenes tab
  @state() scenes: ThingTemplateItem[] = [];
  @state() sceneMaxDepth: Record<string, number> = {};

  // scene instantiate dialog
  @state() sceneDialogItem: ThingTemplateItem | null = null;
  @state() sceneDialogVisible = false;
  @state() sceneDetailLoading = false;
  @state() sceneDetail: SceneTemplateDetail | null = null;
  @state() sceneName = "";
  @state() sceneParentId = "";
  @state() sceneParams: Record<string, number> = {};
  @state() preview: InstantiateResult | null = null;
  @state() previewLoading = false;
  @state() submitting = false;
  @state() resultWarnings: string[] | null = null;
  @state() resultRootId: string | null = null;
  private previewTimer: number | undefined;
  private previewSeq = 0;

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadTemplates();
    this.loadLocalTemplates();
  }

  disconnectedCallback() {
    window.clearTimeout(this.previewTimer);
    super.disconnectedCallback();
  }

  private normalizeTemplate(raw: any): MarketplaceTemplate {
    return {
      ...raw,
      displayName: resolveLocalized(raw.displayName),
      description: resolveLocalized(raw.description),
      category: resolveLocalized(raw.category),
      author: resolveLocalized(raw.author),
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

  async loadScenes() {
    this.loading = true;
    try {
      const result = await sceneApi.listThingTemplates(true);
      const items = (result?.data ?? []).map((t: any) => ({
        ...t,
        description: resolveLocalized(t.description) || t.description,
        category: resolveLocalized(t.category) || t.category,
      }));
      this.scenes = items;
      this.totalPages = result?.pagination?.totalPages ?? 1;
      this.totalCount = result?.pagination?.totalCount ?? items.length;
      // 列表项不含 maxDepth，逐个取详情补齐（场景包数量少）
      const entries = await Promise.all(
        items.map(async (t: ThingTemplateItem) => {
          try {
            const detail = await sceneApi.getThingTemplate(t.id);
            return [t.id, detail?.structureSummary?.maxDepth ?? 0] as const;
          } catch {
            return [t.id, 0] as const;
          }
        })
      );
      this.sceneMaxDepth = Object.fromEntries(entries);
    } catch (e: any) {
      toastError(e.message || "加载场景包失败");
    } finally {
      this.loading = false;
    }
  }

  async openSceneDialog(item: ThingTemplateItem) {
    window.clearTimeout(this.previewTimer);
    this.sceneDialogItem = item;
    this.sceneDialogVisible = true;
    this.sceneDetailLoading = true;
    this.sceneDetail = null;
    this.sceneName = "";
    this.sceneParentId = "";
    this.sceneParams = {};
    this.preview = null;
    this.previewLoading = false;
    this.resultWarnings = null;
    this.resultRootId = null;
    try {
      const detail = await sceneApi.getThingTemplate(item.id);
      this.sceneDetail = detail;
      const params: Record<string, number> = {};
      for (const p of detail?.parameters ?? []) params[p.name] = p.default;
      this.sceneParams = params;
    } catch (e: any) {
      toastError(e.message || "加载场景包详情失败");
    } finally {
      this.sceneDetailLoading = false;
    }
  }

  closeSceneDialog = () => {
    window.clearTimeout(this.previewTimer);
    this.previewSeq++; // 作废在途的 dry-run 响应
    this.sceneDialogVisible = false;
    setTimeout(() => {
      this.sceneDialogItem = null;
      this.sceneDetail = null;
      this.sceneDetailLoading = false;
      this.preview = null;
      this.previewLoading = false;
      this.submitting = false;
      this.resultWarnings = null;
      this.resultRootId = null;
    }, 300);
  };

  private schedulePreview() {
    window.clearTimeout(this.previewTimer);
    this.previewTimer = window.setTimeout(() => this.runPreview(), 300);
  }

  private async runPreview() {
    const id = this.sceneDialogItem?.id;
    const sceneName = this.sceneName.trim();
    if (!id || !sceneName) {
      this.preview = null;
      return;
    }
    const seq = ++this.previewSeq;
    this.previewLoading = true;
    try {
      const result = await sceneApi.instantiate(id, {
        sceneName,
        parentId: this.sceneParentId.trim() || undefined,
        parameterValues: this.sceneParams,
        dryRun: true,
      });
      if (seq !== this.previewSeq) return; // 已有更新的请求在途，丢弃旧响应
      this.preview = result;
    } catch {
      if (seq === this.previewSeq) this.preview = null;
    }
    if (seq === this.previewSeq) this.previewLoading = false;
  }

  private onSceneParamInput(p: SceneParameter, e: InputEvent) {
    const raw = (e.target as HTMLInputElement).value;
    const v = Number(raw);
    this.sceneParams = { ...this.sceneParams, [p.name]: Number.isNaN(v) ? NaN : Math.trunc(v) };
    this.schedulePreview();
  }

  private get sceneParamErrors(): Record<string, string> {
    const errors: Record<string, string> = {};
    for (const p of this.sceneDetail?.parameters ?? []) {
      const v = this.sceneParams[p.name];
      if (typeof v !== "number" || Number.isNaN(v)) {
        errors[p.name] = "必填";
        continue;
      }
      if (v < p.min || v > p.max) errors[p.name] = `范围 ${p.min} ~ ${p.max}`;
    }
    return errors;
  }

  async submitScene() {
    const id = this.sceneDialogItem?.id;
    const sceneName = this.sceneName.trim();
    if (!id || !sceneName || this.submitting || this.previewLoading) return;
    if (Object.keys(this.sceneParamErrors).length > 0) return;
    this.submitting = true;
    try {
      const result = await sceneApi.instantiate(id, {
        sceneName,
        parentId: this.sceneParentId.trim() || undefined,
        parameterValues: this.sceneParams,
        dryRun: false,
      });
      if (result.warnings && result.warnings.length > 0) {
        // 先展示警告，由用户确认后再跳转
        this.resultWarnings = result.warnings;
        this.resultRootId = result.rootThingId;
      } else {
        success("场景包创建成功");
        this.closeSceneDialog();
        if (result.rootThingId) this.navigateTo(`things/${result.rootThingId}`);
      }
    } catch (e: any) {
      toastError(e.message || "创建失败");
    } finally {
      this.submitting = false;
    }
  }

  finishSceneResult = () => {
    const rootId = this.resultRootId;
    this.closeSceneDialog();
    if (rootId) this.navigateTo(`things/${rootId}`);
  };

  async loadLocalTemplates() {
    try {
      const res = await templateApi.getTemplates({ pageSize: 100 });
      const data = res.result;
      const templates = Array.isArray(data) ? data : (data?.data ?? []);
      this.localTemplates = templates.map((t: any) => ({ id: t.id, name: t.name }));
      this.installedTemplateNames = new Set(templates.map((t: any) => t.name));
    } catch {
      // ignore
    }
    try {
      const res = await driverApi.getDrivers({ pageSize: 100 });
      const data = res.result;
      const drivers = Array.isArray(data) ? data : (data?.data ?? []);
      this.installedDriverIds = new Set(drivers.map((d: any) => d.id));
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

  navigateTo(route: string) {
    window.history.pushState({}, "", `/${route}`);
    window.dispatchEvent(new PopStateEvent("popstate"));
  };

  async installTemplate(id: string) {
    this.installingId = id;
    try {
      await marketplaceApi.installTemplate(id);
      this.installedTemplateNames = new Set([...this.installedTemplateNames, id]);
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
      this.installedDriverIds = new Set([...this.installedDriverIds, id]);
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
    else if (tab === "scenes") this.loadScenes();
    else this.loadDrivers();
  }

  goToPage(p: number) {
    if (p < 1 || p > this.totalPages) return;
    this.page = p;
    if (this.activeTab === "templates") this.loadTemplates();
    else if (this.activeTab === "scenes") this.loadScenes();
    else this.loadDrivers();
  }

  private get filteredTemplates() {
    if (!this.searchKeyword) return this.templates;
    const kw = this.searchKeyword.toLowerCase();
    return this.templates.filter(
      (t) =>
        safeString(t.displayName, t.name).toLowerCase().includes(kw) ||
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

  private get filteredScenes() {
    if (!this.searchKeyword) return this.scenes;
    const kw = this.searchKeyword.toLowerCase();
    return this.scenes.filter(
      (t) =>
        safeString(t.displayName, t.name).toLowerCase().includes(kw) ||
        safeString(t.description, "").toLowerCase().includes(kw) ||
        safeString(t.category, "").toLowerCase().includes(kw)
    );
  }

  render() {
    return html`

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
            <button
              class="mp-tab ${this.activeTab === "scenes" ? "active" : ""}"
              @click=${() => this.switchTab("scenes")}
            >
              场景包
            </button>
          </div>
        </div>
      </div>

      ${this.activeTab === "templates"
        ? this.renderTemplatesTab()
        : this.activeTab === "scenes"
          ? this.renderScenesTab()
          : this.renderDriversTab()}

      ${this.localTemplates.length > 0 ? this.renderPublishSection() : nothing}
      ${this.renderDetailModal()}
      ${this.renderSceneDialog()}
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
          const isInstalled = this.installedTemplateNames.has(key);
          return html`
            <div
              class="card mp-card ${isInstalling ? "mp-card--installing" : ""} ${isInstalled ? "mp-card--installed" : ""}"
              style="animation-delay: ${i * 50}ms;"
            >
              <div class="mp-card-header">
                <div class="mp-card-title">${safeString(t.displayName, t.name)}</div>
                <span class="mp-version">${safeString(t.version)}</span>
              </div>
              <div class="mp-meta">
                <span class="mp-meta-item">${safeString(t.category, "其他")}</span>
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
                ${isInstalled
                  ? html`<span class="mp-installed-badge">已安装</span>`
                  : html`<button
                      class="btn primary btn--sm"
                      ?disabled=${isInstalling}
                      @click=${() => this.installTemplate(key)}
                    >
                      ${isInstalling
                        ? html`<span class="mp-spinner"></span>安装中...`
                        : "安装"}
                    </button>`}
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
          const isInstalled = this.installedDriverIds.has(key);
          return html`
            <div
              class="card mp-card ${isInstalling ? "mp-card--installing" : ""} ${isInstalled ? "mp-card--installed" : ""}"
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
                ${isInstalled
                  ? html`<span class="mp-installed-badge">已安装</span>`
                  : html`<button
                      class="btn primary btn--sm"
                      ?disabled=${isInstalling}
                      @click=${() => this.installDriver(key)}
                    >
                      ${isInstalling
                        ? html`<span class="mp-spinner"></span>安装中...`
                        : "安装"}
                    </button>`}
              </div>
            </div>
          `;
        })}
      </div>
      ${this.renderPagination()}
    `;
  }

  renderScenesTab() {
    if (this.loading) return html`<div class="card">加载中...</div>`;
    const items = this.filteredScenes;
    if (items.length === 0) {
      return html`<div class="mp-empty">暂无场景包</div>`;
    }
    return html`
      <div class="mp-grid">
        ${items.map((t, i) => html`
          <div class="card mp-card" style="animation-delay: ${i * 50}ms;">
            <div class="mp-card-header">
              <div class="mp-card-title">${safeString(t.displayName, t.name)}</div>
            </div>
            <div class="mp-meta">
              <span class="mp-meta-item">${safeString(t.category, "其他")}</span>
              <span class="mp-meta-item">${t.parameterCount} 参数 · 模板结构 ${this.sceneMaxDepth[t.id] ?? "…"} 层</span>
            </div>
            <div class="mp-desc">${safeString(t.description, "暂无描述")}</div>
            <div class="mp-actions">
              <div></div>
              <button class="btn primary btn--sm" @click=${() => this.openSceneDialog(t)}>使用模板</button>
            </div>
          </div>
        `)}
      </div>
    `;
  }

  renderSceneDialog() {
    const t = this.sceneDialogItem;
    if (!t) return nothing;

    const errors = this.sceneParamErrors;
    const canSubmit =
      !this.sceneDetailLoading &&
      !!this.sceneName.trim() &&
      !this.previewLoading &&
      !this.submitting &&
      Object.keys(errors).length === 0;

    return html`
      <div
        class="mp-modal-overlay ${this.sceneDialogVisible ? "visible" : ""}"
        @click=${this.closeSceneDialog}
      >
        <div class="mp-modal-box" @click=${(e: Event) => e.stopPropagation()}>
          <div class="mp-modal-header">
            <div>
              <h3>使用场景包：${safeString(t.displayName, t.name)}</h3>
              ${this.sceneDetail?.structureSummary
                ? html`<p class="mp-modal-subtitle">${this.sceneDetail.structureSummary.parameterCount} 参数 · 模板结构 ${this.sceneDetail.structureSummary.maxDepth} 层</p>`
                : nothing}
            </div>
            <button class="mp-modal-close" @click=${this.closeSceneDialog}>×</button>
          </div>
          <div class="mp-modal-body">
            ${this.sceneDetailLoading
              ? html`<div style="padding: var(--space-8); text-align: center; color: var(--muted);">加载中...</div>`
              : this.resultWarnings
                ? this.renderSceneWarnings()
                : this.renderSceneForm(errors)}
          </div>
          <div class="mp-modal-footer">
            ${this.resultWarnings
              ? html`
                <button class="btn" @click=${this.closeSceneDialog}>关闭</button>
                ${this.resultRootId
                  ? html`<button class="btn primary" @click=${this.finishSceneResult}>查看新根本体</button>`
                  : nothing}
              `
              : html`
                <button class="btn" @click=${this.closeSceneDialog}>取消</button>
                <button
                  class="btn primary"
                  ?disabled=${!canSubmit}
                  @click=${() => this.submitScene()}
                >
                  ${this.submitting ? html`<span class="mp-spinner"></span>创建中...` : "创建"}
                </button>
              `}
          </div>
        </div>
      </div>
    `;
  }

  private renderSceneForm(errors: Record<string, string>) {
    const params = this.sceneDetail?.parameters ?? [];
    return html`
      <div class="mp-dt-list">
        <div class="mp-dt-item">
          <div class="mp-dt-label">根节点名称 *</div>
          <div class="mp-dt-value">
            <input
              type="text"
              .value=${this.sceneName}
              placeholder="如：3 号车间"
              @input=${(e: InputEvent) => {
                this.sceneName = (e.target as HTMLInputElement).value;
                this.schedulePreview();
              }}
            />
          </div>
        </div>
        <div class="mp-dt-item">
          <div class="mp-dt-label">父本体 ID（可选）</div>
          <div class="mp-dt-value">
            <input
              type="text"
              .value=${this.sceneParentId}
              placeholder="留空则挂在根下"
              @input=${(e: InputEvent) => {
                this.sceneParentId = (e.target as HTMLInputElement).value;
                this.schedulePreview();
              }}
            />
          </div>
        </div>
        ${params.map((p) => html`
          <div class="mp-dt-item">
            <div class="mp-dt-label">${resolveLocalized(p.displayName ?? p.display_name) || p.name}</div>
            <div class="mp-dt-value">
              <input
                type="number"
                min=${p.min}
                max=${p.max}
                step="1"
                .value=${String(this.sceneParams[p.name] ?? p.default)}
                @input=${(e: InputEvent) => this.onSceneParamInput(p, e)}
              />
              <div style="font-size: 11px; color: var(--muted); margin-top: 2px;">
                ${p.name} · 范围 ${p.min} ~ ${p.max}
                ${errors[p.name] ? html` · <span style="color: var(--danger, #dc2626);">${errors[p.name]}</span>` : nothing}
              </div>
            </div>
          </div>
        `)}
      </div>

      ${this.previewLoading
        ? html`<div style="color: var(--muted); margin-top: var(--space-4);">预览生成中...</div>`
        : nothing}
      ${this.preview
        ? html`
          <div style="margin-top: var(--space-4);">
            <div style="font-weight: 600; margin-bottom: var(--space-2);">
              将创建 ${this.preview.nodeCount} 个本体（预览，最终名称以创建结果为准）
            </div>
            <pre style="margin: 0; padding: var(--space-3); background: var(--bg-secondary, rgba(0,0,0,0.04)); border: 1px solid var(--border); border-radius: var(--radius-sm, 6px); font-family: var(--mono); font-size: 12px; white-space: pre-wrap; word-break: break-all; max-height: 240px; overflow: auto;">${this.preview.treePreview}</pre>
          </div>
        `
        : nothing}
    `;
  }

  private renderSceneWarnings() {
    const warnings = this.resultWarnings ?? [];
    return html`
      <div class="mp-section-title">创建成功，但有 ${warnings.length} 条警告</div>
      <ul style="margin: 0; padding-left: var(--space-6); color: var(--muted);">
        ${warnings.map((w) => html`<li style="margin-bottom: var(--space-1);">${w}</li>`)}
      </ul>
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
            <div>
              <h3>${this.detailItem ? safeString(this.detailItem.displayName, this.detailItem.name) : "模板详情"}</h3>
              ${this.detailItem?.description ? html`<p class="mp-modal-subtitle">${safeString(this.detailItem.description)}</p>` : nothing}
            </div>
            <button class="mp-modal-close" @click=${this.closeDetail}>×</button>
          </div>
          <div class="mp-modal-body">
            ${this.detailLoading
              ? html`<div style="padding: var(--space-8); text-align: center; color: var(--muted);">加载中...</div>`
              : this.renderDetailContent()}
          </div>
          <div class="mp-modal-footer">
            <button class="btn" @click=${this.closeDetail}>关闭</button>
            ${this.detailItem?.name
              ? this.installedTemplateNames.has(this.detailItem.name)
                ? html`
                  <span class="mp-installed-badge">已安装</span>
                  <button class="btn primary btn--sm" @click=${() => { this.closeDetail(); this.navigateTo('things'); }}>在本地查看</button>
                `
                : html`<button
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
                  </button>`
              : nothing}
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
      tabs.push({ key: "properties", label: "物属性" });
    }
    if (Array.isArray(t.commands) && t.commands.length > 0) {
      tabs.push({ key: "commands", label: "物命令" });
    }
    if (t.device_info && Object.values(t.device_info).some(v => v != null && (Array.isArray(v) ? v.length > 0 : true))) {
      tabs.push({ key: "deviceInfo", label: "物信息" });
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
      { label: "协议", value: safeString(t.protocolType, "") },
      { label: "驱动", value: safeString(t.driverName, "") },
      { label: "制造商", value: safeString(t.manufacturer, "") },
      { label: "作者", value: safeString(t.author, "") },
      { label: "评分", value: typeof t.rating === "number" ? String(t.rating) : "" },
      { label: "下载", value: typeof t.downloadCount === "number" ? String(t.downloadCount) : "" },
    ];

    return html`
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
      return html`<div class="mp-empty">暂无物属性</div>`;
    }
    return html`
      <div class="mp-section-title">物属性 (${props.length})</div>
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
      return html`<div class="mp-empty">暂无物命令</div>`;
    }
    return html`
      <div class="mp-section-title">物命令 (${cmds.length})</div>
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
      return html`<div class="mp-empty">暂无物信息</div>`;
    }
    return html`
      <div class="mp-section-title">物信息</div>
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
