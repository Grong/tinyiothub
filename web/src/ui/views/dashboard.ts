import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import { ApiClient } from "../../api/client.js";
import type {
  DashboardStats,
  DeviceStatusDistribution,
  RecentAlarm,
  DashboardMetrics,
  QuickDevice,
} from "../../types/index.js";

@customElement("view-dashboard")
export class DashboardView extends LitElement {
  @state() loading = true;
  @state() error = "";
  @state() stats?: DashboardStats;
  @state() distribution?: DeviceStatusDistribution;
  @state() recentAlarms: RecentAlarm[] = [];
  @state() metrics?: DashboardMetrics;
  @state() quickDevices: QuickDevice[] = [];

  createRenderRoot() {
    return this;
  }

  async connectedCallback() {
    super.connectedCallback();
    await this.loadData();
  }

  async loadData() {
    this.loading = true;
    this.error = "";
    try {
      const [statsRes, distRes, alarmsRes, devicesRes, metricsRes] = await Promise.all([
        ApiClient.get<any>("/monitoring/stats"),
        ApiClient.get<any>("/devices/distribution"),
        ApiClient.get<any>("/alarms/recent", { limit: 10 }),
        ApiClient.get<any>("/devices/quick", { limit: 8 }),
        ApiClient.get<any>("/monitoring/metrics"),
      ]);

      this.stats = statsRes.result || undefined;
      this.distribution = distRes.result || undefined;
      this.recentAlarms = (alarmsRes.result || []).map((a: any) => ({
        id: a.id,
        deviceId: a.deviceId,
        deviceName: a.deviceName,
        level: a.level,
        message: a.message,
        createdAt: a.createdAt,
        status: a.status,
      }));
      this.quickDevices = (devicesRes.result || []).map((d: any) => ({
        id: d.id,
        name: d.name,
        status: d.status,
        lastSeen: d.lastSeen,
        type: d.type || d.deviceType || "unknown",
      }));
      this.metrics = metricsRes.result || undefined;
    } catch (err: any) {
      this.error = err.message || "加载仪表盘失败";
    } finally {
      this.loading = false;
    }
  }

  formatUptime(seconds: number): string {
    const d = Math.floor(seconds / 86400);
    const h = Math.floor((seconds % 86400) / 3600);
    if (d > 0) return `${d}天 ${h}小时`;
    return `${h}小时`;
  }

  formatNumber(n: number | undefined): string {
    if (n == null) return "0";
    return n.toLocaleString();
  }

  levelColor(level: string): string {
    switch (level) {
      case "critical": return "var(--danger)";
      case "error": return "var(--danger)";
      case "warning": return "var(--warning)";
      default: return "var(--muted)";
    }
  }

  statusColor(status: string): string {
    switch (status) {
      case "online": return "var(--success)";
      case "offline": return "var(--muted)";
      case "error": return "var(--danger)";
      case "maintenance": return "var(--warning)";
      default: return "var(--muted)";
    }
  }

