import { LitElement, html} from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { dashboardApi, type DashboardData, type RecentAlarm, type QuickDevice } from '../services/dashboard'
import { navigate } from '../lib/navigate'
import { $currentWorkspaceId } from '../stores/workspace-store'

@customElement('dashboard-page')
export class DashboardPage extends LitElement {
  createRenderRoot() { return this }
  

  @state() data: DashboardData | null = null
  @state() loading = true
  @state() error: string | null = null

  private _workspaceUnsub?: () => void

  async connectedCallback() {
    super.connectedCallback()
    await this.loadData()
    this._workspaceUnsub = $currentWorkspaceId.subscribe(() => { this.loadData() })
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    this._workspaceUnsub?.()
  }

  async loadData() {
    this.loading = true
    this.error = null

    try {
      const response = await dashboardApi.getDashboardData()
      if (response.result) {
        this.data = response.result
      }
    } catch (err: any) {
      this.error = err.message || '加载数据失败'
    } finally {
      this.loading = false
    }
  }

  render() {
    return html`
      <div class="page-header">
        <h1 class="page-title">仪表盘</h1>
        <p class="page-subtitle">查看系统概览和关键指标</p>
      </div>

      ${this.loading ? this.renderSkeleton() : this.error ? this.renderError() : this.renderContent()}
    `
  }

  renderSkeleton() {
    return html`
      <div class="stats-grid">
        ${[1, 2, 3, 4].map(() => html`<div class="skeleton skeleton-stat"></div>`)}
      </div>
    `
  }

  renderError() {
    return html`
      <div class="card">
        <div class="card-body empty-state">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z" />
          </svg>
          <p>${this.error}</p>
          <button class="card-action" @click=${this.loadData}>重试</button>
        </div>
      </div>
    `
  }

