// web-lit/src/components/monitoring/performance-alerts.ts
import { LitElement, html} from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi } from '../../services/devices'
import type { PerformanceAlert } from '../../services/devices'

@customElement('performance-alerts')
export class PerformanceAlerts extends LitElement {
  createRenderRoot() { return this }
  

  @property({ type: String }) deviceId = ''
  @state() private alerts: PerformanceAlert[] = []

  updated(changed: Map<string, unknown>) {
    if (changed.has('deviceId') && this.deviceId) {
      this.loadData()
    }
  }

  private async loadData() {
    try {
      const resp = await deviceApi.getDevicePerformanceAlerts(this.deviceId)
      this.alerts = resp.result || []
    } catch {
      this.alerts = []
    }
  }

  render() {
    if (!this.alerts?.length) {
      return html`<div class="card"><div class="card-title">性能告警</div><div class="empty">暂无告警</div></div>`
    }
    return html`
      <div class="card">
        <div class="card-title">性能告警</div>
        ${this.alerts.map(alert => html`
          <div class="alert-item">
            <div class="alert-dot ${alert.level}"></div>
            <div class="alert-content">
              <div class="alert-message">${alert.message}</div>
              <div class="alert-meta">${new Date(alert.triggeredAt).toLocaleString()} - ${alert.alertType}</div>
            </div>
          </div>
        `)}
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'performance-alerts': PerformanceAlerts }
}
