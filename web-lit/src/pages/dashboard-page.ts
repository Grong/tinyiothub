import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { dashboardApi, type DashboardData, type RecentAlarm, type QuickDevice } from '../services/dashboard'
import { navigate } from '../lib/navigate'
import { $currentWorkspaceId } from '../stores/workspace-store'
import { hostStyles } from '../styles/shared-host'

@customElement('dashboard-page')
export class DashboardPage extends LitElement {
  static styles = [hostStyles, css`
    dashboard-page {
      display: block;
    }

    /* Header */
    .page-header {
      margin-bottom: 24px;
    }

    .page-title {
      font-size: 24px;
      font-weight: 700;
      color: var(--text-strong);
      margin: 0 0 8px;
      letter-spacing: -0.02em;
    }

    .page-subtitle {
      font-size: 14px;
      color: var(--muted);
      margin: 0;
    }

    /* Stats grid */
    .stats-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
      gap: 16px;
      margin-bottom: 24px;
    }

    .stat-card {
      background: var(--card);
      border-radius: var(--radius-lg);
      padding: 20px;
      box-shadow: var(--glass-shadow-sm);
      transition: box-shadow var(--duration-normal) ease;
      animation: rise 0.25s var(--ease-out) backwards;
    }

    .stat-card:hover {
      box-shadow: var(--glass-shadow-hover);
    }

    .stat-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      margin-bottom: 12px;
    }

    .stat-label {
      font-size: 13px;
      font-weight: 500;
      color: var(--muted);
    }

    .stat-icon {
      width: 32px;
      height: 32px;
      border-radius: var(--radius-md);
      display: flex;
      align-items: center;
      justify-content: center;
    }

    .stat-icon svg {
      width: 18px;
      height: 18px;
    }

    .stat-icon.devices {
      background: var(--accent-subtle);
      color: var(--accent);
    }

    .stat-icon.online {
      background: var(--ok-subtle);
      color: var(--ok);
    }

    .stat-icon.alarms {
      background: var(--warn-subtle);
      color: var(--warn);
    }

    .stat-icon.critical {
      background: var(--danger-subtle);
      color: var(--danger);
    }

    .stat-value {
      font-size: 32px;
      font-weight: 700;
      color: var(--text-strong);
      letter-spacing: -0.03em;
      line-height: 1.1;
    }

    .stat-change {
      font-size: 12px;
      color: var(--muted);
      margin-top: 8px;
    }

    .stat-change.positive {
      color: var(--ok);
    }

    .stat-card--danger {
      border-left: 3px solid var(--danger);
      background: color-mix(in srgb, var(--card) 92%, var(--danger) 8%);
    }

    .stat-card--danger .stat-value {
      color: var(--danger);
    }

    /* Main grid */
    .main-grid {
      display: grid;
      grid-template-columns: 2fr 1fr;
      gap: 24px;
    }

    @media (max-width: 1200px) {
      .main-grid {
        grid-template-columns: 1fr;
      }
    }

    /* Card */
    .card {
      background: var(--card);
      border-radius: var(--radius-lg);
      overflow: hidden;
      box-shadow: var(--glass-shadow-sm);
    }

    .card-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 16px 20px;
      box-shadow: 0 1px 0 var(--card-highlight);
    }

    .card-title {
      font-size: 15px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0;
    }

    .card-action {
      font-size: 13px;
      color: var(--accent);
      cursor: pointer;
      text-decoration: none;
    }

    .card-action:hover {
      text-decoration: underline;
    }

    .card-body {
      padding: 16px 20px;
    }

    /* Device distribution */
    .device-dist {
      display: grid;
      grid-template-columns: repeat(2, 1fr);
      gap: 12px;
    }

    .dist-item {
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 12px;
      background: var(--bg);
      border-radius: var(--radius-md);
    }

    .dist-dot {
      width: 10px;
      height: 10px;
      border-radius: 50%;
      flex-shrink: 0;
    }

    .dist-dot.online {
      background: var(--ok);
      box-shadow: 0 0 8px var(--ok);
    }

    .dist-dot.offline {
      background: var(--muted);
    }

    .dist-dot.warning {
      background: var(--warn);
      box-shadow: 0 0 8px var(--warn);
    }

    .dist-dot.error {
      background: var(--danger);
      box-shadow: 0 0 8px var(--danger);
    }

    .dist-info {
      flex: 1;
    }

    .dist-label {
      font-size: 12px;
      color: var(--muted);
    }

    .dist-value {
      font-size: 18px;
      font-weight: 600;
      color: var(--text-strong);
    }

    /* Recent alarms */
    .alarm-list {
      display: flex;
      flex-direction: column;
    }

    .alarm-item {
      display: flex;
      align-items: flex-start;
      gap: 12px;
      padding: 12px 0;
      box-shadow: 0 1px 0 var(--card-highlight);
    }

    .alarm-item:last-child {
      box-shadow: none;
    }

    .alarm-level {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      margin-top: 6px;
      flex-shrink: 0;
    }

    .alarm-level.critical {
      background: var(--danger);
      box-shadow: 0 0 8px var(--danger);
    }

    .alarm-level.warning {
      background: var(--warn);
    }

    .alarm-level.info {
      background: var(--info);
    }

    .alarm-content {
      flex: 1;
      min-width: 0;
    }

    .alarm-message {
      font-size: 13px;
      color: var(--text);
      margin: 0 0 4px;
      word-break: break-word;
    }

    .alarm-meta {
      font-size: 12px;
      color: var(--muted);
    }

    .alarm-device {
      color: var(--accent);
    }

    /* Quick devices */
    .device-grid {
      display: grid;
      grid-template-columns: repeat(2, 1fr);
      gap: 12px;
    }

    .device-item {
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 12px;
      background: var(--bg);
      border-radius: var(--radius-md);
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .device-item:hover {
      background: var(--bg-hover);
    }

    .device-status {
      width: 10px;
      height: 10px;
      border-radius: 50%;
      flex-shrink: 0;
    }

    .device-status.online {
      background: var(--ok);
    }

    .device-status.offline {
      background: var(--muted);
    }

    .device-status.error {
      background: var(--danger);
    }

    .device-info {
      flex: 1;
      min-width: 0;
    }

    .device-name {
      font-size: 13px;
      font-weight: 500;
      color: var(--text);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .device-protocol {
      font-size: 11px;
      color: var(--muted);
    }

    /* Empty state */
    .empty-state {
      text-align: center;
      padding: 32px 20px;
      color: var(--muted);
    }

    .empty-state svg {
      width: 48px;
      height: 48px;
      margin-bottom: 12px;
      opacity: 0.5;
    }

    .empty-state p {
      margin: 0;
      font-size: 13px;
    }

    /* Loading skeleton */
    .skeleton {
      background: linear-gradient(90deg, var(--bg-muted) 25%, var(--bg-hover) 50%, var(--bg-muted) 75%);
      background-size: 200% 100%;
      animation: shimmer 1.5s ease-in-out infinite;
      border-radius: var(--radius-md);
    }

    @keyframes shimmer {
      0% { background-position: 200% 0; }
      100% { background-position: -200% 0; }
    }

    .skeleton-stat {
      height: 100px;
      border-radius: var(--radius-lg);
    }
  `]

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
