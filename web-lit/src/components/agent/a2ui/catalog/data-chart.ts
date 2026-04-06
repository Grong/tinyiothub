import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import { hostStyles } from '../../../../styles/shared-host'

interface DataPoint {
  x: number
  y: number
}

interface ChartSeries {
  name: string
  color?: string
  dataPoints: DataPoint[]
}

@customElement('data-chart')
export class DataChart extends LitElement {
  @property({ type: String }) title = ''
  @property({ type: Array }) series: ChartSeries[] = []
  @property({ type: Array }) labels: string[] = []
  @property({ type: Object }) stats: { min?: number; max?: number; avg?: number } = {}
  @property({ type: Number }) width = 400
  @property({ type: Number }) height = 200

  static styles = [hostStyles, css`
    :host { display: block; }
    .chart-container {
      background: var(--card);
      box-shadow: var(--glass-shadow-sm);
      border-radius: var(--radius);
      padding: 12px;
    }
    .title { font-size: 0.875rem; font-weight: 600; margin-bottom: 8px; }
    svg { width: 100%; height: 200px; }
    .grid-line { stroke: var(--border); stroke-width: 0.5; }
    .data-line { fill: none; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }
    .data-point { r: 3; }
    .label { font-size: 10px; fill: var(--text-muted); }
    .tooltip {
      position: absolute;
      background: var(--bg-elevated);
      color: var(--text-on-accent);
      padding: 4px 8px;
      border-radius: 4px;
      font-size: 0.75rem;
      pointer-events: none;
    }
    .stats {
      display: flex;
      gap: 16px;
      margin-top: 8px;
      font-size: 0.75rem;
      color: var(--text-muted);
    }
    .stats span { font-family: monospace; color: var(--text); }
  `]

  private _buildPath(dataPoints: DataPoint[], width: number, height: number, padding: number): string {
    if (dataPoints.length < 2) return ''
    const yValues = dataPoints.map(dp => dp.y)
    const xValues = dataPoints.map(dp => dp.x)
    const maxY = Math.max(...yValues)
    const minY = Math.min(...yValues)
    const rangeY = maxY - minY || 1
    const minX = Math.min(...xValues)
    const maxX = Math.max(...xValues)
    const rangeX = maxX - minX || 1

    return dataPoints.map((dp, i) => {
      const x = padding + ((dp.x - minX) / rangeX) * (width - padding * 2)
      const y = height - padding - ((dp.y - minY) / rangeY) * (height - padding * 2)
      return `${i === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`
    }).join(' ')
  }

  render() {
    const padding = 20
    const colors = ['#6366f1', '#22c55e', '#f59e0b', '#ef4444', '#8b5cf6']

    return html`
      <div class="chart-container">
        ${this.title ? html`<div class="title">${this.title}</div>` : ''}
        <svg viewBox="0 0 ${this.width} ${this.height}" preserveAspectRatio="xMidYMid meet">
          <!-- Grid lines -->
          ${[0, 0.25, 0.5, 0.75, 1].map(pct => {
            const y = padding + pct * (this.height - padding * 2)
            return html`<line class="grid-line" x1="${padding}" y1="${y}" x2="${this.width - padding}" y2="${y}" />`
          })}
          <!-- Data series -->
          ${this.series.map((s, si) => html`
            <path
              class="data-line"
              d="${this._buildPath(s.dataPoints, this.width, this.height, padding)}"
              stroke="${s.color || colors[si % colors.length]}"
            />
          `)}
          <!-- X-axis labels -->
          ${this.labels.length > 0 ? this.labels.map((label, i) => {
            const x = padding + i * ((this.width - padding * 2) / Math.max(this.labels.length - 1, 1))
            return html`<text class="label" x="${x}" y="${this.height - 4}" text-anchor="middle">${label}</text>`
          }) : ''}
        </svg>
        ${this.stats.min !== undefined ? html`
          <div class="stats">
            <div>Min: <span>${this.stats.min?.toFixed(1)}</span></div>
            <div>Max: <span>${this.stats.max?.toFixed(1)}</span></div>
            <div>Avg: <span>${this.stats.avg?.toFixed(1)}</span></div>
          </div>
        ` : ''}
      </div>
    `
  }
}
