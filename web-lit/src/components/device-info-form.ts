// web-lit/src/components/device-info-form.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { driverApi, type Driver, type DriverConfigOption } from '../services/drivers'
import type { ProcessedDeviceTemplate } from '../services/templates'

@customElement('device-info-form')
export class DeviceInfoForm extends LitElement {
  static styles = css`
    :host { display: block; }
    .form-group { margin-bottom: 16px; }
    .form-label {
      display: block;
      font-size: 13px;
      font-weight: 500;
      color: var(--text);
      margin-bottom: 6px;
    }
    .form-label .required { color: var(--danger); margin-left: 2px; }
    .form-input, .form-select, .form-textarea {
      width: 100%;
      padding: 10px 12px;
      background: var(--card);
      border: none;
      border-bottom: 1px solid var(--input);
      border-radius: var(--radius-md) var(--radius-md) 0 0;
      color: var(--text);
      font-size: 14px;
    }
    .form-input:focus, .form-select:focus, .form-textarea:focus {
      outline: none;
      border-bottom-color: var(--accent);
    }
    .form-textarea { resize: vertical; min-height: 80px; }
    .form-error {
      font-size: 12px;
      color: var(--danger);
      margin-top: 4px;
    }
    .form-hint {
      font-size: 12px;
      color: var(--muted);
      margin-top: 4px;
    }
  `

  @property({ type: Object }) template!: ProcessedDeviceTemplate
  @property({ type: String }) value = ''
  @state() drivers: Driver[] = []
  @state() driverConfig: DriverConfigOption[] = []
  @state() errors: Record<string, string> = {}

  private get _formData() {
    try { return JSON.parse(this.value) } catch { return {} }
  }

  async connectedCallback() {
    super.connectedCallback()
    await this.loadDrivers()
  }

  async loadDrivers() {
    try {
      const res = await driverApi.getDrivers()
      if (res.result) this.drivers = res.result
    } catch { this.drivers = [] }
  }

  async loadDriverConfig(driverName: string) {
    if (!driverName) { this.driverConfig = []; return }
    try {
      const res = await driverApi.getDriverConfig(driverName)
      if (res.result) this.driverConfig = res.result
    } catch { this.driverConfig = [] }
  }

  private handleInput(field: string, value: string) {
    const data = { ...this._formData, [field]: value }
    if (field === 'driverName') {
      this.loadDriverConfig(value)
      data.driverOptions = '{}'
    }
    this.value = JSON.stringify(data)
    this.dispatchEvent(new CustomEvent('change', { detail: data }))
  }

  private handleDriverOption(name: string, optValue: string) {
    const data = this._formData
    const opts = JSON.parse(data.driverOptions || '{}')
    opts[name] = optValue
    data.driverOptions = JSON.stringify(opts)
    this.value = JSON.stringify(data)
    this.dispatchEvent(new CustomEvent('change', { detail: data }))
  }

  render() {
    const d = this._formData
    return html`
      <div class="form-group">
        <label class="form-label">设备名称 <span class="required">*</span></label>
        <input type="text" class="form-input" .value=${d.name || ''} @input=${(e: InputEvent) => this.handleInput('name', (e.target as HTMLInputElement).value)} />
        ${this.errors.name ? html`<span class="form-error">${this.errors.name}</span>` : ''}
      </div>

      <div class="form-group">
        <label class="form-label">描述</label>
        <textarea class="form-textarea" .value=${d.description || ''} @input=${(e: InputEvent) => this.handleInput('description', (e.target as HTMLTextAreaElement).value)}></textarea>
      </div>

      <div class="form-group">
        <label class="form-label">设备地址</label>
        <input type="text" class="form-input" .value=${d.address || ''} @input=${(e: InputEvent) => this.handleInput('address', (e.target as HTMLInputElement).value)} />
      </div>

      <div class="form-group">
        <label class="form-label">安装位置</label>
        <input type="text" class="form-input" .value=${d.position || ''} @input=${(e: InputEvent) => this.handleInput('position', (e.target as HTMLInputElement).value)} />
      </div>

      <div class="form-group">
        <label class="form-label">驱动</label>
        <select class="form-select" .value=${d.driverName || ''} @change=${(e: Event) => this.handleInput('driverName', (e.target as HTMLSelectElement).value)}>
          <option value="">选择驱动</option>
          ${this.drivers.map(dr => html`<option value=${dr.name}>${dr.name}</option>`)}
        </select>
      </div>

      ${this.driverConfig.length > 0 ? html`
        <div class="form-group">
          <label class="form-label">驱动配置</label>
          ${this.driverConfig.map(opt => html`
            <div style="margin-bottom: 12px;">
              <label class="form-label">
                ${opt.label} ${opt.required ? html`<span class="required">*</span>` : ''}
              </label>
              ${opt.type === 'boolean' ? html`
                <select class="form-select" @change=${(e: Event) => this.handleDriverOption(opt.name, (e.target as HTMLSelectElement).value)}>
                  <option value="true">是</option>
                  <option value="false">否</option>
                </select>
              ` : html`
                <input type=${opt.type === 'number' ? 'number' : 'text'} class="form-input"
                  .value=${JSON.parse(d.driverOptions || '{}')[opt.name] || opt.defaultValue || ''}
                  @input=${(e: InputEvent) => this.handleDriverOption(opt.name, (e.target as HTMLInputElement).value)} />
              `}
              ${opt.description ? html`<span class="form-hint">${opt.description}</span>` : ''}
            </div>
          `)}
        </div>
      ` : ''}
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'device-info-form': DeviceInfoForm }
}
