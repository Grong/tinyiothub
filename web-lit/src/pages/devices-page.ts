import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { deviceApi, type Device, type DeviceListParams } from '../services/devices'
import { navigate } from '../lib/navigate'

@customElement('devices-page')
export class DevicesPage extends LitElement {
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

    .btn-primary {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 10px 16px;
      background: var(--accent);
      color: var(--accent-foreground);
      border: none;
      border-radius: var(--radius-md);
      font-size: 13px;
      font-weight: 600;
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .btn-primary:hover {
      background: var(--accent-hover);
    }

    /* Filters */
    .filters {
      display: flex;
      gap: 12px;
      margin-bottom: 20px;
      flex-wrap: wrap;
    }

    .search-input {
      flex: 1;
      min-width: 200px;
      padding: 10px 14px;
      border: 1px solid var(--input);
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
    }

    .search-input:focus {
      outline: none;
      border-color: var(--accent);
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

    /* Device list */
    .device-list {
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      overflow: hidden;
    }

    .device-list-header {
      display: grid;
      grid-template-columns: 1fr 120px 120px 100px 80px;
      gap: 16px;
      padding: 12px 20px;
      background: var(--bg);
      border-bottom: 1px solid var(--border);
      font-size: 12px;
      font-weight: 600;
      color: var(--muted);
      text-transform: uppercase;
      letter-spacing: 0.05em;
    }

    .device-item {
      display: grid;
      grid-template-columns: 1fr 120px 120px 100px 80px;
      gap: 16px;
      padding: 16px 20px;
      border-bottom: 1px solid var(--border);
      align-items: center;
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .device-item:last-child {
      border-bottom: none;
    }

    .device-item:hover {
      background: var(--bg-hover);
    }

    .device-info {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }

    .device-name {
      font-size: 14px;
      font-weight: 500;
      color: var(--text-strong);
    }

    .device-id {
      font-size: 12px;
      color: var(--muted);
      font-family: var(--mono);
    }

    .device-protocol {
      font-size: 13px;
      color: var(--text);
    }

    .device-address {
      font-size: 13px;
      color: var(--muted);
      font-family: var(--mono);
    }

    .device-status {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      font-size: 12px;
      font-weight: 500;
    }

    .status-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
    }

    .status-dot.online {
      background: var(--ok);
      box-shadow: 0 0 6px var(--ok);
    }

    .status-dot.offline {
      background: var(--muted);
    }

    .status-dot.error {
      background: var(--danger);
      box-shadow: 0 0 6px var(--danger);
    }

    .device-actions {
      display: flex;
      gap: 8px;
    }

    .action-btn {
      width: 32px;
      height: 32px;
      display: flex;
      align-items: center;
      justify-content: center;
      border: none;
      border-radius: var(--radius-md);
      background: transparent;
      color: var(--muted);
      cursor: pointer;
      transition: background var(--duration-fast) ease, color var(--duration-fast) ease;
    }

    .action-btn:hover {
      background: var(--bg-hover);
      color: var(--text);
    }

    .action-btn.danger:hover {
      background: var(--danger-subtle);
      color: var(--danger);
    }

    .action-btn svg {
      width: 16px;
      height: 16px;
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
      margin: 0 0 24px;
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

  @state() devices: Device[] = []
  @state() loading = true
  @state() error: string | null = null
  @state() search = ''
  @state() protocol = ''
  @state() status = ''
  @state() page = 1
  @state() pageSize = 10
  @state() totalCount = 0

  async connectedCallback() {
    super.connectedCallback()
    await this.loadDevices()
  }

  async loadDevices() {
    this.loading = true
    this.error = null

    try {
      const params: DeviceListParams = {
        page: this.page,
        pageSize: this.pageSize,
        search: this.search || undefined,
        protocol: this.protocol || undefined,
        status: this.status || undefined,
      }

      const response = await deviceApi.getDevices(params)

      if (response.result) {
        this.devices = response.result.data || []
        this.totalCount = response.result.pagination?.totalCount || 0
      }
    } catch (err: any) {
      this.error = err.message || '加载设备失败'
    } finally {
      this.loading = false
    }
  }

  handleSearch(e: Event) {
    this.search = (e.target as HTMLInputElement).value
    this.page = 1
    this.loadDevices()
  }

  handleProtocolChange(e: Event) {
    this.protocol = (e.target as HTMLSelectElement).value
    this.page = 1
    this.loadDevices()
  }

  handleStatusChange(e: Event) {
    this.status = (e.target as HTMLSelectElement).value
    this.page = 1
    this.loadDevices()
  }

  handlePageChange(newPage: number) {
    this.page = newPage
    this.loadDevices()
  }

  viewDevice(device: Device) {
    navigate(`device-detail?id=${device.id}`)
  }

  async deleteDevice(device: Device, e: Event) {
    e.stopPropagation()
    if (!confirm(`确定要删除设备 "${device.name}" 吗？`)) return

    try {
      await deviceApi.deleteDevice(device.id)
      await this.loadDevices()
    } catch (err: any) {
      alert(err.message || '删除失败')
    }
  }

  render() {
    return html`
      <div class="page-header">
        <h1 class="page-title">设备管理</h1>
        <div class="header-actions">
          <button class="btn-primary" @click=${() => navigate('devices?action=create')}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15"/>
            </svg>
            添加设备
          </button>
        </div>
      </div>

      <div class="filters">
        <input
          type="text"
          class="search-input"
          placeholder="搜索设备名称或ID..."
          .value=${this.search}
          @input=${this.handleSearch}
        />
        <select class="filter-select" .value=${this.protocol} @change=${this.handleProtocolChange}>
          <option value="">全部协议</option>
          <option value="modbus">Modbus</option>
          <option value="onvif">ONVIF</option>
          <option value="snmp">SNMP</option>
          <option value="mqtt">MQTT</option>
        </select>
        <select class="filter-select" .value=${this.status} @change=${this.handleStatusChange}>
          <option value="">全部状态</option>
          <option value="online">在线</option>
          <option value="offline">离线</option>
          <option value="error">错误</option>
        </select>
      </div>

      ${this.loading ? this.renderLoading() : this.error ? this.renderError() : this.renderDeviceList()}
    `
  }

  renderLoading() {
    return html`
      <div class="device-list">
        <div class="loading">
          <div class="spinner"></div>
        </div>
      </div>
    `
  }

  renderError() {
    return html`
      <div class="device-list">
        <div class="empty-state">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z"/>
          </svg>
          <h3>加载失败</h3>
          <p>${this.error}</p>
          <button class="btn-primary" @click=${() => this.loadDevices()}>重试</button>
        </div>
      </div>
    `
  }

  renderDeviceList() {
    if (this.devices.length === 0) {
      return html`
        <div class="device-list">
          <div class="empty-state">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 17.25v1.007a3 3 0 01-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0115 18.257V17.25m6-12V15a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 15V5.25m18 0A2.25 2.25 0 0018.75 3H5.25A2.25 2.25 0 003 5.25m18 0V12a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 12V5.25"/>
            </svg>
            <h3>暂无设备</h3>
            <p>点击"添加设备"创建一个新设备</p>
            <button class="btn-primary" @click=${() => navigate('devices?action=create')}>添加设备</button>
          </div>
        </div>
      `
    }

    const totalPages = Math.ceil(this.totalCount / this.pageSize)

    return html`
      <div class="device-list">
        <div class="device-list-header">
          <span>设备</span>
          <span>协议</span>
          <span>地址</span>
          <span>状态</span>
          <span>操作</span>
        </div>

        ${this.devices.map(device => html`
          <div class="device-item" @click=${() => this.viewDevice(device)}>
            <div class="device-info">
              <span class="device-name">${device.name}</span>
              <span class="device-id">${device.id}</span>
            </div>
            <span class="device-protocol">${device.protocol || '-'}</span>
            <span class="device-address">${device.address || '-'}</span>
            <span class="device-status">
              <span class="status-dot ${device.status || 'offline'}"></span>
              ${device.status === 'online' ? '在线' : device.status === 'offline' ? '离线' : device.status || '未知'}
            </span>
            <div class="device-actions" @click=${(e: Event) => e.stopPropagation()}>
              <button class="action-btn" title="查看详情" @click=${() => this.viewDevice(device)}>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M2.036 12.322a1.012 1.012 0 010-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178z"/>
                  <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                </svg>
              </button>
              <button class="action-btn danger" title="删除" @click=${(e: Event) => this.deleteDevice(device, e)}>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0"/>
                </svg>
              </button>
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
}

declare global {
  interface HTMLElementTagNameMap {
    'devices-page': DevicesPage
  }
}
