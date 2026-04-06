// web-lit/src/components/property-chart-dialog.ts
import { LitElement, html} from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi } from '../services/devices'
import type { DeviceProperty, PerformanceHistory } from '../services/devices'

@customElement('property-chart-dialog')
export class PropertyChartDialog extends LitElement {
  createRenderRoot() { return this }
  

  @property({ type: Boolean }) open = false
  @property({ type: Object }) property!: DeviceProperty
  @property({ type: String }) deviceId = ''
  @state() timeRange = 1 // hours
  @state() data: PerformanceHistory | null = null
  @state() loading = true

  updated(changedProperties: Map<string, any>) {
    if (changedProperties.has('open') && this.open) {
      this.loadData()
    }
  }

  async loadData() {
    this.loading = true
    try {
      const res = await deviceApi.getDevicePerformance(this.deviceId, this.timeRange)
      if (res.result) {
        this.data = res.result
      }
    } finally {
      this.loading = false
    }
  }

  private setTimeRange(hours: number) {
    this.timeRange = hours
    this.loadData()
  }

  private close() {
    this.open = false
    this.dispatchEvent(new CustomEvent('close'))
  }

  render() {
    if (!this.open) return html``
    return html`
      <div class="overlay" @click=${() => this.close()}>
        <div class="dialog" @click=${(e: Event) => e.stopPropagation()}>
          <div class="header">
            <h3>属性历史: ${this.property?.displayName || this.property?.name}</h3>
            <button class="close-btn" @click=${() => this.close()}>
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 6L6 18M6 6l12 12"/>
              </svg>
            </button>
          </div>
          <div class="body">
            <div class="prop-info">
              <div class="prop-info-item">
                <span class="prop-info-label">ID:</span>
                <span class="prop-info-value" style="font-family: monospace; font-size: 12px;">${this.property?.id}</span>
              </div>
              <div class="prop-info-item">
                <span class="prop-info-label">名称:</span>
                <span class="prop-info-value">${this.property?.displayName || this.property?.name}</span>
              </div>
              <div class="prop-info-item">
                <span class="prop-info-label">类型:</span>
                <span class="prop-info-value">${this.property?.dataType}</span>
              </div>
              <div class="prop-info-item">
                <span class="prop-info-label">当前值:</span>
                <span class="prop-info-value">${this.property?.currentValue ?? this.property?.value ?? '-'}${this.property?.unit ? ` ${this.property.unit}` : ''}</span>
              </div>
            </div>
            <div class="time-range">
              ${[1, 6, 24, 168, 720].map(h => html`
                <button class="time-btn ${this.timeRange === h ? 'active' : ''}"
                  @click=${() => this.setTimeRange(h)}>
                  ${h === 1 ? '1小时' : h === 6 ? '6小时' : h === 24 ? '24小时' : h === 168 ? '7天' : '30天'}
                </button>
              `)}
            </div>
            ${this.loading ? html`<div class="no-data">加载中...</div>` :
              this.data?.data?.length ? this.renderChart() : html`<div class="no-data">暂无历史数据</div>`
            }
          </div>
        </div>
      </div>
    `
  }

  private renderChart() {
    const points = this.data?.data || []
    if (points.length < 2) return html`<div class="no-data">数据点不足</div>`
    const width = 700
    const height = 250
    const padding = 30
    const max = Math.max(...points.map(p => p.value))
    const min = Math.min(...points.map(p => p.value))
    const range = max - min || 1
    const coords = points.map((p, i) => {
      const x = padding + (i / (points.length - 1)) * (width - padding * 2)
      const y = height - padding - ((p.value - min) / range) * (height - padding * 2)
      return `${x},${y}`
    }).join(' ')
    return html`
      <div class="chart-container">
        <svg class="chart-svg" viewBox="0 0 ${width} ${height}">
          <polyline
            points=${coords}
            fill="none"
            stroke="var(--accent)"
            stroke-width="2"
          />
          ${points.map((p, i) => {
            const x = padding + (i / (points.length - 1)) * (width - padding * 2)
            const y = height - padding - ((p.value - min) / range) * (height - padding * 2)
            return html`<circle cx=${x} cy=${y} r="3" fill="var(--accent)"/>`
          })}
        </svg>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'property-chart-dialog': PropertyChartDialog }
}
