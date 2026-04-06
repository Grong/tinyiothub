import { LitElement, html} from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { dashboardApi, type DashboardMetrics } from '../services/dashboard'
import { $currentWorkspaceId } from '../stores/workspace-store'

@customElement('monitoring-page')
export class MonitoringPage extends LitElement {
  createRenderRoot() { return this }
  

  @state() metrics: DashboardMetrics | null = null
  @state() loading = true
  @state() error: string | null = null

  private _workspaceUnsub?: () => void

  async connectedCallback() {
    super.connectedCallback()
    await this.loadMetrics()
    this._workspaceUnsub = $currentWorkspaceId.subscribe(() => { this.loadMetrics() })
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    this._workspaceUnsub?.()
  }

  async loadMetrics() {
    this.loading = true
    this.error = null

    try {
      const response = await dashboardApi.getSystemMetrics()
      if (response.result) {
        this.metrics = response.result
      }
    } catch (err: any) {
      this.error = err.message || '加载监控数据失败'
    } finally {
      this.loading = false
    }
  }

  getBarClass(value: number): string {
    if (value < 60) return 'low'
    if (value < 85) return 'medium'
    return 'high'
  }

  render() {
    return html`
      <div class="page-header">
        <h1 class="page-title">系统监控</h1>
        <button
          class="refresh-btn ${this.loading ? 'loading' : ''}"
          @click=${() => this.loadMetrics()}
          ?disabled=${this.loading}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99"/>
          </svg>
          刷新
        </button>
      </div>

      ${this.loading ? this.renderLoading() : this.error ? this.renderError() : this.renderContent()}
    `
  }

  renderLoading() {
    return html`<div class="loading"><div class="spinner"></div></div>`
  }

  renderError() {
    return html`
      <div class="error-state">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z"/>
        </svg>
        <p>${this.error}</p>
        <button class="refresh-btn" @click=${() => this.loadMetrics()}>重试</button>
      </div>
    `
  }

  renderContent() {
    if (!this.metrics) return null

    const { cpu, memory, disk, network } = this.metrics

    return html`
      <!-- Metrics Grid -->
      <div class="metrics-grid">
        <div class="metric-card">
          <div class="metric-header">
            <span class="metric-label">CPU 使用率</span>
            <div class="metric-icon cpu">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z"/>
              </svg>
            </div>
          </div>
          <div>
            <span class="metric-value">${cpu}</span>
            <span class="metric-unit">%</span>
          </div>
          <div class="metric-bar">
            <div class="metric-bar-fill ${this.getBarClass(cpu)}" style="width: ${cpu}%"></div>
          </div>
        </div>

        <div class="metric-card">
          <div class="metric-header">
            <span class="metric-label">内存使用率</span>
            <div class="metric-icon memory">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M12 21a9.004 9.004 0 008.716-6.747M12 21a9.004 9.004 0 01-8.716-6.747M12 21c2.485 0 4.5-4.03 4.5-9S14.485 3 12 3m0 18c-2.485 0-4.5-4.03-4.5-9S9.515 3 12 3m0 0a8.997 8.997 0 017.843 4.582M12 3a8.997 8.997 0 00-7.843 4.582m15.686 0A11.953 11.953 0 0112 10.5c-2.998 0-5.74-1.1-7.843-2.918m15.686 0A8.959 8.959 0 0121 12c0 .778-.099 1.533-.284 2.253m0 0A17.919 17.919 0 0112 16.5c-3.162 0-6.133-.815-8.716-2.247m0 0A9.015 9.015 0 013 12c0-1.605.42-3.113 1.157-4.418"/>
              </svg>
            </div>
          </div>
          <div>
            <span class="metric-value">${memory}</span>
            <span class="metric-unit">%</span>
          </div>
          <div class="metric-bar">
            <div class="metric-bar-fill ${this.getBarClass(memory)}" style="width: ${memory}%"></div>
          </div>
        </div>

        <div class="metric-card">
          <div class="metric-header">
            <span class="metric-label">磁盘使用率</span>
            <div class="metric-icon disk">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M20.25 6.375c0 2.278-3.694 4.125-8.25 4.125S3.75 8.653 3.75 6.375m16.5 0c0-2.278-3.694-4.125-8.25-4.125S3.75 4.097 3.75 6.375m16.5 0v11.25c0 2.278-3.694 4.125-8.25 4.125s-8.25-1.847-8.25-4.125V6.375m16.5 0v3.75m-16.5-3.75v3.75m16.5 0v3.75C20.25 16.153 16.556 18 12 18s-8.25-1.847-8.25-4.125v-3.75m16.5 0c0 2.278-3.694 4.125-8.25 4.125s-8.25-1.847-8.25-4.125"/>
              </svg>
            </div>
          </div>
          <div>
            <span class="metric-value">${disk}</span>
            <span class="metric-unit">%</span>
          </div>
          <div class="metric-bar">
            <div class="metric-bar-fill ${this.getBarClass(disk)}" style="width: ${disk}%"></div>
          </div>
        </div>

        <div class="metric-card">
          <div class="metric-header">
            <span class="metric-label">网络 I/O</span>
            <div class="metric-icon network">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M8.288 15.038a5.25 5.25 0 017.424 0M5.106 11.856c3.807-3.808 9.98-3.808 13.788 0M1.924 8.674c5.565-5.565 14.587-5.565 20.152 0M12.53 18.22l-.53.53-.53-.53a.75.75 0 011.06 0z"/>
              </svg>
            </div>
          </div>
          <div>
            <span class="metric-value">${network.inbound}</span>
            <span class="metric-unit">MB/s</span>
          </div>
          <div style="font-size: 12px; color: var(--muted); margin-top: 8px;">
            入站 ${network.inbound} / 出站 ${network.outbound} MB/s
          </div>
        </div>
      </div>

      <!-- Charts -->
      <div class="grid-2">
        <div class="chart-card">
          <div class="chart-header">
            <h3 class="chart-title">实时数据流</h3>
          </div>
          <div class="chart-body">
            <div class="chart-placeholder">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 013 19.875v-6.75zM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V8.625zM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V4.125z"/>
              </svg>
              <p>实时图表</p>
            </div>
          </div>
        </div>

        <div class="chart-card">
          <div class="chart-header">
            <h3 class="chart-title">历史趋势</h3>
          </div>
          <div class="chart-body">
            <div class="chart-placeholder">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M2.25 18L9 11.25l4.306 4.307a11.95 11.95 0 015.814-5.519l2.74-1.22m0 0l-5.94-2.28m5.94 2.28l-2.28 5.941"/>
              </svg>
              <p>趋势图表</p>
            </div>
          </div>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'monitoring-page': MonitoringPage
  }
}
