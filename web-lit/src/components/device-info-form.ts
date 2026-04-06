// web-lit/src/components/device-info-form.ts
import { LitElement, html} from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { driverApi, type Driver, type DriverConfigOption } from '../services/drivers'
import type { ProcessedDeviceTemplate } from '../services/templates'
import { isFieldRequired } from '../services/templates'

@customElement('device-info-form')
export class DeviceInfoForm extends LitElement {
  createRenderRoot() { return this }
  

  @property({ type: Object }) template!: ProcessedDeviceTemplate
  @property({ type: String }) value = ''
  @state() drivers: Driver[] = []
  @state() driverConfig: DriverConfigOption[] = []
  @state() loadingConfig = false
  @state() errors: Record<string, string> = {}

  // Track the driver that was last used to initialize config
  private _lastInitDriver = ''
  private _loadConfigGeneration = 0

  private get _formData() {
    try { return JSON.parse(this.value) } catch { return {} }
  }

  async connectedCallback() {
    super.connectedCallback()
    await this.loadDrivers()
  }

  updated(changedProperties: Map<string, unknown>) {
    // Template changed — initialize form from template defaults
    if (changedProperties.has('template') && this.template) {
      this._initFromTemplate()
    }

    // Value (form data) changed — check if driver selection changed
    if (changedProperties.has('value')) {
      const d = this._formData
      const currentDriver = d.driverName || ''

      // Driver was set to something new (not the one we initialized for)
      if (currentDriver && currentDriver !== this._lastInitDriver) {
        this._lastInitDriver = currentDriver
        this.loadDriverConfig(currentDriver)
      }
    }
  }

  private async _initFromTemplate() {
    if (!this.template) return

    // Ensure drivers are loaded first
    if (this.drivers.length === 0) {
      await this.loadDrivers()
    }

    const d = this._formData
    const data: Record<string, any> = { ...d }

    // Pre-fill from template defaults
    if (!data.name) {
      const pattern = this.template.deviceInfo?.defaultNamePattern
      data.name = pattern ? pattern.replace('{name}', this.template.name) : this.template.name
    }
    if (!data.description) {
      const desc = this.template.deviceInfo?.defaultDescription
      data.description = typeof desc === 'object' ? Object.values(desc as Record<string, string>)[0] || '' : ''
    }
    if (isFieldRequired(this.template.deviceInfo, 'address') && !data.address) {
      data.address = ''
    }
    if (!data.position) {
      data.position = this.template.deviceInfo?.defaultPosition || ''
    }

    const templateDriver = this.template.driverName

    // Auto-select template's driver and load its config
    if (templateDriver) {
      data.driverName = templateDriver
      this.value = JSON.stringify(data)
      this._lastInitDriver = templateDriver
      await this.loadDriverConfig(templateDriver)

      // Initialize driver options with defaults from template
      const opts: Record<string, string> = {}
      for (const opt of this.driverConfig) {
        if (opt.defaultValue) opts[opt.name] = opt.defaultValue
      }
      data.driverOptions = JSON.stringify(opts)
    }

    this.value = JSON.stringify(data)
    this.dispatchEvent(new CustomEvent('change', { detail: data }))
  }

  async loadDrivers() {
    try {
      const res = await driverApi.getDrivers()
      if (Array.isArray(res.result)) this.drivers = res.result
    } catch { this.drivers = [] }
  }

  async loadDriverConfig(driverName: string) {
    if (!driverName) { this.driverConfig = []; return }
    this.loadingConfig = true
    const generation = ++this._loadConfigGeneration
    try {
      const res = await driverApi.getDriverConfig(driverName)
      // Only apply result if no newer request has been made
      if (generation === this._loadConfigGeneration && Array.isArray(res.result)) {
        this.driverConfig = res.result
      }
    } catch {
      if (generation === this._loadConfigGeneration) this.driverConfig = []
    }
    finally {
      if (generation === this._loadConfigGeneration) this.loadingConfig = false
    }
  }

