import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { deviceApi, type DeviceProfile, type DeviceCommand, type DeviceProperty, type CreateDeviceRequest } from '../services/devices'
import './command-execute-dialog'
import './property-chart-dialog'
import { driverApi, type Driver, type DriverConfigOption } from '../services/drivers'
import { navigate } from '../lib/navigate'

@customElement('device-detail-page')
export class DeviceDetailPage extends LitElement {
  static styles = css`
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
      box-shadow: var(--glass-shadow-sm);
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
      box-shadow: var(--glass-shadow-sm);
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      font-weight: 500;
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .btn:hover {
      background: var(--bg-hover);
    }

    .btn-danger {
      color: var(--danger);
    }

    .btn-danger:hover {
      background: var(--danger-subtle);
    }

    .btn-primary {
      background: var(--accent);
      color: var(--accent-foreground);
    }

    .btn-primary:hover {
      background: var(--accent-hover);
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
      box-shadow: var(--glass-shadow-sm);
      border-radius: var(--radius-lg);
      overflow: hidden;
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
      box-shadow: 0 1px 0 var(--card-highlight);
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
      box-shadow: none;
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
      box-shadow: var(--glass-shadow-sm);
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

    .chart-btn {
      width: 28px;
      height: 28px;
      display: flex;
      align-items: center;
      justify-content: center;
      border: none;
      border-radius: var(--radius-sm);
      background: transparent;
      color: var(--muted);
      cursor: pointer;
      margin-left: 8px;
    }
    .chart-btn:hover { background: var(--bg-hover); color: var(--accent); }

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
      box-shadow: 0 1px 0 var(--card-highlight);
    }

    .event-item:last-child {
      box-shadow: none;
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

    /* Modal overlay */
    .modal-overlay {
      position: fixed;
      inset: 0;
      z-index: 1000;
      background: rgba(0, 0, 0, 0.4);
      backdrop-filter: blur(4px);
      display: flex;
      align-items: center;
      justify-content: center;
      animation: fade-in 0.15s ease;
    }

    @keyframes fade-in {
      from { opacity: 0; }
      to { opacity: 1; }
    }

    .modal-card {
      background: var(--card);
      border-radius: var(--radius-lg);
      box-shadow: var(--glass-shadow);
      width: 100%;
      max-width: 560px;
      max-height: 90vh;
      overflow-y: auto;
      animation: rise 0.2s var(--ease-out);
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
      font-size: 18px;
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
      border-radius: var(--radius-md);
      background: none;
      color: var(--muted);
      cursor: pointer;
      transition: background var(--duration-fast) ease, color var(--duration-fast) ease;
    }

    .modal-close:hover {
      background: var(--bg-hover);
      color: var(--text);
    }

    .modal-close svg {
      width: 18px;
      height: 18px;
    }

    .modal-body {
      padding: 24px;
      display: flex;
      flex-direction: column;
      gap: 20px;
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
      gap: 8px;
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

    .form-input,
    .form-select,
    .form-textarea {
      width: 100%;
      padding: 10px 14px;
      border: none;
      box-shadow: var(--glass-shadow-sm);
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 14px;
      transition: box-shadow var(--duration-fast) ease;
    }

    .form-input:focus,
    .form-select:focus,
    .form-textarea:focus {
      outline: none;
      box-shadow: var(--focus-ring);
    }

    .form-input::placeholder,
    .form-textarea::placeholder {
      color: var(--muted);
    }

    .form-textarea {
      resize: vertical;
      min-height: 80px;
    }

    .form-hint {
      font-size: 12px;
      color: var(--muted);
    }

    .form-error {
      font-size: 12px;
      color: var(--danger);
    }

    .modal-error {
      padding: 12px 16px;
      border-radius: var(--radius-md);
      background: var(--danger-subtle);
      color: var(--danger);
      font-size: 13px;
    }

    .modal-btn {
      padding: 10px 20px;
      border-radius: var(--radius-md);
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .modal-btn-cancel {
      background: var(--bg-secondary);
      color: var(--text);
      box-shadow: var(--glass-shadow-sm);
    }

    .modal-btn-cancel:hover {
      background: var(--bg-hover);
    }

    .modal-btn-primary {
      background: var(--accent);
      color: var(--accent-foreground);
    }

    .modal-btn-primary:hover:not(:disabled) {
      background: var(--accent-hover);
    }

    .modal-btn-primary:disabled {
      opacity: 0.6;
      cursor: not-allowed;
    }
  `

