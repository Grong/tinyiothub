import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { deviceApi, type DeviceAlarm } from '../services/devices'
import { navigate } from '../lib/navigate'

@customElement('alarms-page')
export class AlarmsPage extends LitElement {
  static styles = css`
    alarms-page {
      display: flex;
      flex-direction: column;
      padding: 24px;
      background: var(--bg);
      flex: 1;
      min-height: 0;
    }

    /* Header */
    .page-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      margin-bottom: 24px;
    }

    .page-title {
      font-size: 24px;
      font-weight: 700;
      color: var(--text-strong);
      margin: 0;
    }

    .header-actions {
      display: flex;
      gap: 12px;
    }

    .btn {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 10px 16px;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      font-weight: 500;
      cursor: pointer;
      transition: border-color var(--duration-fast) ease, background var(--duration-fast) ease;
    }

    .btn:hover {
      background: var(--bg-hover);
      border-color: var(--border-strong);
    }

    .btn-primary {
      background: var(--accent);
      border-color: var(--accent);
      color: var(--accent-foreground);
    }

    .btn-primary:hover {
      background: var(--accent-hover);
      border-color: var(--accent-hover);
    }

    /* Filters */
    .filters {
      display: flex;
      gap: 12px;
      margin-bottom: 20px;
      flex-wrap: wrap;
    }

    .filter-select {
      padding: 10px 14px;
      border: 1px solid var(--input);
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      cursor: pointer;
    }

    /* Alarm list */
    .alarm-list {
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      overflow: hidden;
    }

    .alarm-item {
      display: flex;
      align-items: flex-start;
      gap: 16px;
      padding: 16px 20px;
      border-bottom: 1px solid var(--border);
      transition: background var(--duration-fast) ease;
    }

    .alarm-item:last-child {
      border-bottom: none;
    }

    .alarm-item:hover {
      background: var(--bg-hover);
    }

    .alarm-level {
      width: 12px;
      height: 12px;
      border-radius: 50%;
      margin-top: 4px;
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

    .alarm-header {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      gap: 16px;
      margin-bottom: 8px;
    }

    .alarm-message {
      font-size: 14px;
      font-weight: 500;
      color: var(--text-strong);
      margin: 0;
    }

    .alarm-badge {
      padding: 4px 8px;
      border-radius: var(--radius-sm);
      font-size: 11px;
      font-weight: 500;
      text-transform: uppercase;
    }

    .alarm-badge.active {
      background: var(--danger-subtle);
      color: var(--danger);
    }

    .alarm-badge.acknowledged {
      background: var(--warn-subtle);
      color: var(--warn);
    }

    .alarm-badge.resolved {
      background: var(--ok-subtle);
      color: var(--ok);
    }

    .alarm-meta {
      display: flex;
      gap: 16px;
      font-size: 13px;
      color: var(--muted);
    }

    .alarm-device {
      color: var(--accent);
      cursor: pointer;
    }

    .alarm-device:hover {
      text-decoration: underline;
    }

    .alarm-actions {
      display: flex;
      gap: 8px;
      flex-shrink: 0;
    }

    .action-btn {
      padding: 6px 12px;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 12px;
      cursor: pointer;
      transition: background var(--duration-fast) ease, border-color var(--duration-fast) ease;
    }

    .action-btn:hover {
      background: var(--bg-hover);
      border-color: var(--border-strong);
    }

    .action-btn.primary {
      background: var(--accent);
      border-color: var(--accent);
      color: var(--accent-foreground);
    }

    .action-btn.primary:hover {
      background: var(--accent-hover);
    }

    /* Empty state */
    .empty-state {
      text-align: center;
      padding: 64px 24px;
      color: var(--muted);
    }

    .empty-state svg {
      width: 64px;
      height: 64px;
      margin-bottom: 16px;
      opacity: 0.5;
    }

    .empty-state h3 {
      font-size: 16px;
      font-weight: 600;
      color: var(--text);
      margin: 0 0 8px;
    }

    .empty-state p {
      font-size: 14px;
      margin: 0;
    }

    /* Loading */
    .loading {
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 64px 24px;
    }

    .spinner {
      width: 32px;
      height: 32px;
      border: 3px solid var(--border);
      border-top-color: var(--accent);
      border-radius: 50%;
      animation: spin 1s linear infinite;
    }

    @keyframes spin {
      to { transform: rotate(360deg); }
    }

    /* Pagination */
    .pagination {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 16px 20px;
      border-top: 1px solid var(--border);
    }

    .pagination-info {
      font-size: 13px;
      color: var(--muted);
    }

    .pagination-buttons {
      display: flex;
      gap: 8px;
    }

    .page-btn {
      padding: 8px 12px;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      cursor: pointer;
      transition: border-color var(--duration-fast) ease, background var(--duration-fast) ease;
    }

    .page-btn:hover:not(:disabled) {
      background: var(--bg-hover);
      border-color: var(--border-strong);
    }

    .page-btn:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  `

  @state() alarms: DeviceAlarm[] = []
  @state() loading = true
  @state() error: string | null = null
  @state() status = ''
  @state() level = ''
  @state() page = 1
  @state() pageSize = 10
  @state() totalCount = 0

  async connectedCallback() {
    super.connectedCallback()
    await this.loadAlarms()
  }

