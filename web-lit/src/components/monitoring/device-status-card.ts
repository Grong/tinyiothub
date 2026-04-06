// web-lit/src/components/monitoring/device-status-card.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi } from '../../services/devices'
import type { DeviceOnlineStatus, DeviceMetrics } from '../../services/devices'
import { hostStyles } from '../../styles/shared-host'

@customElement('device-status-card')
export class DeviceStatusCard extends LitElement {
  static styles = [hostStyles, css`
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
      margin-bottom: 12px;
    }
    .status-icon {
      width: 40px;
      height: 40px;
      border-radius: var(--radius-md);
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .status-icon.online { background: var(--ok-subtle); color: var(--ok); }
    .status-icon.offline { background: var(--bg-muted); color: var(--muted); }
    .status-text { font-size: 16px; font-weight: 600; }
    .status-sub { font-size: 12px; color: var(--muted); }
    .metrics-row {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 12px;
    }
    .metric-item { text-align: center; }
    .metric-value { font-size: 18px; font-weight: 600; }
    .metric-label { font-size: 11px; color: var(--muted); }
    .empty { text-align: center; padding: 24px; color: var(--muted); font-size: 13px; }
  `]

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

  render() {
    if (!this.status) {
      return html`<div class="card"><div class="card-title">设备状态</div><div class="empty">加载中...</div></div>`
    }
    return html`
      <div class="card">
        <div class="card-title">设备状态</div>
        <div class="status-row">
          <div class="status-icon ${this.status.isOnline ? 'online' : 'offline'}">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              ${this.status.isOnline
                ? html`<path d="M5 12.55a11 11 0 0114.08 0M8.53 16.11a6 6 0 016.97 0M12 20h.01"/>`
                : html`<path d="M1 1l22 22M16.72 11.06A10.94 10.94 0 0119 12.55M5 12.55a11 11 0 015.71-2.63M12 20h.01"/>`
              }
            </svg>
          </div>
          <div>
            <div class="status-text">${this.status.isOnline ? '在线' : '离线'}</div>
            <div class="status-sub">最后检查: ${this.status.lastCheck ? new Date(this.status.lastCheck).toLocaleTimeString() : '-'}</div>
          </div>
        </div>
        ${this.metrics ? html`
          <div class="metrics-row">
            <div class="metric-item">
              <div class="metric-value">${this.metrics.cpuUsage || 0}%</div>
              <div class="metric-label">CPU</div>
            </div>
            <div class="metric-item">
              <div class="metric-value">${this.metrics.memoryUsage || 0}%</div>
              <div class="metric-label">内存</div>
            </div>
            <div class="metric-item">
              <div class="metric-value">${this.metrics.temperature || '-'}${this.metrics.temperature ? '°C' : ''}</div>
              <div class="metric-label">温度</div>
            </div>
          </div>
        ` : ''}
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'device-status-card': DeviceStatusCard }
}