  renderContent() {
    if (!this.data) return null

    const { stats, deviceDistribution, recentAlarms, quickDevices } = this.data

    return html`
      <!-- Stats Grid -->
      <div class="stats-grid">
        <div class="stat-card">
          <div class="stat-header">
            <span class="stat-label">设备总数</span>
            <div class="stat-icon devices">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M9 17.25v1.007a3 3 0 01-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0115 18.257V17.25m6-12V15a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 15V5.25m18 0A2.25 2.25 0 0018.75 3H5.25A2.25 2.25 0 003 5.25m18 0V12a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 12V5.25" />
              </svg>
            </div>
          </div>
          <div class="stat-value">${stats.totalDevices}</div>
          <div class="stat-change">设备总数</div>
        </div>

        <div class="stat-card">
          <div class="stat-header">
            <span class="stat-label">在线设备</span>
            <div class="stat-icon online">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
              </svg>
            </div>
          </div>
          <div class="stat-value" style="color: var(--ok)">${stats.onlineDevices}</div>
          <div class="stat-change positive">正常运行</div>
        </div>

        <div class="stat-card">
          <div class="stat-header">
            <span class="stat-label">活跃告警</span>
            <div class="stat-icon alarms">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M14.857 17.082a23.848 23.848 0 005.454-1.31A8.967 8.967 0 0118 9.75v-.7V9A6 6 0 006 9v.75a8.967 8.967 0 01-2.312 6.022c1.733.64 3.56 1.085 5.455 1.31m5.714 0a24.255 24.255 0 01-5.714 0m5.714 0a3 3 0 11-5.714 0" />
              </svg>
            </div>
          </div>
          <div class="stat-value" style="color: var(--warn)">${stats.activeAlarms}</div>
          <div class="stat-change">需要关注</div>
        </div>

        <div class="stat-card stat-card--danger">
          <div class="stat-header">
            <span class="stat-label">严重告警</span>
            <div class="stat-icon critical">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z" />
              </svg>
            </div>
          </div>
          <div class="stat-value" style="color: var(--danger)">${stats.criticalAlarms}</div>
          <div class="stat-change">紧急处理</div>
        </div>
      </div>

      <!-- Main Grid -->
      <div class="main-grid">
        <!-- Left column -->
        <div>
          <!-- Device Distribution -->
          <div class="card" style="margin-bottom: 24px;">
            <div class="card-header">
              <h3 class="card-title">设备状态分布</h3>
              <a class="card-action" href="/devices">查看全部</a>
            </div>
            <div class="card-body">
              <div class="device-dist">
                <div class="dist-item">
                  <span class="dist-dot online"></span>
                  <div class="dist-info">
                    <div class="dist-label">在线</div>
                    <div class="dist-value">${deviceDistribution.online}</div>
                  </div>
                </div>
                <div class="dist-item">
                  <span class="dist-dot offline"></span>
                  <div class="dist-info">
                    <div class="dist-label">离线</div>
                    <div class="dist-value">${deviceDistribution.offline}</div>
                  </div>
                </div>
                <div class="dist-item">
                  <span class="dist-dot warning"></span>
                  <div class="dist-info">
                    <div class="dist-label">警告</div>
                    <div class="dist-value">${deviceDistribution.warning || 0}</div>
                  </div>
                </div>
                <div class="dist-item">
                  <span class="dist-dot error"></span>
                  <div class="dist-info">
                    <div class="dist-label">错误</div>
                    <div class="dist-value">${deviceDistribution.error || 0}</div>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <!-- Quick Devices -->
          <div class="card">
            <div class="card-header">
              <h3 class="card-title">快速设备</h3>
              <a class="card-action" href="/devices">管理设备</a>
            </div>
            <div class="card-body">
              ${quickDevices.length > 0 ? html`
                <div class="device-grid">
                  ${quickDevices.map(device => this.renderQuickDevice(device))}
                </div>
              ` : html`
                <div class="empty-state">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M9 17.25v1.007a3 3 0 01-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0115 18.257V17.25m6-12V15a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 15V5.25m18 0A2.25 2.25 0 0018.75 3H5.25A2.25 2.25 0 003 5.25m18 0V12a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 12V5.25" />
                  </svg>
                  <p>暂无设备，<a href="/devices" class="card-action">添加设备</a></p>
                </div>
              `}
            </div>
          </div>
        </div>

        <!-- Right column -->
        <div>
          <!-- Recent Alarms -->
          <div class="card">
            <div class="card-header">
              <h3 class="card-title">最近告警</h3>
              <a class="card-action" href="/alarms">查看全部</a>
            </div>
            <div class="card-body">
              ${recentAlarms.length > 0 ? html`
                <div class="alarm-list">
                  ${recentAlarms.map(alarm => this.renderAlarm(alarm))}
                </div>
              ` : html`
                <div class="empty-state">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  <p>暂无告警</p>
                </div>
              `}
            </div>
          </div>
        </div>
      </div>
    `
  }

  renderQuickDevice(device: QuickDevice) {
    return html`
      <div class="device-item" @click=${() => navigate(`device-detail?id=${device.id}`)}>
        <span class="device-status ${device.status}"></span>
        <div class="device-info">
          <div class="device-name">${device.name}</div>
          <div class="device-protocol">${device.protocol}</div>
        </div>
      </div>
    `
  }

  renderAlarm(alarm: RecentAlarm) {
    const levelClass = alarm.level >= 3 ? 'critical' : alarm.level >= 2 ? 'warning' : 'info'
    const timeAgo = this.formatTimeAgo(alarm.timestamp)

    return html`
      <div class="alarm-item">
        <span class="alarm-level ${levelClass}"></span>
        <div class="alarm-content">
          <p class="alarm-message">${alarm.message}</p>
          <div class="alarm-meta">
            <span class="alarm-device">${alarm.deviceName}</span>
            · ${timeAgo}
          </div>
        </div>
      </div>
    `
  }

  formatTimeAgo(timestamp: string): string {
    const now = new Date()
    const time = new Date(timestamp)
    const seconds = Math.floor((now.getTime() - time.getTime()) / 1000)

    if (seconds < 60) return '刚刚'
    if (seconds < 3600) return `${Math.floor(seconds / 60)}分钟前`
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}小时前`
    return `${Math.floor(seconds / 86400)}天前`
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'dashboard-page': DashboardPage
  }
}
