import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { alarmApi } from "../../api/alarms.js";
import type { Alarm, AlarmStatistics, ResolutionType } from "../../types/index.js";
import { success, error as toastError } from "../components/toast.js";

@customElement("view-alarms")
export class AlarmsView extends LitElement {
  @state() loading = true;
  @state() error = "";
  @state() alarms: Alarm[] = [];
  @state() stats?: AlarmStatistics;
  @state() page = 1;
  @state() pageSize = 20;
  @state() totalPages = 0;
  @state() totalCount = 0;
  @state() filterStatus = "active";
  @state() filterLevel = "";

  @state() showAckModal = false;
  @state() ackAlarm: Alarm | null = null;
  @state() ackNote = "";
  @state() ackSaving = false;

  @state() showResolveModal = false;
  @state() resolveAlarm: Alarm | null = null;
  @state() resolveType: ResolutionType = "Fixed";
  @state() resolveNote = "";
  @state() resolveSaving = false;

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
      const [alarmsRes, statsRes] = await Promise.all([
        alarmApi.getAlarms({
          page: this.page,
          pageSize: this.pageSize,
          statuses: this.filterStatus ? [this.filterStatus] : undefined,
          levels: this.filterLevel ? [this.filterLevel] : undefined,
        }),
        alarmApi.getStatistics(),
      ]);
      const alarmData = alarmsRes.result;
      if (alarmData) {
        this.alarms = alarmData.data || [];
        this.totalPages = alarmData.pagination?.totalPages || 0;
        this.totalCount = alarmData.pagination?.totalCount || 0;
      }
      this.stats = statsRes.result || undefined;
    } catch (err: any) {
      this.error = err.message || "加载告警数据失败";
    } finally {
      this.loading = false;
    }
  }

  levelColor(level: string): string {
    switch (level?.toLowerCase()) {
      case "critical": return "var(--danger)";
      case "error": return "var(--danger)";
      case "warning": return "var(--warning)";
      default: return "var(--muted)";
    }
  }

  statusLabel(status: string): string {
    switch (status?.toLowerCase()) {
      case "active": return "活跃";
      case "acknowledged": return "已确认";
      case "resolved": return "已解决";
      case "suppressed": return "已抑制";
      default: return status;
    }
  }

  levelLabel(level: string): string {
    switch (level) {
      case "Critical": return "严重";
      case "Error": return "错误";
      case "Warning": return "警告";
      case "Info": return "信息";
      default: return level;
    }
  }

  openAck(alarm: Alarm) {
    this.ackAlarm = alarm;
    this.ackNote = "";
    this.showAckModal = true;
  }

  closeAckModal() {
    this.showAckModal = false;
    this.ackAlarm = null;
  }

  async confirmAck() {
    if (!this.ackAlarm) return;
    this.ackSaving = true;
    try {
      await alarmApi.acknowledgeAlarm(this.ackAlarm.id, {
        note: this.ackNote || undefined,
      });
      success("告警已确认");
      this.closeAckModal();
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "确认失败");
    } finally {
      this.ackSaving = false;
    }
  }

  openResolve(alarm: Alarm) {
    this.resolveAlarm = alarm;
    this.resolveType = "Fixed";
    this.resolveNote = "";
    this.showResolveModal = true;
  }

  closeResolveModal() {
    this.showResolveModal = false;
    this.resolveAlarm = null;
  }

  async confirmResolve() {
    if (!this.resolveAlarm) return;
    this.resolveSaving = true;
    try {
      await alarmApi.resolveAlarm(this.resolveAlarm.id, {
        resolutionType: this.resolveType,
        note: this.resolveNote || undefined,
      });
      success("告警已解决");
      this.closeResolveModal();
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "解决失败");
    } finally {
      this.resolveSaving = false;
    }
  }

  goToPage(p: number) {
    this.page = p;
    this.loadData();
  }

  render() {
    if (this.loading) {
      return html`
        <div class="page-loading">
          <span class="loading-spinner"></span>
          <span>加载中...</span>
        </div>
      `;
    }

    if (this.error) {
      return html`
        <div class="page-error">
          <div class="page-error__message">${this.error}</div>
          <button class="btn btn--primary" @click=${this.loadData}>重试</button>
        </div>
      `;
    }

    return html`
      ${this.renderStats()}
      <div class="filter-bar">
        <select class="select filter-bar__select" .value=${this.filterStatus} @change=${(e: Event) => { this.filterStatus = (e.target as HTMLSelectElement).value; this.page = 1; this.loadData(); }}>
          <option value="">全部状态</option>
          <option value="active">活跃</option>
          <option value="acknowledged">已确认</option>
          <option value="resolved">已解决</option>
        </select>
        <select class="select filter-bar__select" .value=${this.filterLevel} @change=${(e: Event) => { this.filterLevel = (e.target as HTMLSelectElement).value; this.page = 1; this.loadData(); }}>
          <option value="">全部级别</option>
          <option value="Critical">严重</option>
          <option value="Error">错误</option>
          <option value="Warning">警告</option>
          <option value="Info">信息</option>
        </select>
      </div>
      <div class="card table-container">
        <table class="data-table">
          <thead>
            <tr>
              <th>级别</th>
              <th>设备</th>
              <th>告警信息</th>
              <th>状态</th>
              <th>时间</th>
              <th class="cell-actions">操作</th>
            </tr>
          </thead>
          <tbody>
            ${this.alarms.length === 0
              ? html`<tr><td colspan="6" class="empty-hint">暂无告警</td></tr>`
              : this.alarms.map(a => html`
                <tr>
                  <td>
                    <span class="status-badge">
                      <span class="status-dot" style="background: ${this.levelColor(a.alarmLevel)};"></span>
                      <span class="status-badge__label">${this.levelLabel(a.alarmLevel)}</span>
                    </span>
                  </td>
                  <td class="data-table__cell-sm">${a.deviceName || "-"}</td>
                  <td class="cell-truncate data-table__cell-sm">${a.message}</td>
                  <td>
                    <span class="status-badge">
                      <span class="status-dot" style="background: ${a.status?.toLowerCase() === 'active' ? 'var(--danger)' : a.status?.toLowerCase() === 'acknowledged' ? 'var(--warning)' : 'var(--success)'};"></span>
                      <span class="status-badge__label">${this.statusLabel(a.status)}</span>
                    </span>
                  </td>
                  <td class="cell-muted">${a.alarmTime?.slice(0, 16) || a.createdAt?.slice(0, 16)}</td>
                  <td class="cell-actions">
                    ${a.status?.toLowerCase() === "active" ? html`
                      <button class="btn btn--ghost btn--sm" @click=${() => this.openAck(a)}>确认</button>
                      <button class="btn btn--ghost btn--sm btn--success-text" @click=${() => this.openResolve(a)}>解决</button>
                    ` : nothing}
                    ${a.status?.toLowerCase() === "acknowledged" ? html`
                      <button class="btn btn--ghost btn--sm btn--success-text" @click=${() => this.openResolve(a)}>解决</button>
                    ` : nothing}
                    ${a.status?.toLowerCase() === "resolved" ? html`
                      <span class="inline-muted">-</span>
                    ` : nothing}
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
      ${this.showAckModal ? this.renderAckModal() : nothing}
      ${this.showResolveModal ? this.renderResolveModal() : nothing}
    `;
  }

  renderStats() {
    const s = this.stats;
    if (!s) return nothing;
    return html`
      <div class="stats-grid">
        <div class="card stat-card">
          <div class="stat-card__label">总告警数</div>
          <div class="stat-card__value">${s.totalCount}</div>
        </div>
        <div class="card stat-card">
          <div class="stat-card__label">活跃告警</div>
          <div class="stat-card__value" style="color: var(--danger);">${s.activeCount}</div>
        </div>
        <div class="card stat-card">
          <div class="stat-card__label">已确认</div>
          <div class="stat-card__value" style="color: var(--warning);">${s.acknowledgedCount}</div>
        </div>
        <div class="card stat-card">
          <div class="stat-card__label">已解决</div>
          <div class="stat-card__value" style="color: var(--success);">${s.resolvedCount}</div>
        </div>
      </div>
    `;
  }

  renderAckModal() {
    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label="确认告警" @click=${this.closeAckModal}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">确认告警</div>
          <div class="modal-body modal-fields">
            <div class="form-hint form-hint--block">${this.ackAlarm?.message}</div>
            <div class="field">
              <span>备注（可选）</span>
              <input type="text" placeholder="确认备注" .value=${this.ackNote} @input=${(e: any) => { this.ackNote = e.target.value; }} />
            </div>
          </div>
          <div class="modal-footer">
            <button class="btn btn--ghost" @click=${this.closeAckModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.ackSaving} @click=${this.confirmAck}>
              ${this.ackSaving ? "确认中..." : "确认告警"}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  renderResolveModal() {
    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label="解决告警" @click=${this.closeResolveModal}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">解决告警</div>
          <div class="modal-body modal-fields">
            <div class="form-hint form-hint--block">${this.resolveAlarm?.message}</div>
            <div class="field">
              <span>解决方式</span>
              <select .value=${this.resolveType} @change=${(e: Event) => { this.resolveType = (e.target as HTMLSelectElement).value as ResolutionType; }}>
                <option value="Fixed">已修复</option>
                <option value="FalseAlarm">误报</option>
                <option value="Ignored">忽略</option>
                <option value="AutoResolved">自动恢复</option>
              </select>
            </div>
            <div class="field">
              <span>备注（可选）</span>
              <input type="text" placeholder="解决备注" .value=${this.resolveNote} @input=${(e: any) => { this.resolveNote = e.target.value; }} />
            </div>
          </div>
          <div class="modal-footer">
            <button class="btn btn--ghost" @click=${this.closeResolveModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.resolveSaving} @click=${this.confirmResolve}>
              ${this.resolveSaving ? "解决中..." : "解决告警"}
            </button>
          </div>
        </div>
      </div>
    `;
  }
}
