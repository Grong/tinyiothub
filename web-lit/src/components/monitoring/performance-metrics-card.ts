// web-lit/src/components/monitoring/performance-metrics-card.ts
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'

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
    .metric-badge {
      font-size: 10px;
      padding: 2px 6px;
      border-radius: var(--radius-sm);
      background: var(--info);
      color: white;
    }
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
  `

  @property({ type: Object }) metrics: Record<string, number> = {}

  render() {
    return html`
      <div class="card">
        <div class="card-title">性能指标</div>
        <div class="metrics-grid">
          <div class="metric-item">
            <div class="metric-header">
              <span class="metric-label">CPU 使用率</span>
            </div>
            <div class="metric-value">${this.metrics.cpu_usage || 0}<span class="metric-unit">%</span></div>
            <div class="metric-bar">
              <div class="metric-bar-fill" style="width: ${this.metrics.cpu_usage || 0}%; background: var(--info);"></div>
            </div>
          </div>
          <div class="metric-item">
            <div class="metric-header">
              <span class="metric-label">内存使用率</span>
            </div>
            <div class="metric-value">${this.metrics.memory_usage || 0}<span class="metric-unit">%</span></div>
            <div class="metric-bar">
              <div class="metric-bar-fill" style="width: ${this.metrics.memory_usage || 0}%; background: var(--ok);"></div>
            </div>
          </div>
          <div class="metric-item">
            <div class="metric-header">
              <span class="metric-label">磁盘使用率</span>
            </div>
            <div class="metric-value">${this.metrics.disk_usage || 0}<span class="metric-unit">%</span></div>
            <div class="metric-bar">
              <div class="metric-bar-fill" style="width: ${this.metrics.disk_usage || 0}%; background: var(--warn);"></div>
            </div>
          </div>
          <div class="metric-item">
            <div class="metric-header">
              <span class="metric-label">网络带宽</span>
            </div>
            <div class="metric-value">${this.metrics.network_bandwidth || 0}<span class="metric-unit">Mbps</span></div>
            <div class="metric-bar">
              <div class="metric-bar-fill" style="width: ${Math.min((this.metrics.network_bandwidth || 0), 100)}%; background: var(--accent);"></div>
            </div>
          </div>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'performance-metrics-card': PerformanceMetricsCard }
}