  private _handleInput(field: string, rawValue: string) {
    const data: Record<string, any> = { ...this._formData, [field]: rawValue }

    if (field === 'driverName') {
      // Reset driver options when driver changes
      data.driverOptions = '{}'
      this._lastInitDriver = rawValue // prevent _initFromTemplate override
      this.loadDriverConfig(rawValue)
    }

    this.value = JSON.stringify(data)
    this.dispatchEvent(new CustomEvent('change', { detail: data }))
  }

  private _handleDriverOption(name: string, optValue: string) {
    const data = this._formData
    const opts: Record<string, string> = JSON.parse(data.driverOptions || '{}')
    opts[name] = optValue
    data.driverOptions = JSON.stringify(opts)
    this.value = JSON.stringify(data)
    this.dispatchEvent(new CustomEvent('change', { detail: data }))
  }

  render() {
    const d = this._formData
    const driverOpts: Record<string, string> = JSON.parse(d.driverOptions || '{}')

    return html`
      <!-- Section: Basic Info -->
      <div class="form-section-title">基本信息</div>

      <div class="form-group">
        <label class="form-label">设备名称 <span class="required">*</span></label>
        <input type="text" class="form-input"
          .value=${d.name || ''}
          @input=${(e: InputEvent) => this._handleInput('name', (e.target as HTMLInputElement).value)}
        />
      </div>

      <div class="form-group">
        <label class="form-label">描述</label>
        <textarea class="form-textarea"
          .value=${d.description || ''}
          @input=${(e: InputEvent) => this._handleInput('description', (e.target as HTMLTextAreaElement).value)}
        ></textarea>
      </div>

      <div class="form-group">
        <label class="form-label">
          设备地址
          ${isFieldRequired(this.template?.deviceInfo, 'address') ? html`<span class="required">*</span>` : ''}
        </label>
        <input type="text" class="form-input"
          .value=${d.address || ''}
          placeholder="如 192.168.1.100:502"
          @input=${(e: InputEvent) => this._handleInput('address', (e.target as HTMLInputElement).value)}
        />
      </div>

      <div class="form-group">
        <label class="form-label">安装位置</label>
        <input type="text" class="form-input"
          .value=${d.position || ''}
          @input=${(e: InputEvent) => this._handleInput('position', (e.target as HTMLInputElement).value)}
        />
      </div>

      <!-- Section: Driver -->
      <div class="form-section-title">驱动配置</div>

      <div class="form-group">
        <label class="form-label">驱动</label>
        <select class="form-select"
          .value=${d.driverName || ''}
          @change=${(e: Event) => this._handleInput('driverName', (e.target as HTMLSelectElement).value)}
        >
          <option value="">选择驱动</option>
          ${this.drivers.map(dr => html`
            <option value=${dr.name}>
              ${dr.name}${dr.version ? ` (${dr.version})` : ''}${dr.description ? ` - ${dr.description}` : ''}
            </option>
          `)}
        </select>
      </div>

      <!-- Driver config -->
      ${!d.driverName ? '' : this.loadingConfig ? html`
        <div class="loading">加载驱动配置...</div>
      ` : this.driverConfig.length > 0 ? html`
        ${this.driverConfig.map(opt => html`
          <div class="form-group">
            <label class="form-label">
              ${opt.label || opt.name}
              ${opt.required ? html`<span class="required">*</span>` : ''}
            </label>
            ${opt.type === 'boolean' ? html`
              <select class="form-select"
                .value=${driverOpts[opt.name] || opt.defaultValue || 'false'}
                @change=${(e: Event) => this._handleDriverOption(opt.name, (e.target as HTMLSelectElement).value)}
              >
                <option value="true">是</option>
                <option value="false">否</option>
              </select>
            ` : html`
              <input type=${opt.type === 'number' ? 'number' : 'text'}
                class="form-input"
                .value=${driverOpts[opt.name] || opt.defaultValue || ''}
                placeholder=${opt.defaultValue ? `默认: ${opt.defaultValue}` : ''}
                @input=${(e: InputEvent) => this._handleDriverOption(opt.name, (e.target as HTMLInputElement).value)}
              />
            `}
            ${opt.description ? html`<span class="form-hint">${opt.description}</span>` : ''}
          </div>
        `)}
      ` : html`
        <div class="loading">该驱动无需额外配置</div>
      `}
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'device-info-form': DeviceInfoForm }
}