  @state() profile: DeviceProfile | null = null
  @state() loading = true
  @state() error: string | null = null
  @state() activeTab = 'properties'

  // Auto-refresh and dialog state
  @state() refreshInterval: number | null = null
  @state() showCommandDialog = false
  @state() selectedCommand: DeviceCommand | null = null
  @state() showPropertyChart = false
  @state() selectedProperty: DeviceProperty | null = null

  // Edit modal state
  @state() showEditModal = false
  @state() editLoading = false
  @state() editError = ''
  @state() editName = ''
  @state() editDisplayName = ''
  @state() editDescription = ''
  @state() editProtocol = ''
  @state() editAddress = ''
  @state() editDriverName = ''
  @state() editDriverOptions: Record<string, string> = {}

  // Driver state
  @state() drivers: Driver[] = []
  @state() driverConfigOptions: DriverConfigOption[] = []

  connectedCallback() {
    super.connectedCallback()
    const params = new URLSearchParams(window.location.search)
    const deviceId = params.get('id')
    if (deviceId) {
      this.loadDevice(deviceId)
      // Auto-refresh every 3 seconds
      this.refreshInterval = window.setInterval(() => {
        if (this.deviceId) this.loadDevice(this.deviceId)
      }, 3000)
    } else {
      this.error = '未指定设备ID'
      this.loading = false
    }
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    if (this.refreshInterval) clearInterval(this.refreshInterval)
  }

