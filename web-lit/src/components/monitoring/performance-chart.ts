// web-lit/src/components/monitoring/performance-chart.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi, type PerformanceHistory } from '../../services/devices'
import uPlot from 'uplot'
import 'uplot/dist/uPlot.min.css'
import { hostStyles } from '../../styles/shared-host'

@customElement('performance-chart')
export class PerformanceChart extends LitElement {
  static styles = [hostStyles, css`
    :host { display: block; }
    .chart-container {
      background: var(--card);
      border-radius: var(--radius-lg);
      padding: 16px;
    }
    .card-title {
      font-size: 14px;
      font-weight: 600;
      margin-bottom: 16px;
    }
    .time-range {
      display: flex;
      gap: 8px;
      margin-bottom: 16px;
    }
    .time-btn {
      padding: 6px 12px;
      border: none;
      border-radius: var(--radius-md);
      background: var(--bg-secondary);
      color: var(--text);
      font-size: 12px;
      cursor: pointer;
    }
    .time-btn.active { background: var(--accent); color: white; }
    .chart { width: 100%; height: 300px; }
    .no-data {
      display: flex;
      align-items: center;
      justify-content: center;
      height: 200px;
      color: var(--muted);
    }
  `]

  @property({ type: String }) deviceId = ''
  @property({ type: Number }) refreshInterval = 10000
  @state() timeRange = 1
  @state() data: PerformanceHistory | null = null
  @state() loading = true
  private chart: uPlot | null = null
  private interval: number | null = null

  firstUpdated() {
    this.loadData()
    this.interval = window.setInterval(() => this.loadData(), this.refreshInterval)
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    if (this.interval) {
      clearInterval(this.interval)
      this.interval = null
    }
    if (this.chart) {
      this.chart.destroy()
      this.chart = null
    }
  }

  async loadData() {
    const res = await deviceApi.getDevicePerformance(this.deviceId, this.timeRange)
    if (res.result) {
      this.data = res.result
      this.renderChart()
    }
    this.loading = false
  }

  private renderChart() {
    if (!this.data?.data?.length) return
    const d = this.data.data
    const timestamps = d.map(p => p.timestamp / 1000)
    const values = d.map(p => p.value)
    const opts: uPlot.Options = {
      width: this.querySelector('.chart')?.clientWidth || 600,
      height: 280,
      series: [
        {},
        { label: this.data.metric, stroke: '#3b82f6', width: 2 },
      ],
    }
    if (this.chart) this.chart.destroy()
    this.chart = new uPlot(opts, [timestamps, values], this.querySelector('.chart') as HTMLElement)
  }

  private setTimeRange(h: number) {
    this.timeRange = h
    this.loadData()
  }

  render() {
    return html`
      <div class="chart-container">
        <div class="card-title">性能趋势</div>
        <div class="time-range">
          ${[1, 6, 24, 168, 720].map(h => html`
            <button class="time-btn ${this.timeRange === h ? 'active' : ''}" @click=${() => this.setTimeRange(h)}>
              ${h === 1 ? '1小时' : h === 6 ? '6小时' : h === 24 ? '24小时' : h === 168 ? '7天' : '30天'}
            </button>
          `)}
        </div>
        ${this.loading ? html`<div class="no-data">加载中...</div>` : html`<div class="chart"></div>`}
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'performance-chart': PerformanceChart }
}
