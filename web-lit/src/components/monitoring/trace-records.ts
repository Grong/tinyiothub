// web-lit/src/components/monitoring/trace-records.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi, type DeviceTrace } from '../../services/devices'
import { hostStyles } from '../../styles/shared-host'

@customElement('trace-records')
export class TraceRecords extends LitElement {
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
    .trace-item {
      display: flex;
      align-items: flex-start;
      gap: 12px;
      padding: 10px 0;
      box-shadow: 0 1px 0 var(--card-highlight);
    }
    .trace-level {
      font-size: 10px;
      padding: 2px 6px;
      border-radius: var(--radius-sm);
      text-transform: uppercase;
    }
    .trace-level.info { background: var(--info); color: white; }
    .trace-level.warning { background: var(--warn); color: black; }
    .trace-level.error { background: var(--danger); color: white; }
    .trace-content { flex: 1; }
    .trace-title { font-size: 13px; font-weight: 500; margin-bottom: 2px; }
    .trace-message { font-size: 12px; color: var(--muted); }
    .trace-time { font-size: 11px; color: var(--muted); white-space: nowrap; }
    .empty { text-align: center; padding: 32px; color: var(--muted); }
  `]

  @property({ type: String }) deviceId = ''
  @state() traces: DeviceTrace[] = []
  @state() loading = true

  async connectedCallback() {
    super.connectedCallback()
    await this.loadTraces()
  }

  async loadTraces() {
    this.loading = true
    try {
      const res = await deviceApi.getDeviceTraces(this.deviceId, { limit: 50 })
      if (res.result) this.traces = res.result
    } finally {
      this.loading = false
    }
  }

  render() {
    const content = this.loading
      ? html`<div class="empty">加载中...</div>`
      : !this.traces?.length
        ? html`<div class="empty">暂无追踪记录</div>`
        : html`
          ${this.traces.map(t => html`
            <div class="trace-item">
              <span class="trace-level ${t.level}">${t.level}</span>
              <div class="trace-content">
                <div class="trace-title">${t.title}</div>
                <div class="trace-message">${t.message}</div>
              </div>
              <span class="trace-time">${new Date(t.createdAt).toLocaleString()}</span>
            </div>
          `)}
        `
    return html`
      <div class="card">
        <div class="card-title">追踪记录</div>
        ${content}
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'trace-records': TraceRecords }
}
