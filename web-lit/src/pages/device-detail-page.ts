import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { deviceApi, type DeviceProfile } from '../services/devices'
import { navigate } from '../lib/navigate'

@customElement('device-detail-page')
export class DeviceDetailPage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
      background: var(--bg);
      min-height: 100%;
    }

    /* Header */
    .page-header {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      margin-bottom: 24px;
    }

    .header-left {
      display: flex;
      align-items: center;
      gap: 16px;
    }

    .back-btn {
      width: 36px;
      height: 36px;
      display: flex;
      align-items: center;
      justify-content: center;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--muted);
      cursor: pointer;
      transition: background var(--duration-fast) ease, color var(--duration-fast) ease;
    }

    .back-btn:hover {
      background: var(--bg-hover);
      color: var(--text);
    }

    .back-btn svg {
      width: 18px;
      height: 18px;
    }

    .page-title {
      font-size: 24px;
      font-weight: 700;
      color: var(--text-strong);
      margin: 0;
    }

    .device-id {
      font-size: 13px;
      color: var(--muted);
      font-family: var(--mono);
      margin-top: 4px;
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

    .btn-danger {
      border-color: var(--danger);
      color: var(--danger);
    }

    .btn-danger:hover {
      background: var(--danger-subtle);
    }

    /* Status badge */
    .status-badge {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 8px 14px;
      border-radius: var(--radius-full);
      font-size: 13px;
      font-weight: 500;
    }

    .status-badge.online {
      background: var(--ok-subtle);
      color: var(--ok);
    }

    .status-badge.offline {
      background: var(--bg-muted);
      color: var(--muted);
    }

    .status-badge.error {
      background: var(--danger-subtle);
      color: var(--danger);
    }

    .status-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
    }

    .status-badge.online .status-dot {
      background: var(--ok);
      box-shadow: 0 0 6px var(--ok);
    }

    .status-badge.offline .status-dot {
      background: var(--muted);
    }

    .status-badge.error .status-dot {
      background: var(--danger);
      box-shadow: 0 0 6px var(--danger);
    }

    /* Grid layout */
    .detail-grid {
      display: grid;
      grid-template-columns: 1fr 380px;
      gap: 24px;
    }

    @media (max-width: 1100px) {
      .detail-grid {
        grid-template-columns: 1fr;
      }
    }

    /* Card */
    .card {
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      overflow: hidden;
    }

    .card-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 16px 20px;
      border-bottom: 1px solid var(--border);
    }

    .card-title {
      font-size: 15px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0;
    }

    .card-body {
      padding: 20px;
    }

    /* Info grid */
    .info-grid {
      display: grid;
      grid-template-columns: repeat(2, 1fr);
      gap: 16px;
    }

    .info-item {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }

    .info-label {
      font-size: 12px;
      color: var(--muted);
      text-transform: uppercase;
      letter-spacing: 0.05em;
    }

    .info-value {
      font-size: 14px;
      color: var(--text-strong);
    }

    /* Properties table */
    .prop-table {
      width: 100%;
      border-collapse: collapse;
    }

    .prop-table th,
    .prop-table td {
      padding: 12px 16px;
      text-align: left;
      border-bottom: 1px solid var(--border);
    }

    .prop-table th {
      font-size: 12px;
      font-weight: 600;
      color: var(--muted);
      text-transform: uppercase;
      letter-spacing: 0.05em;
      background: var(--bg);
    }

    .prop-table td {
      font-size: 13px;
      color: var(--text);
    }

    .prop-table tr:last-child td {
      border-bottom: none;
    }

    .prop-name {
      font-weight: 500;
    }

    .prop-value {
      font-family: var(--mono);
    }

    .prop-badge {
      display: inline-block;
      padding: 2px 8px;
      border-radius: var(--radius-sm);
      font-size: 11px;
    }

    .prop-badge.readonly {
      background: var(--bg-muted);
      color: var(--muted);
    }

    .prop-badge.writable {
      background: var(--ok-subtle);
      color: var(--ok);
    }

    /* Commands */
    .command-list {
      display: flex;
      flex-direction: column;
      gap: 12px;
    }

    .command-item {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 12px 16px;
      background: var(--bg);
      border-radius: var(--radius-md);
    }

    .command-name {
      font-size: 14px;
      font-weight: 500;
      color: var(--text);
    }

    .command-btn {
      padding: 6px 12px;
      border: 1px solid var(--accent);
      border-radius: var(--radius-md);
      background: transparent;
      color: var(--accent);
      font-size: 12px;
      font-weight: 500;
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .command-btn:hover {
      background: var(--accent-subtle);
    }

    /* Events */
    .event-list {
      display: flex;
      flex-direction: column;
    }

    .event-item {
      display: flex;
      align-items: flex-start;
      gap: 12px;
      padding: 12px 0;
      border-bottom: 1px solid var(--border);
    }

    .event-item:last-child {
      border-bottom: none;
    }

    .event-level {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      margin-top: 6px;
      flex-shrink: 0;
    }

    .event-level.info { background: var(--info); }
    .event-level.warning { background: var(--warn); }
    .event-level.error { background: var(--danger); }
    .event-level.critical {
      background: var(--danger);
      box-shadow: 0 0 6px var(--danger);
    }

    .event-content {
      flex: 1;
    }

    .event-message {
      font-size: 13px;
      color: var(--text);
      margin: 0 0 4px;
    }

    .event-time {
      font-size: 12px;
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
  `

  @state() profile: DeviceProfile | null = null
  @state() loading = true
  @state() error: string | null = null
  @state() activeTab = 'properties'

  connectedCallback() {
    super.connectedCallback()
    const params = new URLSearchParams(window.location.search)
    const deviceId = params.get('id')
    if (deviceId) {
      this.loadDevice(deviceId)
    } else {
      this.error = '未指定设备ID'
      this.loading = false
    }
  }

  async loadDevice(deviceId: string) {
    this.loading = true
    this.error = null

    try {
      const response = await deviceApi.getDeviceProfile(deviceId)
      if (response.result) {
        this.profile = response.result
      }
    } catch (err: any) {
      this.error = err.message || '加载设备详情失败'
    } finally {
      this.loading = false
    }
  }

  async executeCommand(commandId: string) {
    if (!this.profile) return
    try {
      await deviceApi.executeCommand(this.profile.device.id, commandId, {})
      alert('命令已发送')
    } catch (err: any) {
      alert(err.message || '命令执行失败')
    }
  }

  render() {
    return html`
      <div class="page-header">
        <div class="header-left">
          <button class="back-btn" @click=${() => navigate('devices')}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M10.5 19.5L3 12m0 0l7.5-7.5M3 12h18"/>
            </svg>
          </button>
          <div>
            <h1 class="page-title">${this.profile?.device.name || '设备详情'}</h1>
            ${this.profile ? html`<div class="device-id">${this.profile.device.id}</div>` : ''}
          </div>
        </div>
        <div class="header-actions">
          <button class="btn btn-danger" @click=${() => this.deleteDevice()}>删除设备</button>
        </div>
      </div>

      ${this.loading ? this.renderLoading() : this.error ? this.renderError() : this.renderContent()}
    `
  }

  renderLoading() {
    return html`<div class="loading"><div class="spinner"></div></div>`
  }

  renderError() {
    return html`
      <div class="card">
        <div class="empty-state">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z"/>
          </svg>
          <p>${this.error}</p>
          <button class="btn" @click=${() => navigate('devices')}>返回设备列表</button>
        </div>
      </div>
    `
  }

  renderContent() {
    if (!this.profile) return null

    const { device, isOnline, properties, commands, recentEvents, overview } = this.profile

    return html`
      <!-- Status -->
      <div style="margin-bottom: 24px;">
        <span class="status-badge ${isOnline ? 'online' : 'offline'}">
          <span class="status-dot"></span>
          ${isOnline ? '在线' : '离线'}
        </span>
      </div>

      <div class="detail-grid">
        <!-- Left column -->
        <div>
          <!-- Properties -->
          <div class="card" style="margin-bottom: 24px;">
            <div class="card-header">
              <h3 class="card-title">属性 (${overview.totalProperties})</h3>
            </div>
            <div class="card-body" style="padding: 0;">
              ${properties.length > 0 ? html`
                <table class="prop-table">
                  <thead>
                    <tr>
                      <th>名称</th>
                      <th>值</th>
                      <th>类型</th>
                    </tr>
                  </thead>
                  <tbody>
                    ${properties.slice(0, 10).map(prop => html`
                      <tr>
                        <td class="prop-name">${prop.name}</td>
                        <td class="prop-value">${this.formatValue(prop.value)}</td>
                        <td>
                          <span class="prop-badge ${prop.readonly ? 'readonly' : 'writable'}">
                            ${prop.readonly ? '只读' : '可写'}
                          </span>
                        </td>
                      </tr>
                    `)}
                  </tbody>
                </table>
              ` : html`
                <div class="empty-state">
                  <p>暂无属性数据</p>
                </div>
              `}
            </div>
          </div>

          <!-- Commands -->
          <div class="card" style="margin-bottom: 24px;">
            <div class="card-header">
              <h3 class="card-title">命令 (${overview.totalCommands})</h3>
            </div>
            <div class="card-body">
              ${commands.length > 0 ? html`
                <div class="command-list">
                  ${commands.map(cmd => html`
                    <div class="command-item">
                      <span class="command-name">${cmd.name}</span>
                      <button class="command-btn" @click=${() => this.executeCommand(cmd.id)}>执行</button>
                    </div>
                  `)}
                </div>
              ` : html`
                <div class="empty-state">
                  <p>暂无命令</p>
                </div>
              `}
            </div>
          </div>

          <!-- Recent Events -->
          <div class="card">
            <div class="card-header">
              <h3 class="card-title">最近事件</h3>
            </div>
            <div class="card-body" style="padding: 0 20px;">
              ${recentEvents.length > 0 ? html`
                <div class="event-list">
                  ${recentEvents.map(event => html`
                    <div class="event-item">
                      <span class="event-level ${event.level}"></span>
                      <div class="event-content">
                        <p class="event-message">${event.message}</p>
                        <span class="event-time">${this.formatTime(event.timestamp)}</span>
                      </div>
                    </div>
                  `)}
                </div>
              ` : html`
                <div class="empty-state">
                  <p>暂无事件</p>
                </div>
              `}
            </div>
          </div>
        </div>

        <!-- Right column - Overview -->
        <div>
          <div class="card">
            <div class="card-header">
              <h3 class="card-title">设备信息</h3>
            </div>
            <div class="card-body">
              <div class="info-grid">
                <div class="info-item">
                  <span class="info-label">设备名称</span>
                  <span class="info-value">${device.name}</span>
                </div>
                <div class="info-item">
                  <span class="info-label">协议</span>
                  <span class="info-value">${device.protocol || '-'}</span>
                </div>
                <div class="info-item">
                  <span class="info-label">地址</span>
                  <span class="info-value">${device.address || '-'}</span>
                </div>
                <div class="info-item">
                  <span class="info-label">在线属性</span>
                  <span class="info-value">${overview.onlineProperties} / ${overview.totalProperties}</span>
                </div>
                <div class="info-item">
                  <span class="info-label">可写属性</span>
                  <span class="info-value">${overview.writableProperties}</span>
                </div>
                <div class="info-item">
                  <span class="info-label">最后更新</span>
                  <span class="info-value">${overview.updatedAt ? this.formatTime(overview.updatedAt) : '-'}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    `
  }

  formatValue(value: any): string {
    if (value === null || value === undefined) return '-'
    if (typeof value === 'object') return JSON.stringify(value)
    return String(value)
  }

  formatTime(timestamp?: string): string {
    if (!timestamp) return '-'
    const date = new Date(timestamp)
    return date.toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    })
  }

  async deleteDevice() {
    if (!this.profile) return
    if (!confirm(`确定要删除设备 "${this.profile.device.name}" 吗？此操作不可恢复。`)) return

    try {
      await deviceApi.deleteDevice(this.profile.device.id)
      navigate('devices')
    } catch (err: any) {
      alert(err.message || '删除失败')
    }
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'device-detail-page': DeviceDetailPage
  }
}
