import { LitElement, html} from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { deviceApi, type DeviceAlarm } from '../services/devices'
import { navigate } from '../lib/navigate'
import { $currentWorkspaceId } from '../stores/workspace-store'

@customElement('alarms-page')
export class AlarmsPage extends LitElement {
  createRenderRoot() { return this }
  

  @state() alarms: DeviceAlarm[] = []
  @state() loading = true
  @state() error: string | null = null
  @state() status = ''
  @state() level = ''
  @state() page = 1
  @state() pageSize = 10
  @state() totalCount = 0

  private _workspaceUnsub?: () => void

  async connectedCallback() {
    super.connectedCallback()
    await this.loadAlarms()
    this._workspaceUnsub = $currentWorkspaceId.subscribe(() => { this.loadAlarms() })
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    this._workspaceUnsub?.()
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
