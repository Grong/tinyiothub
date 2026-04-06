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
      ${this.renderStatsCards()}
      <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 16px; margin-top: 16px;">
        ${this.renderDeviceDistribution()}
        ${this.renderSystemMetrics()}
      </div>
      <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 16px; margin-top: 16px;">
        ${this.renderRecentAlarms()}
        ${this.renderQuickDevices()}
      </div>
    `;
  }

  renderStatsCards() {
    const s = this.stats;
    return html`
      <div style="display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px;">
        <div class="card" style="padding: 20px;">
          <div style="color: var(--muted); font-size: 13px;">设备总数</div>
          <div style="font-size: 28px; font-weight: 700; margin: 8px 0;">${this.formatNumber(s?.totalDevices)}</div>
          <div style="color: var(--success); font-size: 13px;">${this.formatNumber(s?.onlineDevices)} 在线</div>
        </div>
        <div class="card" style="padding: 20px;">
          <div style="color: var(--muted); font-size: 13px;">活跃告警</div>
          <div style="font-size: 28px; font-weight: 700; margin: 8px 0; color: ${(s?.activeAlarms ?? 0) > 0 ? 'var(--danger)' : 'inherit'}">
            ${this.formatNumber(s?.activeAlarms)}
          </div>
          <div style="color: var(--muted); font-size: 13px;">需要处理</div>
        </div>
        <div class="card" style="padding: 20px;">
          <div style="color: var(--muted); font-size: 13px;">今日消息</div>
          <div style="font-size: 28px; font-weight: 700; margin: 8px 0;">${this.formatNumber(s?.todayMessages)}</div>
          <div style="font-size: 13px; color: var(--muted);">
            ${s?.monthlyGrowth?.messages != null ? `月增长 ${s.monthlyGrowth.messages}%` : ""}
          </div>
        </div>
        <div class="card" style="padding: 20px;">
          <div style="color: var(--muted); font-size: 13px;">系统状态</div>
          <div style="font-size: 28px; font-weight: 700; margin: 8px 0; color: ${s?.systemStatus === 'healthy' ? 'var(--success)' : s?.systemStatus === 'warning' ? 'var(--warning)' : 'var(--danger)'}">
            ${s?.systemStatus === 'healthy' ? '正常' : s?.systemStatus === 'warning' ? '告警' : '异常'}
          </div>
          <div style="font-size: 13px; color: var(--muted);">
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
        ` : html`<div style="color: var(--muted); text-align: center; padding: 20px;">暂无数据</div>`}
      </div>
    `;
  }

  renderDistBar(label: string, value: number, total: number, color: string) {
    const pct = total > 0 ? (value / total) * 100 : 0;
    return html`
      <div>
        <div style="display: flex; justify-content: space-between; font-size: 13px; margin-bottom: 4px;">
          <span>${label}</span>
          <span>${value} (${pct.toFixed(1)}%)</span>
        </div>
        <div style="height: 6px; background: var(--border); border-radius: 3px; overflow: hidden;">
          <div style="height: 100%; width: ${pct}%; background: ${color}; border-radius: 3px;"></div>
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
        ` : html`<div style="color: var(--muted); text-align: center; padding: 20px;">暂无数据</div>`}
      </div>
    `;
  }

  renderMetricBar(label: string, value?: number) {
    const v = value ?? 0;
    const color = v > 90 ? "var(--danger)" : v > 70 ? "var(--warning)" : "var(--success)";
    return html`
      <div>
        <div style="display: flex; justify-content: space-between; font-size: 13px; margin-bottom: 4px;">
          <span>${label}</span>
          <span>${v.toFixed(1)}%</span>
        </div>
        <div style="height: 6px; background: var(--border); border-radius: 3px; overflow: hidden;">
          <div style="height: 100%; width: ${v}%; background: ${color}; border-radius: 3px;"></div>
        </div>
      </div>
    `;
  }

  renderRecentAlarms() {
    return html`
      <div class="card" style="padding: 20px;">
        <div style="font-weight: 600; margin-bottom: 16px;">最近告警</div>
        ${this.recentAlarms.length === 0
          ? html`<div style="color: var(--muted); text-align: center; padding: 20px;">暂无告警</div>`
          : html`
            <div style="display: flex; flex-direction: column; gap: 8px;">
              ${this.recentAlarms.slice(0, 5).map(a => html`
                <div style="display: flex; align-items: center; gap: 8px; padding: 8px; border-radius: 6px; background: var(--bg-subtle);">
                  <span style="width: 8px; height: 8px; border-radius: 50%; background: ${this.levelColor(a.level)}; flex-shrink: 0;"></span>
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
          ? html`<div style="color: var(--muted); text-align: center; padding: 20px;">暂无设备</div>`
          : html`
            <div style="display: flex; flex-direction: column; gap: 8px;">
              ${this.quickDevices.slice(0, 5).map(d => html`
                <a href="/devices/${d.id}"
                  style="display: flex; align-items: center; gap: 8px; padding: 8px; border-radius: 6px; background: var(--bg-subtle); text-decoration: none; color: inherit;"
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
