import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { deviceApi, type Device, type DeviceListParams, type CreateDeviceRequest } from '../services/devices'
import { driverApi, type Driver, type DriverConfigOption } from '../services/drivers'
import { $currentWorkspaceId } from '../stores/workspace-store'
import '../components/device-card'
import '../components/tag-filter'
import '../components/create-device-wizard'
import { hostStyles } from '../styles/shared-host'

@customElement('devices-page')
export class DevicesPage extends LitElement {
  static styles = [hostStyles, css`
    :host {
      display: flex;
      flex-direction: column;
      padding: 0;
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

    .btn-primary:disabled {
      opacity: 0.6;
      cursor: not-allowed;
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
      border: none;
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      box-shadow: var(--glass-shadow-sm);
    }

    .search-input:focus {
      outline: none;
      box-shadow: var(--focus-ring);
    }

    .filter-select {
      padding: 10px 14px;
      border: none;
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      cursor: pointer;
      box-shadow: var(--glass-shadow-sm);
      transition: background var(--duration-fast) ease, box-shadow var(--duration-fast) ease;
    }

    .filter-select:hover {
      background: var(--bg-hover);
    }

    /* Device list */
    .device-list {
      background: var(--card);
      box-shadow: var(--glass-shadow-sm);
      border-radius: var(--radius-lg);
      overflow: hidden;
    }

    /* Device grid — fluid auto-fill like templates */
    .device-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
      gap: 16px;
    }

    .empty-card {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      min-height: 160px;
      background: var(--card);
      border-radius: var(--radius-lg);
      border: 2px dashed var(--border);
      cursor: pointer;
      color: var(--muted);
      transition: border-color 0.15s ease, color 0.15s ease;
    }

    .empty-card:hover {
      border-color: var(--accent);
      color: var(--accent);
    }

    .empty-card svg { margin-bottom: 8px; opacity: 0.5; }
    .empty-card p { margin: 0; font-size: 14px; }
    .empty-card span { font-size: 12px; }

    /* View toggle */
    .view-toggle {
      width: 36px;
      height: 36px;
      display: flex;
      align-items: center;
      justify-content: center;
      border: none;
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--muted);
      cursor: pointer;
    }

    .view-toggle:hover { background: var(--bg-hover); color: var(--text); }

    .device-list-header {
      display: grid;
      grid-template-columns: 1fr 120px 120px 100px 100px;
      gap: 16px;
      padding: 12px 20px;
      background: var(--bg);
      box-shadow: 0 1px 0 var(--card-highlight);
      font-size: 12px;
      font-weight: 600;
      color: var(--muted);
      text-transform: uppercase;
      letter-spacing: 0.05em;
    }

    .device-item {
      display: grid;
      grid-template-columns: 1fr 120px 120px 100px 100px;
      gap: 16px;
      padding: 16px 20px;
      box-shadow: 0 1px 0 var(--card-highlight);
      align-items: center;
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .device-item:last-of-type {
      box-shadow: none;
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
      box-shadow: 0 -1px 0 var(--card-highlight);
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
      border: none;
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      cursor: pointer;
      box-shadow: var(--glass-shadow-sm);
      transition: background var(--duration-fast) ease;
    }

    .page-btn:hover:not(:disabled) {
      background: var(--bg-hover);
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

    /* Modal overlay */
    .modal-overlay {
      position: fixed;
      inset: 0;
      z-index: 200;
      background: rgba(0, 0, 0, 0.4);
      backdrop-filter: blur(4px);
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 24px;
      animation: fade-in 0.2s ease;
    }

    @keyframes fade-in {
      from { opacity: 0; }
      to { opacity: 1; }
    }

    .modal {
      background: var(--card);
      border-radius: var(--radius-lg);
      box-shadow: var(--glass-shadow);
      width: 100%;
      max-width: 520px;
      max-height: 85vh;
      display: flex;
      flex-direction: column;
      animation: rise 0.25s var(--ease-out);
    }

    @keyframes rise {
      from { opacity: 0; transform: translateY(12px); }
      to { opacity: 1; transform: translateY(0); }
    }

    .modal-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 20px 24px;
      box-shadow: 0 1px 0 var(--card-highlight);
    }

    .modal-title {
      font-size: 16px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0;
    }

    .modal-close {
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
      transition: background var(--duration-fast) ease;
    }

    .modal-close:hover {
      background: var(--bg-hover);
    }

    .modal-close svg {
      width: 18px;
      height: 18px;
    }

    .modal-body {
      padding: 24px;
      overflow-y: auto;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }

    .modal-footer {
      display: flex;
      justify-content: flex-end;
      gap: 12px;
      padding: 16px 24px;
      box-shadow: 0 -1px 0 var(--card-highlight);
    }

    /* Form */
    .form-group {
      display: flex;
      flex-direction: column;
      gap: 6px;
    }

    .form-label {
      font-size: 13px;
      font-weight: 500;
      color: var(--text);
    }

    .form-label .required {
      color: var(--danger);
      margin-left: 2px;
    }

    .form-input {
      padding: 10px 14px;
      border: none;
      border-bottom: 1px solid var(--input);
      border-radius: var(--radius-md) var(--radius-md) 0 0;
      background: var(--card);
      color: var(--text);
      font-size: 14px;
      transition: border-color var(--duration-fast) ease, box-shadow var(--duration-fast) ease;
    }

    .form-input:focus {
      outline: none;
      border-bottom-color: var(--accent);
      box-shadow: var(--focus-ring);
    }

    .form-input::placeholder {
      color: var(--muted);
    }

    .form-hint {
      font-size: 12px;
      color: var(--muted);
    }

    .form-error {
      font-size: 12px;
      color: var(--danger);
    }

    .form-textarea {
      padding: 10px 14px;
      border: none;
      border-bottom: 1px solid var(--input);
      border-radius: var(--radius-md) var(--radius-md) 0 0;
      background: var(--card);
      color: var(--text);
      font-size: 14px;
      font-family: inherit;
      resize: vertical;
      min-height: 60px;
      transition: border-color var(--duration-fast) ease, box-shadow var(--duration-fast) ease;
    }

    .form-textarea:focus {
      outline: none;
      border-bottom-color: var(--accent);
      box-shadow: var(--focus-ring);
    }

    .form-select {
      padding: 10px 14px;
      border: none;
      border-bottom: 1px solid var(--input);
      border-radius: var(--radius-md) var(--radius-md) 0 0;
      background: var(--card);
      color: var(--text);
      font-size: 14px;
      cursor: pointer;
      transition: border-color var(--duration-fast) ease, box-shadow var(--duration-fast) ease;
    }

    .form-select:focus {
      outline: none;
      border-bottom-color: var(--accent);
      box-shadow: var(--focus-ring);
    }

    .btn-secondary {
      padding: 10px 16px;
      border: none;
      border-radius: var(--radius-md);
      background: var(--bg-secondary);
      color: var(--text);
      font-size: 13px;
      font-weight: 500;
      cursor: pointer;
      box-shadow: var(--glass-shadow-sm);
      transition: background var(--duration-fast) ease;
    }

    .btn-secondary:hover {
      background: var(--bg-hover);
    }

    .alert-error {
      padding: 12px 16px;
      border-radius: var(--radius-md);
      background: var(--danger-subtle);
      color: var(--danger);
      font-size: 13px;
    }
  `]

