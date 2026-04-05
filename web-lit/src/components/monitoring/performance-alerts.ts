// web-lit/src/components/monitoring/performance-alerts.ts
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { PerformanceAlert } from '../../services/devices'

@customElement('performance-alerts')
export class PerformanceAlerts extends LitElement {
  static styles = css`
    :host { display: block; }
    .alert-item {
      display: flex;
      align-items: flex-start;
      gap: 12px;
      padding: 12px;
      background: var(--card);
      border-radius: var(--radius-md);
      margin-bottom: 8px;
    }
    .alert-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      margin-top: 6px;
      flex-shrink: 0;
    }
    .alert-dot.info { background: var(--info); }
    .alert-dot.warning { background: var(--warn); }
    .alert-dot.error { background: var(--danger); }
    .alert-dot.critical { background: var(--danger); box-shadow: 0 0 6px var(--danger); }
    .alert-content { flex: 1; }
    .alert-message { font-size: 13px; margin-bottom: 4px; }
    .alert-meta { font-size: 11px; color: var(--muted); }
    .empty { text-align: center; padding: 32px; color: var(--muted); }
  `

  @property({ type: Array }) alerts: PerformanceAlert[] = []

  render() {
    if (!this.alerts?.length) {
      return html`<div class="empty">暂无告警</div>`
    }
    return html`
      ${this.alerts.map(alert => html`
        <div class="alert-item">
          <div class="alert-dot ${alert.level}"></div>
          <div class="alert-content">
            <div class="alert-message">${alert.message}</div>
            <div class="alert-meta">${new Date(alert.triggered_at).toLocaleString()} - ${alert.alert_type}</div>
          </div>
        </div>
      `)}
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'performance-alerts': PerformanceAlerts }
}
