// web-lit/src/components/monitoring/trace-records.ts
import { LitElement, html} from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi, type DeviceTrace } from '../../services/devices'

@customElement('trace-records')
export class TraceRecords extends LitElement {
  createRenderRoot() { return this }
  

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
