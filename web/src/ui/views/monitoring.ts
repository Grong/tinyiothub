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
      <div class="toolbar">
        <div class="toolbar__spacer"></div>
        <div class="toolbar__meta">
          ${this.lastUpdated ? html`
            <span class="last-updated">上次更新: ${this.lastUpdated}</span>
          ` : nothing}
          <button class="btn btn--ghost btn--sm" @click=${this.loadData}>刷新</button>
        </div>
      </div>
      <div class="grid grid-cols-2 gap-4">
        ${this.renderMetrics()}
        ${this.renderHealth()}
      </div>
    `;
  }

  renderMetrics() {
    const m = this.metrics;
    return html`
      <div class="card card--p-5">
        <div class="card__title">系统资源</div>
        ${m ? html`
          <div class="metric-group">
            ${this.renderBar("CPU", m.cpu)}
            ${this.renderBar("内存", m.memory)}
            ${this.renderBar("磁盘", m.disk)}
          </div>
          <div class="metric-meta-grid">
            <div>
              <div class="metric-meta-item__label">网络入站</div>
              <div class="metric-meta-item__value">${this.formatBytes(m.network.inbound)}/s</div>
            </div>
            <div>
              <div class="metric-meta-item__label">网络出站</div>
              <div class="metric-meta-item__value">${this.formatBytes(m.network.outbound)}/s</div>
            </div>
            <div>
              <div class="metric-meta-item__label">活跃连接</div>
              <div class="metric-meta-item__value">${m.activeConnections ?? '-'}</div>
            </div>
            <div>
              <div class="metric-meta-item__label">运行时间</div>
              <div class="metric-meta-item__value">${m.uptime ? this.formatUptime(m.uptime) : '-'}</div>
            </div>
          </div>
        ` : html`<div class="empty-hint--sm">暂无数据</div>`}
      </div>
    `;
  }

  renderHealth() {
    const h = this.health;
    const statusDotClass = (s: string) => s === "healthy" ? "status-dot status-dot--success" : s === "degraded" ? "status-dot status-dot--warning" : "status-dot status-dot--danger";
    const statusLabel = (s: string) => s === "healthy" ? "正常" : s === "degraded" ? "降级" : "异常";
    return html`
      <div class="card card--p-5">
        <div class="health-header">
          <div class="card__title card__title--flush">健康状态</div>
          ${h ? html`
            <span class="status-badge status-badge--subtle">
              <span class="${statusDotClass(h.status)}"></span>
              <span class="status-badge__label">${statusLabel(h.status)}</span>
            </span>
          ` : nothing}
        </div>
        ${h?.components?.length ? html`
          <div class="health-list">
            ${h.components!.map((c: ComponentHealth) => html`
              <div class="health-item">
                <div class="health-item__info">
                  <div class="health-item__name">${c.name}</div>
                  ${c.message ? html`<div class="health-item__message">${c.message}</div>` : nothing}
                  ${c.lastChecked ? html`<div class="health-item__checked">${c.lastChecked}</div>` : nothing}
                </div>
                <span class="health-item__status">
                  <span class="${statusDotClass(c.status)}"></span>
                  ${statusLabel(c.status)}
                </span>
              </div>
            `)}
          </div>
        ` : html`<div class="empty-hint--sm">暂无数据</div>`}
      </div>
    `;
  }

  renderBar(label: string, value?: number) {
    const v = value ?? 0;
    return html`
      <div>
        <div class="metric-bar__header">
          <span>${label}</span>
          <span>${v.toFixed(1)}%</span>
        </div>
        <div class="metric-bar__track metric-bar__track--md">
          <div class="metric-bar__fill" style="width: ${v}%;"></div>
        </div>
      </div>
    `;
  }
}
