// web-lit/src/components/monitoring/performance-metrics-card.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi } from '../../services/devices'
import type { DeviceOnlineStatus, DeviceMetrics } from '../../services/devices'

@customElement('performance-metrics-card')
export class PerformanceMetricsCard extends LitElement {
  static styles = css`
    :host { display: block; }
    .card {
      background: var(--card);
      border-radius: var(--radius-lg);
      padding: 16px;
    }
    .card-title {
      font-size: 14px;
      font-weight: 600;
      margin-bottom: 16px;
    }
    .status-row {
      display: flex;
      align-items: center;
      gap: 12px;
      padding-bottom: 16px;
      margin-bottom: 16px;
      border-bottom: 1px solid var(--border);
    }
    .status-icon {
      width: 36px;
      height: 36px;
      border-radius: var(--radius-md);
      display: flex;
      align-items: center;
      justify-content: center;
      flex-shrink: 0;
    }
    .status-icon.online { background: var(--ok-subtle); color: var(--ok); }
    .status-icon.offline { background: var(--bg-muted); color: var(--muted); }
    .status-info { flex: 1; }
    .status-text { font-size: 14px; font-weight: 600; }
    .status-sub { font-size: 12px; color: var(--muted); }
    .status-quick-metrics {
      display: flex;
      gap: 20px;
    }
    .quick-metric { text-align: center; }
    .quick-metric-value { font-size: 16px; font-weight: 600; }
    .quick-metric-label { font-size: 11px; color: var(--muted); }
    .metrics-grid {
      display: grid;
      grid-template-columns: repeat(2, 1fr);
      gap: 12px;
    }
    .metric-item {
      background: var(--bg-muted);
      border-radius: var(--radius-md);
      padding: 12px;
    }
    .metric-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      margin-bottom: 8px;
    }
    .metric-label { font-size: 12px; color: var(--muted); }
    .metric-value { font-size: 24px; font-weight: 600; }
    .metric-unit { font-size: 12px; color: var(--muted); margin-left: 4px; }
    .metric-bar {
      height: 4px;
      background: var(--border);
      border-radius: 2px;
      margin-top: 8px;
      overflow: hidden;
    }
    .metric-bar-fill {
      height: 100%;
      border-radius: 2px;
      transition: width 0.3s;
    }
    .empty { text-align: center; padding: 24px; color: var(--muted); font-size: 13px; }
  `

  @property({ type: String }) deviceId = ''
  @state() private status: DeviceOnlineStatus | null = null
  @state() private metrics: DeviceMetrics | null = null

  updated(changed: Map<string, unknown>) {
    if (changed.has('deviceId') && this.deviceId) {
      this.loadData()
    }
  }

  private async loadData() {
    try {
      const [s, m] = await Promise.all([
        deviceApi.getDeviceStatus(this.deviceId),
        deviceApi.getDeviceMetrics(this.deviceId),
      ])
      this.status = s.result || null
      this.metrics = m.result || null
    } catch {
      this.status = null
      this.metrics = null
    }
  }

  private barColor(value: number): string {
    if (value >= 90) return 'var(--danger)'
    if (value >= 70) return 'var(--warn)'
    return 'var(--ok)'
  }

  render() {
    return html`
      <div class="card">
        <div class="card-title">设备监控</div>
        ${this.renderStatus()}
        ${this.renderMetrics()}
      </div>
    `
  }

  private renderStatus() {
    if (!this.status) {
      return html`<div class="status-row"><div class="empty" style="padding:8px 0">加载中...</div></div>`
    }
    return html`
      <div class="status-row">
        <div class="status-icon ${this.status.isOnline ? 'online' : 'offline'}">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            ${this.status.isOnline
              ? html`<path d="M5 12.55a11 11 0 0114.08 0M8.53 16.11a6 6 0 016.97 0M12 20h.01"/>`
              : html`<path d="M1 1l22 22M16.72 11.06A10.94 10.94 0 0119 12.55M5 12.55a11 11 0 015.71-2.63M12 20h.01"/>`
            }
          </svg>
        </div>
        <div class="status-info">
          <div class="status-text">${this.status.isOnline ? '在线' : '离线'}</div>
          <div class="status-sub">最后检查: ${this.status.lastCheck ? new Date(this.status.lastCheck).toLocaleTimeString() : '-'}</div>
        </div>
        ${this.metrics ? html`
          <div class="status-quick-metrics">
            <div class="quick-metric">
              <div class="quick-metric-value">${this.metrics.cpuUsage || 0}%</div>
              <div class="quick-metric-label">CPU</div>
            </div>
            <div class="quick-metric">
              <div class="quick-metric-value">${this.metrics.memoryUsage || 0}%</div>
              <div class="quick-metric-label">内存</div>
            </div>
            <div class="quick-metric">
              <div class="quick-metric-value">${this.metrics.temperature ?? '-'}${this.metrics.temperature != null ? '°C' : ''}</div>
              <div class="quick-metric-label">温度</div>
            </div>
          </div>
        ` : ''}
      </div>
    `
  }

  private renderMetrics() {
    if (!this.metrics) {
      return html`<div class="empty">加载中...</div>`
    }
    const m = this.metrics
    return html`
      <div class="metrics-grid">
        <div class="metric-item">
          <div class="metric-header">
            <span class="metric-label">CPU 使用率</span>
          </div>
          <div class="metric-value">${m.cpuUsage ?? 0}<span class="metric-unit">%</span></div>
          <div class="metric-bar">
            <div class="metric-bar-fill" style="width: ${m.cpuUsage || 0}%; background: ${this.barColor(m.cpuUsage || 0)};"></div>
          </div>
        </div>
        <div class="metric-item">
          <div class="metric-header">
            <span class="metric-label">内存使用率</span>
          </div>
          <div class="metric-value">${m.memoryUsage ?? 0}<span class="metric-unit">%</span></div>
          <div class="metric-bar">
            <div class="metric-bar-fill" style="width: ${m.memoryUsage || 0}%; background: ${this.barColor(m.memoryUsage || 0)};"></div>
          </div>
        </div>
        <div class="metric-item">
          <div class="metric-header">
            <span class="metric-label">磁盘使用率</span>
          </div>
          <div class="metric-value">${m.diskUsage ?? 0}<span class="metric-unit">%</span></div>
          <div class="metric-bar">
            <div class="metric-bar-fill" style="width: ${m.diskUsage || 0}%; background: ${this.barColor(m.diskUsage || 0)};"></div>
          </div>
        </div>
        <div class="metric-item">
          <div class="metric-header">
            <span class="metric-label">温度</span>
          </div>
          <div class="metric-value">${m.temperature ?? '-'}<span class="metric-unit">${m.temperature != null ? '°C' : ''}</span></div>
          <div class="metric-bar">
            <div class="metric-bar-fill" style="width: ${Math.min((m.temperature || 0), 100)}%; background: var(--accent);"></div>
          </div>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'performance-metrics-card': PerformanceMetricsCard }
}
