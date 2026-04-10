import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { monitoringApi } from "../../api/monitoring.js";
import type { SystemMetrics, HealthStatus, ComponentHealth } from "../../types/index.js";

@customElement("view-monitoring")
export class MonitoringView extends LitElement {
  @state() loading = true;
  @state() error = "";
  @state() metrics?: SystemMetrics;
  @state() health?: HealthStatus;
  @state() lastUpdated = "";

  private refreshTimer: ReturnType<typeof setInterval> | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadData();
    this.refreshTimer = setInterval(() => this.loadData(), 30000);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    if (this.refreshTimer) {
      clearInterval(this.refreshTimer);
      this.refreshTimer = null;
    }
  }

  async loadData() {
    this.error = "";
    try {
      const [metricsRes, healthRes] = await Promise.all([
        monitoringApi.getSystemMetrics(),
        monitoringApi.getHealthStatus(),
      ]);
      this.metrics = metricsRes.result || undefined;
      console.log('[Monitoring] metrics:', metricsRes.result);
      this.health = healthRes.result || undefined;
      this.lastUpdated = new Date().toLocaleTimeString();
    } catch (err: any) {
      this.error = err.message || "加载监控数据失败";
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

  formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1048576).toFixed(1)} MB`;
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
      <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;">
        <div></div>
        <div style="display: flex; align-items: center; gap: 12px;">
          ${this.lastUpdated ? html`
            <span style="font-size: 13px; color: var(--muted);">上次更新: ${this.lastUpdated}</span>
          ` : nothing}
          <button class="btn btn--ghost" style="padding: 6px 12px; font-size: 13px;" @click=${this.loadData}>刷新</button>
        </div>
      </div>
      <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 16px;">
        ${this.renderMetrics()}
        ${this.renderHealth()}
      </div>
    `;
  }

  renderMetrics() {
    const m = this.metrics;
    return html`
      <div class="card" style="padding: 20px;">
        <div style="font-weight: 600; margin-bottom: 16px;">系统资源</div>
        ${m ? html`
          <div style="display: flex; flex-direction: column; gap: 16px;">
            ${this.renderBar("CPU", m.cpu)}
            ${this.renderBar("内存", m.memory)}
            ${this.renderBar("磁盘", m.disk)}
          </div>
          <div style="margin-top: 20px; display: grid; grid-template-columns: 1fr 1fr; gap: 12px;">
            <div>
              <div style="color: var(--muted); font-size: 12px;">网络入站</div>
              <div style="font-size: 16px; font-weight: 600; margin-top: 4px;">${this.formatBytes(m.network.inbound)}/s</div>
            </div>
            <div>
              <div style="color: var(--muted); font-size: 12px;">网络出站</div>
              <div style="font-size: 16px; font-weight: 600; margin-top: 4px;">${this.formatBytes(m.network.outbound)}/s</div>
            </div>
            <div>
              <div style="color: var(--muted); font-size: 12px;">活跃连接</div>
              <div style="font-size: 16px; font-weight: 600; margin-top: 4px;">${m.activeConnections ?? '-'}</div>
            </div>
            <div>
              <div style="color: var(--muted); font-size: 12px;">运行时间</div>
              <div style="font-size: 16px; font-weight: 600; margin-top: 4px;">${m.uptime ? this.formatUptime(m.uptime) : '-'}</div>
            </div>
          </div>
        ` : html`<div style="color: var(--muted); text-align: center; padding: 20px;">暂无数据</div>`}
      </div>
    `;
  }

  renderHealth() {
    const h = this.health;
    const statusColor = (s: string) => s === "healthy" ? "var(--success)" : s === "degraded" ? "var(--warning)" : "var(--danger)";
    const statusLabel = (s: string) => s === "healthy" ? "正常" : s === "degraded" ? "降级" : "异常";
    return html`
      <div class="card" style="padding: 20px;">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;">
          <div style="font-weight: 600;">健康状态</div>
          ${h ? html`
            <span style="display: inline-flex; align-items: center; gap: 6px; padding: 4px 12px; border-radius: 9999px; font-size: 13px; background: var(--bg-subtle);">
              <span style="width: 8px; height: 8px; border-radius: 50%; background: ${statusColor(h.status)};"></span>
              ${statusLabel(h.status)}
            </span>
          ` : nothing}
        </div>
        ${h?.components?.length ? html`
          <div style="display: flex; flex-direction: column; gap: 8px;">
            ${h.components!.map((c: ComponentHealth) => html`
              <div style="display: flex; align-items: center; justify-content: space-between; padding: 10px 12px; background: var(--bg-subtle); border-radius: 8px;">
                <div>
                  <div style="font-size: 14px; font-weight: 500;">${c.name}</div>
                  ${c.message ? html`<div style="font-size: 12px; color: var(--muted);">${c.message}</div>` : nothing}
                  ${c.lastChecked ? html`<div style="font-size: 11px; color: var(--muted); margin-top: 2px;">${c.lastChecked}</div>` : nothing}
                </div>
                <span style="display: inline-flex; align-items: center; gap: 6px; font-size: 13px;">
                  <span style="width: 8px; height: 8px; border-radius: 50%; background: ${statusColor(c.status)};"></span>
                  ${statusLabel(c.status)}
                </span>
              </div>
            `)}
          </div>
        ` : html`<div style="color: var(--muted); text-align: center; padding: 20px;">暂无数据</div>`}
      </div>
    `;
  }

  renderBar(label: string, value?: number) {
    const v = value ?? 0;
    const color = v > 90 ? "var(--danger)" : v > 70 ? "var(--warning)" : "var(--success)";
    return html`
      <div>
        <div style="display: flex; justify-content: space-between; font-size: 13px; margin-bottom: 4px;">
          <span>${label}</span>
          <span>${v.toFixed(1)}%</span>
        </div>
        <div style="height: 16px; background: #e5e7eb; border-radius: 3px; overflow: hidden;">
          <div style="height: 100%; width: ${v}%; background: ${color}; border-radius: 2px; transition: width 0.3s;"></div>
        </div>
      </div>
    `;
  }
}