  @state() devices: Device[] = []
  @state() loading = true
  @state() error: string | null = null
  @state() search = ''
  @state() protocol = ''
  @state() status = ''
  @state() page = 1
  @state() pageSize = 10
  @state() totalCount = 0
  @state() viewMode: 'grid' | 'table' = 'grid'
  @state() tagId = ''

  // Modal state
  @state() wizardRef: any = null
  @state() showModal = false
  @state() isEditMode = false
  @state() editingDeviceId = ''
  @state() formLoading = false
  @state() formError = ''

  // Form fields
  @state() formName = ''
  @state() formDisplayName = ''
  @state() formDescription = ''
  @state() formProtocol = ''
  @state() formAddress = ''
  @state() formDriverName = ''
  @state() formDriverOptions: Record<string, string> = {}

  // Driver state
  @state() drivers: Driver[] = []
  @state() driverConfigOptions: DriverConfigOption[] = []

  private _workspaceUnsub?: () => void

  async connectedCallback() {
    super.connectedCallback()
    await this.loadDevices()
    this._workspaceUnsub = $currentWorkspaceId.subscribe(() => { this.loadDevices() })
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    this._workspaceUnsub?.()
  }

  firstUpdated() {
    this.wizardRef = this.querySelector('create-device-wizard')
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
        tagIds: this.tagId ? [this.tagId] : undefined,
      }