  render() {
    if (this.loading) {
      return html`
        <div style="display: flex; align-items: center; justify-content: center; padding: 60px;">
          <span class="loading-spinner"></span>
          <span style="margin-left: 8px; color: var(--muted);">加载仪表盘...</span>
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
      ${this.renderStatsCards()}
      <div class="grid grid-cols-2" style="margin-top: 16px;">
        ${this.renderDeviceDistribution()}
        ${this.renderSystemMetrics()}
      </div>
      <div class="grid grid-cols-2" style="margin-top: 16px;">
        ${this.renderRecentAlarms()}
        ${this.renderQuickDevices()}
      </div>
    `;
  }

  renderStatsCards() {
    const s = this.stats;
    return html`
      <div class="stats-grid">
        <div class="card stat-card">
          <div class="stat-card__label">设备总数</div>
          <div class="stat-card__value">${this.formatNumber(s?.totalDevices)}</div>
          <div class="stat-card__meta" style="color: var(--success);">${this.formatNumber(s?.onlineDevices)} 在线</div>
        </div>
        <div class="card stat-card">
          <div class="stat-card__label">活跃告警</div>
          <div class="stat-card__value" style="color: ${(s?.activeAlarms ?? 0) > 0 ? 'var(--danger)' : 'inherit'};">
            ${this.formatNumber(s?.activeAlarms)}
          </div>
          <div class="stat-card__meta">需要处理</div>
        </div>
        <div class="card stat-card">
          <div class="stat-card__label">今日消息</div>
          <div class="stat-card__value">${this.formatNumber(s?.todayMessages)}</div>
          <div class="stat-card__meta">
            ${s?.monthlyGrowth?.messages != null ? `月增长 ${s.monthlyGrowth.messages}%` : ""}
          </div>
        </div>
        <div class="card stat-card">
          <div class="stat-card__label">系统状态</div>
          <div class="stat-card__value" style="color: ${s?.systemStatus === 'healthy' ? 'var(--success)' : s?.systemStatus === 'warning' ? 'var(--warning)' : 'var(--danger)'};">
            ${s?.systemStatus === 'healthy' ? '正常' : s?.systemStatus === 'warning' ? '告警' : '异常'}
          </div>
          <div class="stat-card__meta">
            ${s?.systemUptime != null ? `运行 ${this.formatUptime(s.systemUptime)}` : ""}
          </div>
        </div>
      </div>
    `;
  }

  renderDeviceDistribution() {
    const d = this.distribution;
    const total = (d?.online ?? 0) + (d?.offline ?? 0) + (d?.error ?? 0) + (d?.maintenance ?? 0);
    return html`
      <div class="card" style="padding: 20px;">
        <div style="font-weight: 600; margin-bottom: 16px;">设备状态分布</div>
        ${d ? html`
          <div style="display: flex; flex-direction: column; gap: 12px;">
            ${this.renderDistBar("在线", d.online, total, "var(--success)")}
            ${this.renderDistBar("离线", d.offline, total, "var(--muted)")}
            ${this.renderDistBar("故障", d.error, total, "var(--danger)")}
            ${this.renderDistBar("维护", d.maintenance, total, "var(--warning)")}
          </div>
        ` : html`<div style="color: var(--muted); text-align: center; padding: 20px;">暂无分布数据</div>`}
      </div>
    `;
  }

  renderDistBar(label: string, value: number, total: number, color: string) {
    const pct = total > 0 ? (value / total) * 100 : 0;
    return html`
      <div>
        <div class="metric-bar__header">
          <span>${label}</span>
          <span>${value} (${pct.toFixed(1)}%)</span>
        </div>
        <div class="metric-bar__track">
          <div class="metric-bar__fill" style="width: ${pct}%; background: ${color};"></div>
        </div>
      </div>
    `;
  }

  renderSystemMetrics() {
    const m = this.metrics;
    return html`
      <div class="card" style="padding: 20px;">
        <div style="font-weight: 600; margin-bottom: 16px;">系统资源</div>
        ${m ? html`
          <div style="display: flex; flex-direction: column; gap: 12px;">
            ${this.renderMetricBar("CPU", m.cpu)}
            ${this.renderMetricBar("内存", m.memory)}
            ${this.renderMetricBar("磁盘", m.disk)}
          </div>
          <div style="margin-top: 16px; font-size: 13px; color: var(--muted);">
            网络: ↓ ${this.formatNumber(m.network?.inbound)} / ↑ ${this.formatNumber(m.network?.outbound)} bytes
          </div>
        ` : html`<div style="color: var(--muted); text-align: center; padding: 20px;">暂无系统资源数据</div>`}
      </div>
    `;
  }

  renderMetricBar(label: string, value?: number) {
    const v = value ?? 0;
    const color = v > 90 ? "var(--danger)" : v > 70 ? "var(--warning)" : "var(--success)";
    return html`
      <div>
        <div class="metric-bar__header">
          <span>${label}</span>
          <span>${v.toFixed(1)}%</span>
        </div>
        <div class="metric-bar__track">
          <div class="metric-bar__fill" style="width: ${v}%; background: ${color};"></div>
        </div>
      </div>
    `;
  }

  renderRecentAlarms() {
    return html`
      <div class="card" style="padding: 20px;">
        <div style="font-weight: 600; margin-bottom: 16px;">最近告警</div>
        ${this.recentAlarms.length === 0
          ? html`
            <div class="empty-center">
              <div class="empty-center__icon">
                <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/>
                </svg>
              </div>
              <div class="empty-center__text">暂无告警，系统运行正常</div>
            </div>`
          : html`
            <div style="display: flex; flex-direction: column; gap: 8px;">
              ${this.recentAlarms.slice(0, 5).map(a => html`
                <div style="display: flex; align-items: center; gap: 8px; padding: 8px; border-radius: 6px; background: var(--bg-subtle);">
                  <span class="status-dot" style="background: ${this.levelColor(a.level)};"></span>
                  <div style="flex: 1; min-width: 0;">
                    <div style="font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${a.message}</div>
                    <div style="font-size: 12px; color: var(--muted);">${a.deviceName}</div>
                  </div>
                  <span style="font-size: 12px; color: var(--muted); flex-shrink: 0;">${a.createdAt?.slice(0, 16)}</span>
                </div>
              `)}
            </div>
          `
        }
      </div>
    `;
  }

  renderQuickDevices() {
    return html`
      <div class="card" style="padding: 20px;">
        <div style="font-weight: 600; margin-bottom: 16px;">设备快捷入口</div>
        ${this.quickDevices.length === 0
          ? html`
            <div class="empty-center">
              <div class="empty-center__icon">
                <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>
                </svg>
              </div>
              <div class="empty-center__text">还没有设备</div>
              <a href="/devices" @click=${(e: Event) => { e.preventDefault(); window.history.pushState({}, "", "/devices"); window.dispatchEvent(new PopStateEvent("popstate")); }} style="font-size: 13px; color: var(--accent); text-decoration: none; margin-top: 4px; display: inline-block;">去添加 →</a>
            </div>`
          : html`
            <div style="display: flex; flex-direction: column; gap: 8px;">
              ${this.quickDevices.slice(0, 5).map(d => html`
                <a href="/devices/${d.id}"
                  class="device-list-item"
                  @click=${(e: Event) => { e.preventDefault(); window.history.pushState({}, "", `/devices/${d.id}`); window.dispatchEvent(new PopStateEvent("popstate")); }}
                >
                  <span style="width: 8px; height: 8px; border-radius: 50%; background: ${this.statusColor(d.status)}; flex-shrink: 0;"></span>
                  <div style="flex: 1;">
                    <div style="font-size: 13px;">${d.name}</div>
                    <div style="font-size: 12px; color: var(--muted);">${d.type}</div>
                  </div>
                  <span style="font-size: 12px; color: var(--muted);">${d.status === 'online' ? '在线' : d.status === 'offline' ? '离线' : d.status === 'error' ? '故障' : '维护'}</span>
                </a>
              `)}
            </div>
          `
        }
      </div>
    `;
  }
}