  async loadAlarms() {
    this.loading = true
    this.error = null

    try {
      const response = await deviceApi.getDeviceAlarms({
        page: this.page,
        pageSize: this.pageSize,
        status: this.status || undefined,
      })

      if (response.result) {
        this.alarms = response.result.data || []
        this.totalCount = response.result.pagination?.totalCount || 0
      }
    } catch (err: any) {
      this.error = err.message || '加载告警失败'
    } finally {
      this.loading = false
    }
  }

  handleStatusChange(e: Event) {
    this.status = (e.target as HTMLSelectElement).value
    this.page = 1
    this.loadAlarms()
  }

  handleLevelChange(e: Event) {
    this.level = (e.target as HTMLSelectElement).value
    this.page = 1
    this.loadAlarms()
  }

  handlePageChange(newPage: number) {
    this.page = newPage
    this.loadAlarms()
  }

  async acknowledgeAlarm(alarm: DeviceAlarm, e: Event) {
    e.stopPropagation()
    try {
      await deviceApi.acknowledgeAlarm(alarm.id)
      await this.loadAlarms()
    } catch (err: any) {
      alert(err.message || '操作失败')
    }
  }

  async resolveAlarm(alarm: DeviceAlarm, e: Event) {
    e.stopPropagation()
    try {
      await deviceApi.resolveAlarm(alarm.id)
      await this.loadAlarms()
    } catch (err: any) {
      alert(err.message || '操作失败')
    }
  }

  viewDevice(deviceId: string) {
    navigate(`device-detail?id=${deviceId}`)
  }

  render() {
    return html`
      <div class="page-header">
        <h1 class="page-title">告警管理</h1>
        <div class="header-actions">
          <button class="btn" @click=${() => this.loadAlarms()}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99"/>
            </svg>
            刷新
          </button>
        </div>
      </div>

      <div class="filters">
        <select class="filter-select" .value=${this.status} @change=${this.handleStatusChange}>
          <option value="">全部状态</option>
          <option value="active">活跃</option>
          <option value="acknowledged">已确认</option>
          <option value="resolved">已解决</option>
        </select>
        <select class="filter-select" .value=${this.level} @change=${this.handleLevelChange}>
          <option value="">全部级别</option>
          <option value="info">信息</option>
          <option value="warning">警告</option>
          <option value="error">错误</option>
          <option value="critical">严重</option>
        </select>
      </div>

      ${this.loading ? this.renderLoading() : this.error ? this.renderError() : this.renderAlarmList()}
    `
  }

  renderLoading() {
    return html`
      <div class="alarm-list">
        <div class="loading">
          <div class="spinner"></div>
        </div>
      </div>
    `
  }

  renderError() {
    return html`
      <div class="alarm-list">
        <div class="empty-state">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z"/>
          </svg>
          <h3>加载失败</h3>
          <p>${this.error}</p>
          <button class="btn btn-primary" @click=${() => this.loadAlarms()}>重试</button>
        </div>
      </div>
    `
  }

  renderAlarmList() {
    if (this.alarms.length === 0) {
      return html`
        <div class="alarm-list">
          <div class="empty-state">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
            </svg>
            <h3>暂无告警</h3>
            <p>系统运行正常，没有告警信息</p>
          </div>
        </div>
      `
    }

    const totalPages = Math.ceil(this.totalCount / this.pageSize)

    return html`
      <div class="alarm-list">
        ${this.alarms.map(alarm => html`
          <div class="alarm-item">
            <span class="alarm-level ${alarm.level || 'info'}"></span>
            <div class="alarm-content">
              <div class="alarm-header">
                <p class="alarm-message">${alarm.message || alarm.alarmType || '告警消息'}</p>
                <span class="alarm-badge ${alarm.status || 'active'}">${this.getStatusText(alarm.status)}</span>
              </div>
              <div class="alarm-meta">
                <span class="alarm-device" @click=${() => this.viewDevice(alarm.deviceId)}>
                  ${alarm.deviceName || alarm.deviceId}
                </span>
                <span>${this.formatTime(alarm.timestamp)}</span>
              </div>
            </div>
            <div class="alarm-actions">
              ${alarm.status !== 'acknowledged' ? html`
                <button class="action-btn" @click=${(e: Event) => this.acknowledgeAlarm(alarm, e)}>确认</button>
              ` : ''}
              ${alarm.status !== 'resolved' ? html`
                <button class="action-btn primary" @click=${(e: Event) => this.resolveAlarm(alarm, e)}>解决</button>
              ` : ''}
            </div>
          </div>
        `)}

        <div class="pagination">
          <span class="pagination-info">
            显示 ${(this.page - 1) * this.pageSize + 1} - ${Math.min(this.page * this.pageSize, this.totalCount)}，共 ${this.totalCount} 条
          </span>
          <div class="pagination-buttons">
            <button class="page-btn" ?disabled=${this.page <= 1} @click=${() => this.handlePageChange(this.page - 1)}>上一页</button>
            <button class="page-btn" ?disabled=${this.page >= totalPages} @click=${() => this.handlePageChange(this.page + 1)}>下一页</button>
          </div>
        </div>
      </div>
    `
  }

  getStatusText(status?: string): string {
    switch (status) {
      case 'active': return '活跃'
      case 'acknowledged': return '已确认'
      case 'resolved': return '已解决'
      default: return '活跃'
    }
  }

  formatTime(timestamp?: string): string {
    if (!timestamp) return '-'
    const date = new Date(timestamp)
    return date.toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    })
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'alarms-page': AlarmsPage
  }
}