      const response = await deviceApi.getDevices(params)

      if (response.result) {
        // API returns direct array, not PaginatedResponse {data, pagination}
        if (Array.isArray(response.result)) {
          this.devices = response.result
          this.totalCount = response.result.length
        } else {
          this.devices = response.result.data || []
          this.totalCount = response.result.pagination?.totalCount || 0
        }
      }
    } catch (err: any) {
      this.error = err.message || '加载设备失败'
    } finally {
      this.loading = false
    }
  }

  async loadDrivers() {
    try {
      const response = await driverApi.getDrivers()
      if (Array.isArray(response.result)) {
        this.drivers = response.result
      }
    } catch {
      this.drivers = []
    }
  }

  async loadDriverConfig(driverName: string) {
    if (!driverName) {
      this.driverConfigOptions = []
      return
    }
    try {
      const response = await driverApi.getDriverConfig(driverName)
      if (response.result) {
        this.driverConfigOptions = response.result
        // Initialize default values
        const defaults: Record<string, string> = {}
        for (const opt of response.result) {
          if (opt.defaultValue) {
            defaults[opt.name] = opt.defaultValue
          }
        }
        this.formDriverOptions = { ...defaults, ...this.formDriverOptions }
      }
    } catch {
      this.driverConfigOptions = []
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

  // Modal actions
  openCreateModal() {
    this.wizardRef?.show()
  }

  openEditModal(device: Device, e?: Event) {
    if (e) e.stopPropagation()
    this.isEditMode = true
    this.editingDeviceId = device.id
    this.formName = device.name
    this.formDisplayName = device.displayName || ''
    this.formDescription = device.description || ''
    this.formProtocol = device.protocol || ''
    this.formAddress = device.address || ''
    this.formDriverName = device.driverName || ''
    this.formDriverOptions = {}
    this.driverConfigOptions = []
    this.formError = ''
    this.showModal = true
    this.loadDrivers()
    if (device.driverName) {
      this.loadDriverConfig(device.driverName)
    }
  }

  closeModal() {
    this.showModal = false
    this.formError = ''
  }

  handleDriverChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value
    this.formDriverName = value
    this.formDriverOptions = {}
    this.loadDriverConfig(value)
  }

  handleDriverOptionChange(optionName: string, value: string) {
    this.formDriverOptions = { ...this.formDriverOptions, [optionName]: value }
  }

  async handleSubmit(e: Event) {
    e.preventDefault()
    this.formError = ''

    // Validation
    if (!this.formName.trim()) {
      this.formError = '请输入设备名称'
      return
    }
    if (this.formName.trim().length < 2) {
      this.formError = '设备名称至少2个字符'
      return
    }
    if (this.formName.trim().length > 50) {
      this.formError = '设备名称最多50个字符'
      return
    }

    // Validate required driver config options
    for (const opt of this.driverConfigOptions) {
      if (opt.required && !this.formDriverOptions[opt.name]?.trim()) {
        this.formError = `请填写必填配置项: ${opt.label || opt.name}`
        return
      }
    }

    this.formLoading = true

    const data: CreateDeviceRequest = {
      name: this.formName.trim(),
      displayName: this.formDisplayName.trim() || undefined,
      description: this.formDescription.trim() || undefined,
      protocol: this.formProtocol || undefined,
      address: this.formAddress.trim() || undefined,
      driverName: this.formDriverName || undefined,
      driverOptions: Object.keys(this.formDriverOptions).length > 0
        ? JSON.stringify(this.formDriverOptions)
        : undefined,
    }

    try {
      if (this.isEditMode) {
        await deviceApi.updateDevice(this.editingDeviceId, data)
      } else {
        await deviceApi.createDevice(data)
      }
      this.showModal = false
      await this.loadDevices()
    } catch (err: any) {
      this.formError = err.message || (this.isEditMode ? '更新失败' : '创建失败')
    } finally {
      this.formLoading = false
    }
  }

  viewDevice(device: Device) {
    window.history.pushState({}, '', `/device-detail?id=${device.id}`)
    window.dispatchEvent(new PopStateEvent('popstate'))
  }

  async deleteDevice(device: Device) {
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
          <button class="view-toggle" @click=${() => { this.viewMode = this.viewMode === 'grid' ? 'table' : 'grid' }}>
            ${this.viewMode === 'grid' ? html`
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M3 10h18M3 14h18M3 6h18M3 18h18"/>
              </svg>
            ` : html`
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/>
                <rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/>
              </svg>
            `}
          </button>
          <button class="btn-primary" @click=${() => this.openCreateModal()}>
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
          <option value="maintenance">维护</option>
        </select>
        <tag-filter
          .value=${this.tagId}
          @change=${(e: CustomEvent) => { this.tagId = e.detail; this.page = 1; this.loadDevices() }}
        ></tag-filter>
      </div>

      ${this.loading ? this.renderLoading() : this.error ? this.renderError() : this.viewMode === 'grid' ? this.renderDeviceGrid() : this.renderDeviceList()}
      <create-device-wizard
        id="wizard"
        @success=${() => this.loadDevices()}
      ></create-device-wizard>
      ${this.showModal ? this.renderModal() : ''}
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
            <button class="btn-primary" @click=${() => this.openCreateModal()}>添加设备</button>
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
              <button class="action-btn" title="编辑" @click=${(e: Event) => this.openEditModal(device, e)}>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10"/>
                </svg>
              </button>
              <button class="action-btn danger" title="删除" @click=${() => this.deleteDevice(device)}>
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

  renderDeviceGrid() {
    if (this.devices.length === 0) {
      return this.renderEmptyGrid()
    }
    return html`
      <div class="device-grid">
        ${this.devices.map(device => html`
          <device-card
            .device=${device}
            .onEdit=${(d: Device) => this.openEditModal(d)}
            .onDelete=${(d: Device) => this.deleteDevice(d)}
          ></device-card>
        `)}
      </div>
    `
  }

  renderEmptyGrid() {
    return html`
      <div class="device-grid">
        <div class="empty-card" @click=${() => this.openCreateModal()}>
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15"/>
          </svg>
          <p>暂无设备</p>
          <span>点击添加设备</span>
        </div>
      </div>
    `
  }

  renderModal() {
    const title = this.isEditMode ? '编辑设备' : '添加设备'

    return html`
      <div class="modal-overlay" @click=${() => this.closeModal()}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <h2 class="modal-title">${title}</h2>
            <button class="modal-close" @click=${() => this.closeModal()}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
              </svg>
            </button>
          </div>

          <form @submit=${this.handleSubmit}>
            <div class="modal-body">
              ${this.formError ? html`<div class="alert-error">${this.formError}</div>` : ''}

              <div class="form-group">
                <label class="form-label">设备名称 <span class="required">*</span></label>
                <input
                  type="text"
                  class="form-input"
                  placeholder="请输入设备名称（2-50字符）"
                  .value=${this.formName}
                  @input=${(e: InputEvent) => { this.formName = (e.target as HTMLInputElement).value }}
                />
              </div>

              <div class="form-group">
                <label class="form-label">显示名称</label>
                <input
                  type="text"
                  class="form-input"
                  placeholder="可选，友好显示名称"
                  .value=${this.formDisplayName}
                  @input=${(e: InputEvent) => { this.formDisplayName = (e.target as HTMLInputElement).value }}
                />
              </div>

              <div class="form-group">
                <label class="form-label">描述</label>
                <textarea
                  class="form-textarea"
                  placeholder="设备描述..."
                  .value=${this.formDescription}
                  @input=${(e: InputEvent) => { this.formDescription = (e.target as HTMLTextAreaElement).value }}
                ></textarea>
              </div>

              <div class="form-group">
                <label class="form-label">协议</label>
                <select
                  class="form-select"
                  .value=${this.formProtocol}
                  @change=${(e: Event) => { this.formProtocol = (e.target as HTMLSelectElement).value }}
                >
                  <option value="">选择协议</option>
                  <option value="modbus">Modbus</option>
                  <option value="onvif">ONVIF</option>
                  <option value="snmp">SNMP</option>
                  <option value="mqtt">MQTT</option>
                </select>
              </div>

              <div class="form-group">
                <label class="form-label">地址</label>
                <input
                  type="text"
                  class="form-input"
                  placeholder="如 192.168.1.100:502"
                  .value=${this.formAddress}
                  @input=${(e: InputEvent) => { this.formAddress = (e.target as HTMLInputElement).value }}
                />
              </div>

              <div class="form-group">
                <label class="form-label">驱动</label>
                <select
                  class="form-select"
                  .value=${this.formDriverName}
                  @change=${this.handleDriverChange}
                >
                  <option value="">选择驱动</option>
                  ${this.drivers.map(d => html`
                    <option value=${d.name}>${d.name}${d.version ? ` (${d.version})` : ''}</option>
                  `)}
                </select>
              </div>

              ${this.driverConfigOptions.length > 0 ? html`
                ${this.driverConfigOptions.map(opt => html`
                  <div class="form-group">
                    <label class="form-label">
                      ${opt.label || opt.name}
                      ${opt.required ? html`<span class="required">*</span>` : ''}
                    </label>
                    ${opt.type === 'boolean' ? html`
                      <select
                        class="form-select"
                        .value=${this.formDriverOptions[opt.name] ?? opt.defaultValue ?? ''}
                        @change=${(e: Event) => this.handleDriverOptionChange(opt.name, (e.target as HTMLSelectElement).value)}
                      >
                        <option value="true">是</option>
                        <option value="false">否</option>
                      </select>
                    ` : html`
                      <input
                        type=${opt.type === 'number' ? 'number' : 'text'}
                        class="form-input"
                        placeholder=${opt.defaultValue || ''}
                        .value=${this.formDriverOptions[opt.name] ?? ''}
                        @input=${(e: InputEvent) => this.handleDriverOptionChange(opt.name, (e.target as HTMLInputElement).value)}
                      />
                    `}
                    ${opt.description ? html`<span class="form-hint">${opt.description}</span>` : ''}
                  </div>
                `)}
              ` : ''}
            </div>

            <div class="modal-footer">
              <button type="button" class="btn-secondary" @click=${() => this.closeModal()}>取消</button>
              <button type="submit" class="btn-primary" ?disabled=${this.formLoading}>
                ${this.formLoading ? (this.isEditMode ? '保存中...' : '创建中...') : (this.isEditMode ? '保存' : '创建')}
              </button>
            </div>
          </form>
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
