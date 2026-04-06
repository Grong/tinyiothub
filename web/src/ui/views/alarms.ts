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
    switch (status) {
      case "Active": return "活跃";
      case "Acknowledged": return "已确认";
      case "Resolved": return "已解决";
      case "Suppressed": return "已抑制";
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
      ${this.renderStats()}
      <div style="display: flex; gap: 12px; margin-bottom: 16px;">
        <select .value=${this.filterStatus} @change=${(e: Event) => { this.filterStatus = (e.target as HTMLSelectElement).value; this.page = 1; this.loadData(); }}>
          <option value="">全部状态</option>
          <option value="Active">活跃</option>
          <option value="Acknowledged">已确认</option>
          <option value="Resolved">已解决</option>
        </select>
        <select .value=${this.filterLevel} @change=${(e: Event) => { this.filterLevel = (e.target as HTMLSelectElement).value; this.page = 1; this.loadData(); }}>
          <option value="">全部级别</option>
          <option value="Critical">严重</option>
          <option value="Error">错误</option>
          <option value="Warning">警告</option>
          <option value="Info">信息</option>
        </select>
      </div>
      <div class="card" style="overflow: hidden;">
        <table style="width: 100%; border-collapse: collapse;">
          <thead>
            <tr style="border-bottom: 1px solid var(--border);">
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">级别</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">设备</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">告警信息</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">状态</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">时间</th>
              <th style="padding: 12px 16px; text-align: right; font-size: 13px; color: var(--muted); font-weight: 500;">操作</th>
            </tr>
          </thead>
          <tbody>
            ${this.alarms.length === 0
              ? html`<tr><td colspan="6" style="padding: 40px; text-align: center; color: var(--muted);">暂无告警</td></tr>`
              : this.alarms.map(a => html`
                <tr style="border-bottom: 1px solid var(--border);">
                  <td style="padding: 12px 16px;">
                    <span style="display: inline-flex; align-items: center; gap: 6px; font-size: 13px;">
                      <span style="width: 8px; height: 8px; border-radius: 50%; background: ${this.levelColor(a.alarmLevel)};"></span>
                      ${this.levelLabel(a.alarmLevel)}
                    </span>
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px;">${a.deviceName || "-"}</td>
                  <td style="padding: 12px 16px; font-size: 13px; max-width: 300px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${a.message}</td>
                  <td style="padding: 12px 16px;">
                    <span style="display: inline-flex; align-items: center; gap: 6px; font-size: 13px;">
                      <span style="width: 8px; height: 8px; border-radius: 50%; background: ${a.status === 'Active' ? 'var(--danger)' : a.status === 'Acknowledged' ? 'var(--warning)' : 'var(--success)'};"></span>
                      ${this.statusLabel(a.status)}
                    </span>
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px; color: var(--muted);">${a.alarmTime?.slice(0, 16) || a.createdAt?.slice(0, 16)}</td>
                  <td style="padding: 12px 16px; text-align: right;">
                    ${a.status === "Active" ? html`
                      <button class="btn btn--ghost btn--sm" style="font-size: 12px;" @click=${() => this.openAck(a)}>确认</button>
                      <button class="btn btn--ghost btn--sm" style="font-size: 12px; color: var(--success);" @click=${() => this.openResolve(a)}>解决</button>
                    ` : nothing}
                    ${a.status === "Acknowledged" ? html`
                      <button class="btn btn--ghost btn--sm" style="font-size: 12px; color: var(--success);" @click=${() => this.openResolve(a)}>解决</button>
                    ` : nothing}
                    ${a.status === "Resolved" ? html`
                      <span style="font-size: 12px; color: var(--muted);">-</span>
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
      <div style="display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; margin-bottom: 16px;">
        <div class="card" style="padding: 16px;">
          <div style="color: var(--muted); font-size: 12px;">总告警数</div>
          <div style="font-size: 24px; font-weight: 700; margin-top: 4px;">${s.totalCount}</div>
        </div>
        <div class="card" style="padding: 16px;">
          <div style="color: var(--muted); font-size: 12px;">活跃告警</div>
          <div style="font-size: 24px; font-weight: 700; margin-top: 4px; color: var(--danger);">${s.activeCount}</div>
        </div>
        <div class="card" style="padding: 16px;">
          <div style="color: var(--muted); font-size: 12px;">已确认</div>
          <div style="font-size: 24px; font-weight: 700; margin-top: 4px; color: var(--warning);">${s.acknowledgedCount}</div>
        </div>
        <div class="card" style="padding: 16px;">
          <div style="color: var(--muted); font-size: 12px;">已解决</div>
          <div style="font-size: 24px; font-weight: 700; margin-top: 4px; color: var(--success);">${s.resolvedCount}</div>
        </div>
      </div>
    `;
  }

  renderAckModal() {
    return html`
      <div class="modal-overlay" @click=${this.closeAckModal}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">确认告警</div>
          <div class="modal-body">
            <div style="font-size: 13px; color: var(--muted); margin-bottom: 12px;">
              ${this.ackAlarm?.message}
            </div>
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
      <div class="modal-overlay" @click=${this.closeResolveModal}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">解决告警</div>
          <div class="modal-body">
            <div style="font-size: 13px; color: var(--muted); margin-bottom: 12px;">
              ${this.resolveAlarm?.message}
            </div>
            <div class="field">
              <span>解决方式</span>
              <select .value=${this.resolveType} @change=${(e: Event) => { this.resolveType = (e.target as HTMLSelectElement).value as ResolutionType; }} style="width: 100%;">
                <option value="Fixed">已修复</option>
                <option value="FalseAlarm">误报</option>
                <option value="Ignored">忽略</option>
                <option value="AutoResolved">自动恢复</option>
              </select>
            </div>
            <div class="field" style="margin-top: 12px;">
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