  private get deviceId(): string {
    const params = new URLSearchParams(window.location.search)
    return params.get('id') || ''
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

  async openEditModal() {
    if (!this.profile) return
    const { device } = this.profile

    this.editName = device.name
    this.editDisplayName = device.displayName || ''
    this.editDescription = device.description || ''
    this.editProtocol = device.protocol || ''
    this.editAddress = device.address || ''
    this.editDriverName = device.driverName || ''
    this.editDriverOptions = {}
    this.editError = ''
    this.showEditModal = true

    // Load drivers list
    try {
      const response = await driverApi.getDrivers()
      if (response.result) {
        this.drivers = response.result
      }
    } catch {
      // Drivers list is optional
    }

    // Load driver config if device has a driver
    if (this.editDriverName) {
      await this.loadDriverConfig(this.editDriverName)
    }
  }

  async loadDriverConfig(driverName: string) {
    try {
      const response = await driverApi.getDriverConfig(driverName)
      this.driverConfigOptions = response.result || []
    } catch {
      this.driverConfigOptions = []
    }
  }

  private isNumericProperty(prop: DeviceProperty): boolean {
    return prop.dataType === 'number' || prop.dataType === 'integer' || prop.dataType === 'float'
  }

  async handleDriverChange(driverName: string) {
    this.editDriverName = driverName
    this.editDriverOptions = {}
    if (driverName) {
      await this.loadDriverConfig(driverName)
    } else {
      this.driverConfigOptions = []
    }
  }

  handleDriverOptionChange(optionName: string, value: string) {
    this.editDriverOptions = { ...this.editDriverOptions, [optionName]: value }
  }

  closeEditModal() {
    this.showEditModal = false
    this.editError = ''
  }

  async handleEditSubmit(e: Event) {
    e.preventDefault()
    if (!this.profile) return

    this.editError = ''

    // Validation
    if (!this.editName.trim()) {
      this.editError = '请输入设备名称'
      return
    }
    if (this.editName.length < 2 || this.editName.length > 50) {
      this.editError = '设备名称长度为2-50个字符'
      return
    }

    // Check required driver config options
    for (const opt of this.driverConfigOptions) {
      if (opt.required && (!this.editDriverOptions[opt.name] || !this.editDriverOptions[opt.name].trim())) {
        this.editError = `请填写必填项：${opt.label || opt.name}`
        return
      }
    }

    this.editLoading = true
    try {
      const data: Partial<CreateDeviceRequest> = {
        name: this.editName.trim(),
        displayName: this.editDisplayName.trim() || undefined,
        description: this.editDescription.trim() || undefined,
        protocol: this.editProtocol || undefined,
        address: this.editAddress.trim() || undefined,
        driverName: this.editDriverName || undefined,
        driverOptions: Object.keys(this.editDriverOptions).length > 0
          ? JSON.stringify(this.editDriverOptions)
          : undefined,
      }

      await deviceApi.updateDevice(this.profile.device.id, data)
      this.closeEditModal()
      await this.loadDevice(this.profile.device.id)
    } catch (err: any) {
      this.editError = err.message || '更新设备失败'
    } finally {
      this.editLoading = false
    }
  }

  renderEditModal() {
    if (!this.showEditModal) return null

    return html`
      <div class="modal-overlay" @click=${this.closeEditModal}>
        <div class="modal-card" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <h2 class="modal-title">编辑设备</h2>
            <button class="modal-close" @click=${this.closeEditModal}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
              </svg>
            </button>
          </div>
          <form @submit=${this.handleEditSubmit}>
            <div class="modal-body">
              ${this.editError ? html`<div class="modal-error">${this.editError}</div>` : ''}

              <div class="form-group">
                <label class="form-label">设备名称 <span class="required">*</span></label>
                <input
                  type="text"
                  class="form-input"
                  placeholder="请输入设备名称"
                  .value=${this.editName}
                  @input=${(e: InputEvent) => { this.editName = (e.target as HTMLInputElement).value }}
                />
                <span class="form-hint">2-50个字符</span>
              </div>

              <div class="form-group">
                <label class="form-label">显示名称</label>
                <input
                  type="text"
                  class="form-input"
                  placeholder="可选，用于显示"
                  .value=${this.editDisplayName}
                  @input=${(e: InputEvent) => { this.editDisplayName = (e.target as HTMLInputElement).value }}
                />
              </div>

              <div class="form-group">
                <label class="form-label">描述</label>
                <textarea
                  class="form-textarea"
                  placeholder="可选，设备描述"
                  .value=${this.editDescription}
                  @input=${(e: InputEvent) => { this.editDescription = (e.target as HTMLTextAreaElement).value }}
                ></textarea>
              </div>

              <div class="form-group">
                <label class="form-label">协议</label>
                <select
                  class="form-select"
                  .value=${this.editProtocol}
                  @change=${(e: Event) => { this.editProtocol = (e.target as HTMLSelectElement).value }}
                >
                  <option value="">请选择协议</option>
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
                  placeholder="例如 192.168.1.100:502"
                  .value=${this.editAddress}
                  @input=${(e: InputEvent) => { this.editAddress = (e.target as HTMLInputElement).value }}
                />
              </div>

              <div class="form-group">
                <label class="form-label">驱动</label>
                <select
                  class="form-select"
                  .value=${this.editDriverName}
                  @change=${(e: Event) => this.handleDriverChange((e.target as HTMLSelectElement).value)}
                >
                  <option value="">请选择驱动</option>
                  ${this.drivers.map(d => html`
                    <option value=${d.name}>${d.name}${d.version ? ` (${d.version})` : ''}</option>
                  `)}
                </select>
              </div>

              ${this.driverConfigOptions.length > 0 ? html`
                <div style="padding: 12px; background: var(--bg); border-radius: var(--radius-md);">
                  <label class="form-label" style="margin-bottom: 12px;">驱动配置</label>
                  ${this.driverConfigOptions.map(opt => html`
                    <div class="form-group" style="margin-bottom: 12px;">
                      <label class="form-label">
                        ${opt.label || opt.name}
                        ${opt.required ? html`<span class="required">*</span>` : ''}
                      </label>
                      ${this.renderDriverOptionInput(opt)}
                      ${opt.description ? html`<span class="form-hint">${opt.description}</span>` : ''}
                    </div>
                  `)}
                </div>
              ` : ''}
            </div>
            <div class="modal-footer">
              <button type="button" class="modal-btn modal-btn-cancel" @click=${this.closeEditModal}>取消</button>
              <button type="submit" class="modal-btn modal-btn-primary" ?disabled=${this.editLoading}>
                ${this.editLoading ? '保存中...' : '保存'}
              </button>
            </div>
          </form>
        </div>
      </div>
    `
  }

  renderDriverOptionInput(opt: DriverConfigOption) {
    const value = this.editDriverOptions[opt.name] ?? opt.defaultValue ?? ''

    if (opt.type === 'select') {
      return html`
        <select
          class="form-select"
          .value=${value}
          @change=${(e: Event) => this.handleDriverOptionChange(opt.name, (e.target as HTMLSelectElement).value)}
        >
          <option value="">请选择</option>
          ${opt.defaultValue?.split(',').map(v => html`
            <option value=${v.trim()}>${v.trim()}</option>
          `)}
        </select>
      `
    }

    if (opt.type === 'number') {
      return html`
        <input
          type="number"
          class="form-input"
          placeholder=${opt.label || opt.name}
          .value=${value}
          @input=${(e: InputEvent) => this.handleDriverOptionChange(opt.name, (e.target as HTMLInputElement).value)}
        />
      `
    }

    // Default: string input
    return html`
      <input
        type="text"
        class="form-input"
        placeholder=${opt.label || opt.name}
        .value=${value}
        @input=${(e: InputEvent) => this.handleDriverOptionChange(opt.name, (e.target as HTMLInputElement).value)}
      />
    `
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
          <button class="btn" @click=${() => this.loadDevice(this.deviceId)}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M23 4v6h-6M1 20v-6h6"/>
              <path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/>
            </svg>
            刷新
          </button>
          <button class="btn btn-primary" @click=${() => this.openEditModal()}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10"/>
            </svg>
            编辑
          </button>
          <button class="btn btn-danger" @click=${() => this.deleteDevice()}>删除设备</button>
        </div>
      </div>

      ${this.loading ? this.renderLoading() : this.error ? this.renderError() : this.renderContent()}
      ${this.renderEditModal()}
      ${this.showCommandDialog ? html`
        <command-execute-dialog
          .open=${this.showCommandDialog}
          .command=${this.selectedCommand}
          .deviceId=${this.deviceId}
          @close=${() => this.showCommandDialog = false}
          @success=${() => this.loadDevice(this.deviceId)}
        ></command-execute-dialog>
      ` : ''}
      ${this.showPropertyChart ? html`
        <property-chart-dialog
          .open=${this.showPropertyChart}
          .property=${this.selectedProperty}
          .deviceId=${this.deviceId}
          @close=${() => this.showPropertyChart = false}
        ></property-chart-dialog>
      ` : ''}
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
                        <td class="prop-value">
                          ${this.formatValue(prop.value)}
                          ${this.isNumericProperty(prop) ? html`
                            <button class="chart-btn" @click=${() => { this.selectedProperty = prop; this.showPropertyChart = true }} title="查看曲线">
                              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                <path d="M3 3v18h18"/>
                                <path d="M18 17l-5-5-3 3-4-4"/>
                              </svg>
                            </button>
                          ` : ''}
                        </td>
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
                      <button class="command-btn" @click=${() => { this.selectedCommand = cmd; this.showCommandDialog = true }}>执行</button>
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
