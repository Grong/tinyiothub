import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { alarmApi } from "../../api/alarms.js";
import type { Alarm, AlarmStatistics, ResolutionType } from "../../types/index.js";
import { success, error as toastError } from "../components/toast.js";

const STATUS_TABS = [
  { key: "", label: "全部" },
  { key: "active", label: "活跃" },
  { key: "acknowledged", label: "已确认" },
  { key: "resolved", label: "已解决" },
] as const;

const LEVEL_CHIPS = [
  { key: "", label: "全部级别", color: "var(--muted)" },
  { key: "Critical", label: "严重", color: "#ef4444" },
  { key: "Error", label: "错误", color: "#f97316" },
  { key: "Warning", label: "警告", color: "#eab308" },
  { key: "Info", label: "信息", color: "#3b82f6" },
] as const;

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
  @state() filterStatus = "";
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

  createRenderRoot() { return this; }

  connectedCallback() { super.connectedCallback(); this.loadData(); }

  async loadData() {
    this.loading = true;
    this.error = "";
    try {
      const [alarmsRes, statsRes] = await Promise.all([
        alarmApi.getAlarms({
          page: this.page, pageSize: this.pageSize,
          statuses: this.filterStatus ? [this.filterStatus] : undefined,
          levels: this.filterLevel ? [this.filterLevel] : undefined,
        }),
        alarmApi.getStatistics(),
      ]);
      const d = alarmsRes.result as any;
      if (d) {
        this.alarms = d.data || [];
        this.totalPages = d.pagination?.totalPages || 0;
        this.totalCount = d.pagination?.totalCount || 0;
      }
      this.stats = statsRes.result || undefined;
    } catch (err: any) {
      this.error = err.message || "加载告警数据失败";
    } finally {
      this.loading = false;
    }
  }

  setStatus(s: string) { this.filterStatus = s; this.page = 1; this.loadData(); }
  setLevel(l: string) { this.filterLevel = l; this.page = 1; this.loadData(); }
  goToPage(p: number) { this.page = p; this.loadData(); }

  statusLabel(s: string): string {
    const m: Record<string, string> = { active: "活跃", acknowledged: "已确认", resolved: "已解决", suppressed: "已抑制" };
    return m[s?.toLowerCase()] || s || "—";
  }
  levelLabel(l: string): string {
    const m: Record<string, string> = { Critical: "严重", Error: "错误", Warning: "警告", Info: "信息", critical: "严重", error: "错误", warning: "警告", info: "信息" };
    return m[l] || l || "—";
  }
  levelColor(l: string): string {
    const m: Record<string, string> = { critical: "#ef4444", error: "#f97316", warning: "#eab308", info: "#3b82f6" };
    return m[l?.toLowerCase()] || "var(--muted)";
  }
  statusColor(s: string): string {
    const m: Record<string, string> = { active: "#ef4444", acknowledged: "#eab308", resolved: "#22c55e", suppressed: "#6b7280" };
    return m[s?.toLowerCase()] || "var(--muted)";
  }

  openAck(a: Alarm) { this.ackAlarm = a; this.ackNote = ""; this.showAckModal = true; }
  closeAckModal() { this.showAckModal = false; this.ackAlarm = null; }
  async confirmAck() {
    if (!this.ackAlarm) return;
    this.ackSaving = true;
    try {
      await alarmApi.acknowledgeAlarm(this.ackAlarm.id, { note: this.ackNote || undefined });
      success("告警已确认"); this.closeAckModal(); await this.loadData();
    } catch (err: any) { toastError(err.message || "确认失败"); }
    finally { this.ackSaving = false; }
  }

  openResolve(a: Alarm) { this.resolveAlarm = a; this.resolveType = "Fixed"; this.resolveNote = ""; this.showResolveModal = true; }
  closeResolveModal() { this.showResolveModal = false; this.resolveAlarm = null; }
  async confirmResolve() {
    if (!this.resolveAlarm) return;
    this.resolveSaving = true;
    try {
      await alarmApi.resolveAlarm(this.resolveAlarm.id, { resolutionType: this.resolveType, note: this.resolveNote || undefined });
      success("告警已解决"); this.closeResolveModal(); await this.loadData();
    } catch (err: any) { toastError(err.message || "解决失败"); }
    finally { this.resolveSaving = false; }
  }

  render() {
    return html`
      <div class="alarm-page">
        ${this.renderToolbar()}
        ${this.loading ? this.renderSkeleton()
          : this.error ? this.renderError()
          : this.alarms.length === 0 ? this.renderEmpty()
          : html`${this.renderTable()}${this.renderPagination()}`}
      </div>
      ${this.showAckModal ? this.renderAckModal() : nothing}
      ${this.showResolveModal ? this.renderResolveModal() : nothing}
    `;
  }

  renderToolbar() {
    const s = this.stats;
    return html`
      <div class="alarm-toolbar">
        <div class="alarm-tabs">
          ${STATUS_TABS.map(t => {
            const count = !t.key ? (s?.totalCount ?? 0)
              : t.key === "active" ? (s?.activeCount ?? 0)
              : t.key === "acknowledged" ? (s?.acknowledgedCount ?? 0)
              : (s?.resolvedCount ?? 0);
            return html`
              <button class="alarm-tab ${this.filterStatus === t.key ? 'alarm-tab--active' : ''}"
                @click=${() => this.setStatus(t.key)}>
                <span class="alarm-tab__label">${t.label}</span>
                <span class="alarm-tab__count">${count}</span>
              </button>
            `;
          })}
        </div>
        <div class="alarm-level-chips">
          ${LEVEL_CHIPS.map(l => html`
            <button class="level-chip2 ${this.filterLevel === l.key ? 'level-chip2--active' : ''}"
              style="--lc-color: ${l.color}"
              @click=${() => this.setLevel(l.key)}>${l.label}</button>
          `)}
        </div>
      </div>
    `;
  }

  renderTable() {
    return html`
      <div class="alarm-table-wrap">
        <div class="alarm-table-scroll">
          ${this.alarms.map((a, i) => html`
            <div class="alarm-row ${a.status?.toLowerCase() === 'resolved' ? 'alarm-row--resolved' : ''}"
              style="--row-color: ${this.levelColor(a.alarmLevel)}; animation: alarmRowIn 0.35s var(--ease-out) both; animation-delay: ${Math.min(i * 40, 200)}ms">
              <div class="alarm-row__bar" style="background: ${this.levelColor(a.alarmLevel)}"></div>
              <div class="alarm-row__body">
                <div class="alarm-row__top">
                  <span class="alarm-row__level" style="color: ${this.levelColor(a.alarmLevel)}">${this.levelLabel(a.alarmLevel)}</span>
                  <span class="alarm-row__device">${a.deviceName || a.deviceId?.slice(0, 12) || "—"}</span>
                  <span class="alarm-row__status" style="color: ${this.statusColor(a.status)}; background: ${this.statusColor(a.status)}18">
                    ${this.statusLabel(a.status)}
                  </span>
                  <span class="alarm-row__time">${(a.alarmTime || a.createdAt || "").slice(0, 16)}</span>
                </div>
                <div class="alarm-row__msg">${a.message}</div>
                ${a.status?.toLowerCase() !== "resolved" ? html`
                  <div class="alarm-row__actions">
                    ${a.status?.toLowerCase() === "active" ? html`
                      <button class="alarm-action-btn" @click=${() => this.openAck(a)}>确认</button>
                    ` : nothing}
                    <button class="alarm-action-btn alarm-action-btn--resolve" @click=${() => this.openResolve(a)}>解决</button>
                  </div>
                ` : nothing}
              </div>
            </div>
          `)}
        </div>
      </div>
    `;
  }

  renderPagination() {
    if (this.totalPages <= 1) return nothing;
    return html`
      <div class="alarm-pagination">
        <button class="btn btn--ghost btn--sm" ?disabled=${this.page <= 1} @click=${() => this.goToPage(this.page - 1)}>上一页</button>
        <span class="alarm-pagination__info">${this.page} / ${this.totalPages}</span>
        <button class="btn btn--ghost btn--sm" ?disabled=${this.page >= this.totalPages} @click=${() => this.goToPage(this.page + 1)}>下一页</button>
      </div>
    `;
  }

  renderSkeleton() {
    return html`<div class="alarm-skeleton">
      ${[1,2,3,4,5].map((_, i) => html`
        <div class="alarm-skeleton__row" style="animation-delay: ${i * 60}ms">
          <div class="alarm-skeleton__bar"></div>
          <div class="alarm-skeleton__body"><div class="skeleton-line w-30"></div><div class="skeleton-line w-60"></div></div>
        </div>
      `)}
    </div>`;
  }

  renderError() {
    return html`
      <div class="alarm-empty">
        <div class="alarm-empty__icon">!</div>
        <div class="alarm-empty__text">${this.error}</div>
        <button class="btn btn--primary btn--sm" @click=${this.loadData}>重试</button>
      </div>
    `;
  }

  renderEmpty() {
    const label = STATUS_TABS.find(t => t.key === this.filterStatus)?.label || "告警";
    return html`
      <div class="alarm-empty">
        <div class="alarm-empty__icon">✓</div>
        <div class="alarm-empty__text">暂无${label}告警</div>
        <div class="alarm-empty__sub">一切正常，继续保持</div>
      </div>
    `;
  }

  renderAckModal() {
    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label="确认告警" @click=${this.closeAckModal}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">确认告警</div>
          <div class="modal-body modal-fields">
            <div class="alarm-modal-msg">${this.ackAlarm?.message}</div>
            <div class="field">
              <span>备注（可选）</span>
              <input type="text" placeholder="确认备注" .value=${this.ackNote} @input=${(e: any) => { this.ackNote = e.target.value; }} />
            </div>
          </div>
          <div class="modal-footer">
            <button class="btn btn--ghost" @click=${this.closeAckModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.ackSaving} @click=${this.confirmAck}>${this.ackSaving ? "确认中..." : "确认告警"}</button>
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
            <div class="alarm-modal-msg">${this.resolveAlarm?.message}</div>
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
            <button class="btn btn--primary" ?disabled=${this.resolveSaving} @click=${this.confirmResolve}>${this.resolveSaving ? "解决中..." : "解决告警"}</button>
          </div>
        </div>
      </div>
    `;
  }
}
