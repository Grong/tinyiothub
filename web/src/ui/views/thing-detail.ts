import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { SignalWatcher } from "@lit-labs/signals";
import { thingApi, type ThingProfileResponse, type ThingProperty, type ThingEvent, type KnowledgeDoc } from "../../api/things.js";
import { success, error as toastError } from "../components/toast.js";
import { icons } from "../icons.js";
import "./confirm-modal.js";

type TabKey = "overview" | "events" | "actions" | "knowledge";

@customElement("view-thing-detail")
export class ThingDetailView extends SignalWatcher(LitElement) {
  @state() loading = true;
  @state() error = "";
  @state() thingId = "";

  // Profile data (loaded once, shared across tabs)
  @state() profile: ThingProfileResponse | null = null;

  // Active tab
  @state() activeTab: TabKey = "overview";

  // Per-tab states
  @state() eventsLoading = false;
  @state() eventsError = "";
  @state() events: ThingEvent[] = [];
  @state() eventLevelFilter: string = "";

  @state() actionsLoading = false;
  @state() actionsError = "";
  @state() actionExecuting: string | null = null;

  @state() knowledgeLoading = false;
  @state() knowledgeError = "";
  @state() knowledgeDocs: KnowledgeDoc[] = [];

  // Confirm modal
  @state() confirmOpen = false;
  @state() confirmActionName = "";
  @state() confirmActionParams: Record<string, string> = {};
  @state() confirmLoading = false;

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    const path = window.location.pathname;
    const id = path.startsWith("/things/") ? path.split("/")[2] : "";
    if (id) {
      this.thingId = id;
      this.loadProfile();
    }
  }

  // === Data Loading ===

  async loadProfile() {
    this.loading = true;
    this.error = "";
    try {
      const res = await thingApi.getProfile(this.thingId);
      this.profile = res.result;
      if (this.profile) {
        // Pre-populate events and knowledge from profile
        this.events = this.profile.recentEvents || [];
        this.knowledgeDocs = this.profile.knowledgeDocs || [];
      }
    } catch (err: any) {
      this.error = err.message || "加载物详情失败";
    } finally {
      this.loading = false;
    }
  }

  async loadEvents() {
    // Events are already loaded from profile; re-fetch on demand
    if (this.eventsLoading) return;
    this.eventsLoading = true;
    this.eventsError = "";
    try {
      const res = await thingApi.getProfile(this.thingId);
      const p = res.result;
      this.events = p?.recentEvents || [];
    } catch (err: any) {
      this.eventsError = err.message || "加载事件失败";
    } finally {
      this.eventsLoading = false;
    }
  }

  // === Navigation ===

  navigateToThing(id: string) {
    window.history.pushState({}, "", `/things/${id}`);
    this.thingId = id;
    this.activeTab = "overview";
    this.profile = null;
    this.loadProfile();
  }

  backToList() {
    window.history.pushState({}, "", "/things");
    window.dispatchEvent(new PopStateEvent("popstate"));
  }

  switchTab(key: TabKey) {
    this.activeTab = key;
    if (key === "events" && this.events.length === 0 && !this.eventsLoading) {
      this.loadEvents();
    }
  }

  // === Helpers ===

  statusLabel(state?: string): string {
    switch (state) {
      case "online": case "1": return "在线";
      case "offline": case "0": return "离线";
      case "error": case "2": return "故障";
      case "maintenance": return "维护";
      default: return String(state || "未知");
    }
  }

  statusColor(state?: string): string {
    switch (state) {
      case "online": case "1": return "var(--success)";
      case "offline": case "0": return "var(--muted)";
      case "error": case "2": return "var(--danger)";
      case "maintenance": return "var(--warning)";
      default: return "var(--muted)";
    }
  }

  isOnline(state?: string): boolean {
    return state === "online" || state === "1";
  }

  thingTypeLabel(t?: string): string {
    if (!t) return "-";
    const labels: Record<string, string> = {
      device: "设备",
      space: "空间",
      line: "产线",
      building: "建筑",
    };
    return labels[t] || t;
  }

  levelLabel(level: string): string {
    switch (level) {
      case "info": return "信息";
      case "warning": return "警告";
      case "error": return "错误";
      case "critical": return "严重";
      default: return level || "未知";
    }
  }

  levelColor(level: string): string {
    switch (level) {
      case "info": return "var(--accent)";
      case "warning": return "var(--warning)";
      case "error": return "var(--danger)";
      case "critical": return "#dc2626";
      default: return "var(--muted)";
    }
  }

  levelClass(level: string): string {
    switch (level) {
      case "info": return "event-badge--info";
      case "warning": return "event-badge--warning";
      case "error": return "event-badge--error";
      case "critical": return "event-badge--critical";
      default: return "event-badge--info";
    }
  }

  summaryStatusLabel(status?: string): string {
    switch (status) {
      case "ok": return "已生成";
      case "dirty": return "已过期";
      case "failed": return "失败";
      default: return status || "未知";
    }
  }

  formatTime(iso?: string): string {
    if (!iso) return "-";
    return iso.slice(0, 16).replace("T", " ");
  }

  // Number display helper
  displayValue(p: ThingProperty): string {
    const v = p.currentValue ?? p.value;
    if (v === null || v === undefined) return "-";
    if (typeof v === "number") {
      return Number.isInteger(v) ? String(v) : v.toFixed(2);
    }
    return String(v);
  }

  // === Actions ===

  async executeAction(actionName: string) {
    if (this.actionExecuting) return;

    // Check if confirmation is needed
    const wsSettings = this._getWorkspaceSettings();
    if (wsSettings?.requireActionConfirm) {
      this.confirmActionName = actionName;
      this.confirmActionParams = {};
      this.confirmOpen = true;
      return;
    }

    await this._doExecuteAction(actionName);
  }

  private _getWorkspaceSettings(): { requireActionConfirm?: boolean } | null {
    try {
      const raw = localStorage.getItem("workspace-settings");
      return raw ? JSON.parse(raw) : null;
    } catch {
      return null;
    }
  }

  async _doExecuteAction(actionName: string) {
    this.actionExecuting = actionName;
    try {
      // Direct execution via the simple execute endpoint (no token = no confirmation needed)
      // For confirmed actions, use confirmAction endpoint
      const res = await thingApi.confirmAction(this.thingId, actionName, "direct");
      const data = res.result;
      const taskId = (data as any)?.taskId || "";
      const msg = taskId ? `操作 "${actionName}" 执行成功 (task_id: ${taskId})` : `操作 "${actionName}" 执行成功`;
      success(msg);
      return data;
    } catch (err: any) {
      toastError(err.message || "操作执行失败");
      return null;
    } finally {
      this.actionExecuting = null;
    }
  }

  async onConfirmExecute() {
    this.confirmLoading = true;
    try {
      const res = await thingApi.confirmAction(this.thingId, this.confirmActionName, "confirmed");
      const data = res.result;
      const taskId = (data as any)?.taskId || "";
      const msg = taskId ? `操作 "${this.confirmActionName}" 执行成功 (task_id: ${taskId})` : `操作 "${this.confirmActionName}" 执行成功`;
      success(msg);
      this.confirmOpen = false;
    } catch (err: any) {
      toastError(err.message || "操作执行失败");
    } finally {
      this.confirmLoading = false;
    }
  }

  onCancelConfirm() {
    this.confirmOpen = false;
    this.confirmActionName = "";
    this.confirmActionParams = {};
  }

  // === Render ===

  render() {
    if (this.loading) {
      return this.renderPageSkeleton();
    }
    if (this.error && !this.profile) {
      return this.renderPageError();
    }
    if (!this.profile) {
      return this.renderPageError();
    }

    return html`
      <div class="thing-detail">
        ${this.renderHeader()}
        ${this.renderTabBar()}
        <div class="thing-detail__content">
          ${this.activeTab === "overview" ? this.renderOverviewTab() : nothing}
          ${this.activeTab === "events" ? this.renderEventsTab() : nothing}
          ${this.activeTab === "actions" ? this.renderActionsTab() : nothing}
          ${this.activeTab === "knowledge" ? this.renderKnowledgeTab() : nothing}
        </div>
        <confirm-modal
          .open=${this.confirmOpen}
          .actionName=${this.confirmActionName}
          .thingName=${this.profile.name || ""}
          .parameters=${this.confirmActionParams}
          .danger=${false}
          .loading=${this.confirmLoading}
          @confirm=${this.onConfirmExecute}
          @cancel=${this.onCancelConfirm}
        ></confirm-modal>
      </div>
    `;
  }

  // === Header (D4 breadcrumb + name + type + status) ===

  renderHeader() {
    const p = this.profile!;
    const breadcrumb = p.breadcrumb || [];

    return html`
      <div class="card detail-header">
        <div class="detail-header__row">
          <div class="detail-header__main">
            <button class="btn btn--ghost btn--sm detail-header__back" @click=${this.backToList}>
              &larr; 返回
            </button>
            <div class="thing-detail__breadcrumb">
              ${breadcrumb.map((b, i) => html`
                <span class="thing-detail__breadcrumb-item">
                  <a
                    href="/things/${b.id}"
                    class="thing-detail__breadcrumb-link"
                    @click=${(e: Event) => { e.preventDefault(); this.navigateToThing(b.id); }}
                  >${b.name}</a>
                  ${i < breadcrumb.length - 1 ? html`<span class="thing-detail__breadcrumb-sep"> / </span>` : nothing}
                </span>
              `)}
            </div>
            <h2 class="detail-header__title">${p.name}</h2>
            ${p.thingType ? html`
              <span class="type-tag">${this.thingTypeLabel(p.thingType)}</span>
            ` : nothing}
            <span class="status-badge status-badge--subtle">
              <span class="status-dot status-dot--sm" style="background: ${this.statusColor(p.state)};" role="img" aria-label="${this.statusLabel(p.state)}"></span>
              <span class="status-badge__label">${this.statusLabel(p.state)}</span>
            </span>
          </div>
        </div>
      </div>
    `;
  }

  // === Tab Bar ===

  renderTabBar() {
    const tabs: { key: TabKey; icon: unknown; label: string }[] = [
      { key: "overview", icon: icons.barChart, label: "概览" },
      { key: "events", icon: icons.scrollText, label: "事件" },
      { key: "actions", icon: icons.zap, label: "动作" },
      { key: "knowledge", icon: icons.book, label: "知识" },
    ];

    return html`
      <div class="detail-tabs">
        ${tabs.map(t => html`
          <button
            class="detail-tab ${this.activeTab === t.key ? "active" : ""}"
            @click=${() => this.switchTab(t.key)}
            role="tab"
            aria-selected=${this.activeTab === t.key}
          >
            ${t.icon}
            <span>${t.label}</span>
          </button>
        `)}
      </div>
    `;
  }

  // ========================
  // OVERVIEW TAB (D4)
  // ========================

  renderOverviewTab() {
    return html`
      <div class="thing-detail__tab-content">
        ${this.renderAiSummaryCard()}
        ${this.renderPropertyGrid()}
        ${this.renderRecentEvents()}
      </div>
    `;
  }

  renderAiSummaryCard() {
    const p = this.profile!;
    const summary = p.ontologySummary;
    const status = p.summaryStatus;
    const isFailed = status === "failed";
    const isDirty = status === "dirty";
    const updatedAt = p.updatedAt;

    return html`
      <div class="card thing-detail__summary-card">
        <div class="thing-detail__summary-header">
          <span class="thing-detail__summary-title">AI 摘要</span>
          <span class="thing-detail__summary-badge ${isFailed ? "thing-detail__summary-badge--failed" : ""}">
            AI 生成
          </span>
          ${updatedAt ? html`
            <span class="thing-detail__summary-time">${this.formatTime(updatedAt)}</span>
          ` : nothing}
        </div>
        <div class="thing-detail__summary-body">
          ${summary
            ? html`
              <p class="thing-detail__summary-text">${summary}</p>
              ${isDirty ? html`
                <div class="thing-detail__summary-stale">
                  <span class="thing-detail__summary-stale-icon">&#9888;</span>
                  <span>摘要是基于旧数据生成的，新数据到达后将自动重新生成</span>
                  ${isFailed ? html`<span class="thing-detail__summary-failed-badge">上次生成失败</span>` : nothing}
                </div>
              ` : nothing}
              ${isFailed && !isDirty ? html`
                <div class="thing-detail__summary-stale">
                  <span class="thing-detail__summary-stale-icon">&#9888;</span>
                  <span>摘要生成失败，稍后自动重试</span>
                </div>
              ` : nothing}
            ` : html`
              <p class="thing-detail__summary-empty">暂无 AI 摘要</p>
            `}
        </div>
      </div>
    `;
  }

  renderPropertyGrid() {
    const p = this.profile!;
    const properties = p.properties || [];

    if (properties.length === 0) {
      return html`
        <div class="card empty-center">
          <div class="empty-center__icon">&#128203;</div>
          <div class="empty-center__text">暂无属性数据</div>
        </div>
      `;
    }

    return html`
      <div class="card thing-detail__property-grid-wrap">
        <div class="thing-detail__section-title">属性</div>
        <div class="thing-detail__property-grid">
          ${properties.map(prop => html`
            <div class="thing-detail__property-card">
              <div class="thing-detail__property-name">${prop.displayName || prop.name}</div>
              <div class="thing-detail__property-value">
                <span class="thing-detail__property-number">${this.displayValue(prop)}</span>
                ${prop.unit ? html`<span class="prop-unit">${prop.unit}</span>` : nothing}
              </div>
              <div class="thing-detail__property-time">${this.formatTime(prop.updatedAt)}</div>
            </div>
          `)}
        </div>
      </div>
    `;
  }

  renderRecentEvents() {
    const events = this.events.slice(0, 5);

    if (events.length === 0) {
      return html`
        <div class="card empty-center">
          <div class="empty-center__icon">&#128196;</div>
          <div class="empty-center__text">暂无最近事件——配置事件上报</div>
        </div>
      `;
    }

    return html`
      <div class="card thing-detail__events-wrap">
        <div class="thing-detail__section-title">最近事件</div>
        <div class="events-list-wrap">
          ${events.map(ev => html`
            <div class="event-item">
              <span class="event-badge ${this.levelClass(ev.level)}">
                <span class="level-dot" style="background: ${this.levelColor(ev.level)}; display: inline-block; width: 6px; height: 6px; border-radius: 50%; margin-right: 4px;" role="img" aria-label="${this.levelLabel(ev.level)}"></span>
                ${ev.level === "unknown" ? "未知事件" : this.levelLabel(ev.level)}
              </span>
              <div class="event-item__body">
                <div class="event-item__title">${ev.title || "未命名事件"}</div>
                ${ev.message ? html`<div class="event-item__message">${ev.message}</div>` : nothing}
              </div>
              <span class="event-item__time">${this.formatTime(ev.createdAt)}</span>
            </div>
          `)}
        </div>
        ${this.events.length > 5 ? html`
          <div class="thing-detail__see-more">
            <button class="btn btn--ghost btn--sm" @click=${() => this.switchTab("events")}>
              查看全部 ${this.events.length} 条事件 &rarr;
            </button>
          </div>
        ` : nothing}
      </div>
    `;
  }

  // ========================
  // EVENTS TAB
  // ========================

  renderEventsTab() {
    if (this.eventsLoading) {
      return this.renderTabSkeleton("事件");
    }
    if (this.eventsError && this.events.length === 0) {
      return this.renderTabError("事件", this.eventsError, () => this.loadEvents());
    }

    const levels = ["", "info", "warning", "error", "critical"];
    const filtered = this.eventLevelFilter
      ? this.events.filter(e => e.level === this.eventLevelFilter)
      : this.events;

    if (this.events.length === 0) {
      return html`
        <div class="card empty-center">
          <div class="empty-center__icon">&#128196;</div>
          <div class="empty-center__text">暂无事件——配置事件上报</div>
        </div>
      `;
    }

    return html`
      <div class="card thing-detail__events-tab">
        <div class="thing-detail__events-toolbar">
          <div class="thing-detail__section-title">事件列表</div>
          <div class="thing-detail__level-filter">
            ${levels.map(l => html`
              <button
                class="btn btn--ghost btn--xs ${this.eventLevelFilter === l ? "btn--active" : ""}"
                @click=${() => { this.eventLevelFilter = this.eventLevelFilter === l ? "" : l; }}
              >${l ? this.levelLabel(l) : "全部"}</button>
            `)}
          </div>
        </div>
        ${this.eventsError ? html`
          <div class="thing-detail__error-banner">
            <span>&#9888;</span>
            <span>${this.eventsError}</span>
            <button class="btn btn--ghost btn--xs" @click=${this.loadEvents}>重试</button>
          </div>
        ` : nothing}
        ${filtered.length === 0 ? html`
          <div class="empty-center" style="padding: var(--space-4);">
            <div class="empty-center__text">该级别暂无事件</div>
          </div>
        ` : html`
          <div class="events-list-wrap">
            ${filtered.map(ev => html`
              <div class="event-item">
                <span class="event-badge ${this.levelClass(ev.level)}">
                  <span class="level-dot" style="background: ${this.levelColor(ev.level)}; display: inline-block; width: 6px; height: 6px; border-radius: 50%; margin-right: 4px;" role="img" aria-label="${this.levelLabel(ev.level)}"></span>
                  ${ev.level === "unknown" ? "未知事件" : this.levelLabel(ev.level)}
                </span>
                <div class="event-item__body">
                  <div class="event-item__title">${ev.title || "未命名事件"}</div>
                  ${ev.message ? html`<div class="event-item__message">${ev.message}</div>` : nothing}
                </div>
                <span class="event-item__time">${this.formatTime(ev.createdAt)}</span>
              </div>
            `)}
          </div>
        `}
      </div>
    `;
  }

  // ========================
  // ACTIONS TAB
  // ========================

  renderActionsTab() {
    const p = this.profile!;
    const isDevice = p.thingType === "device" || p.deviceType === "device";
    const commands = (p as any).commands || [];

    if (!isDevice && commands.length === 0) {
      return html`
        <div class="card empty-center">
          <div class="empty-center__icon">&#9889;</div>
          <div class="empty-center__text">该物无可用动作</div>
        </div>
      `;
    }

    if (commands.length === 0) {
      return html`
        <div class="card empty-center">
          <div class="empty-center__icon">&#9889;</div>
          <div class="empty-center__text">暂无注册的动作</div>
        </div>
      `;
    }

    return html`
      <div class="card command-list-wrap">
        <div class="thing-detail__section-title">可用动作</div>
        <div class="command-list">
          ${commands.map((c: any) => html`
            <div class="command-item">
              <div>
                <div class="command-item__name">${c.name}</div>
                <div class="command-item__desc">${c.description || "无描述"}</div>
              </div>
              <button
                class="btn btn--primary btn--sm"
                ?disabled=${this.actionExecuting === c.name}
                @click=${() => this.executeAction(c.name)}
              >
                ${this.actionExecuting === c.name ? "执行中..." : "执行"}
              </button>
            </div>
          `)}
        </div>
      </div>
    `;
  }

  // ========================
  // KNOWLEDGE TAB (D6)
  // ========================

  renderKnowledgeTab() {
    if (this.knowledgeLoading) {
      return this.renderTabSkeleton("知识");
    }
    if (this.knowledgeError && this.knowledgeDocs.length === 0) {
      return this.renderTabError("知识", this.knowledgeError, () => this.loadProfile());
    }

    if (this.knowledgeDocs.length === 0) {
      return html`
        <div class="card empty-center">
          <div class="empty-center__icon">&#128214;</div>
          <div class="empty-center__text">还没有知识文档——上传第一篇</div>
        </div>
      `;
    }

    return html`
      <div class="card thing-detail__knowledge-tab">
        <div class="thing-detail__section-title">知识文档</div>
        ${this.knowledgeError ? html`
          <div class="thing-detail__error-banner">
            <span>&#9888;</span>
            <span>${this.knowledgeError}</span>
            <button class="btn btn--ghost btn--xs" @click=${() => this.loadProfile()}>重试</button>
          </div>
        ` : nothing}
        <div class="thing-detail__doc-list">
          ${this.knowledgeDocs.map(doc => html`
            <div class="thing-detail__doc-item">
              <div class="thing-detail__doc-icon">${icons.fileText}</div>
              <div class="thing-detail__doc-body">
                <div class="thing-detail__doc-name">${doc.name}</div>
                <div class="thing-detail__doc-snippet">${(doc.content || "").slice(0, 120)}</div>
              </div>
              <span class="thing-detail__doc-time">${this.formatTime(doc.updatedAt)}</span>
            </div>
          `)}
        </div>
      </div>
    `;
  }

  // === Skeleton / Loading / Error ===

  renderPageSkeleton() {
    return html`
      <div class="thing-detail">
        <div class="card detail-header" style="padding: var(--space-4);">
          <div class="skeleton-row"><div class="skeleton-line skeleton-line--md"></div></div>
        </div>
        <div class="detail-tabs">
          ${["概览", "事件", "动作", "知识"].map(t => html`
            <div class="detail-tab" style="pointer-events: none;">
              <span>${t}</span>
            </div>
          `)}
        </div>
        <div class="card" style="padding: var(--space-4); margin-top: var(--space-3);">
          ${Array.from({ length: 5 }).map(() => html`
            <div class="skeleton-row" style="margin-bottom: var(--space-3);">
              <div class="skeleton-line skeleton-line--lg"></div>
            </div>
          `)}
        </div>
      </div>
    `;
  }

  renderPageError() {
    return html`
      <div class="page-error">
        <div class="page-error__message">${this.error || "无法加载物详情"}</div>
        <button class="btn btn--primary" @click=${this.loadProfile}>重试</button>
      </div>
    `;
  }

  renderTabSkeleton(tabName: string) {
    return html`
      <div class="card" style="padding: var(--space-4);">
        <div class="thing-detail__section-title">${tabName}</div>
        ${Array.from({ length: 3 }).map(() => html`
          <div class="skeleton-row" style="margin-bottom: var(--space-3);">
            <div class="skeleton-line skeleton-line--lg"></div>
          </div>
        `)}
      </div>
    `;
  }

  renderTabError(_tabName: string, errMsg: string, retry: () => void) {
    return html`
      <div class="card" style="padding: var(--space-4); text-align: center;">
        <div style="font-size: 14px; color: var(--danger); margin-bottom: var(--space-3);">${errMsg}</div>
        <button class="btn btn--primary" @click=${retry}>重试</button>
      </div>
    `;
  }
}
