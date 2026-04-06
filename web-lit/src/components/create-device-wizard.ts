// web-lit/src/components/create-device-wizard.ts
import { LitElement, html} from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { templateApi, transformDeviceTemplate, type ProcessedDeviceTemplate, type DeviceCreationInput } from '../services/templates'
import './template-card'
import './template-preview'
import './device-info-form'

type WizardStep = 'template' | 'device'

@customElement('create-device-wizard')
export class CreateDeviceWizard extends LitElement {
  createRenderRoot() { return this }
  

  @state() open = false
  @state() step: WizardStep = 'template'
  @state() templates: ProcessedDeviceTemplate[] = []
  @state() filteredTemplates: ProcessedDeviceTemplate[] = []
  @state() selectedTemplate: ProcessedDeviceTemplate | null = null
  @state() searchQuery = ''
  @state() category = ''
  @state() formData = '{}'
  @state() loading = false
  @state() creating = false
  @state() toast = ''

  async show() {
    this.open = true
    this.step = 'template'
    this.selectedTemplate = null
    this.formData = '{}'
    await this.loadTemplates()
  }

  hide() {
    this.open = false
  }

  async loadTemplates() {
    this.loading = true
    try {
      const res = await templateApi.getTemplates()
      if (res.result) {
        this.templates = res.result.map(transformDeviceTemplate)
        this.filterTemplates()
      }
    } finally {
      this.loading = false
    }
  }

  filterTemplates() {
    let filtered = this.templates
    if (this.searchQuery) {
      const q = this.searchQuery.toLowerCase()
      filtered = filtered.filter(t =>
        t.name.toLowerCase().includes(q) ||
        (t.displayName as any)?.['zh']?.toLowerCase().includes(q)
      )
    }
    if (this.category) {
      filtered = filtered.filter(t => t.category === this.category)
    }
    this.filteredTemplates = filtered
  }

  selectTemplate(t: ProcessedDeviceTemplate) {
    this.selectedTemplate = t
    this.step = 'device'
  }

  handleFormChange(e: CustomEvent) {
    this.formData = JSON.stringify(e.detail)
  }

  async handleCreate() {
    this.creating = true
    try {
      const data = JSON.parse(this.formData)
      const driverOptions = data.driverOptions ? JSON.parse(data.driverOptions) : {}

      const deviceInput: DeviceCreationInput = {
        name: data.name,
        displayName: data.name,
        description: data.description || undefined,
        address: data.address || undefined,
        position: data.position || undefined,
        driverName: data.driverName || this.selectedTemplate?.driverName || undefined,
        driverOptions: Object.keys(driverOptions).length > 0 ? JSON.stringify(driverOptions) : undefined,
        propertyValues: {},
        enabledCommands: this.selectedTemplate?.commands?.map(c => c.name) || [],
      }

      await templateApi.createDeviceFromTemplate({
        templateId: this.selectedTemplate!.id,
        deviceInput,
      })

      this.showToast('设备创建成功', 'success')
      this.hide()
      this.dispatchEvent(new CustomEvent('success'))
    } catch (err: any) {
      this.showToast(err.message || '创建失败', 'error')
    } finally {
      this.creating = false
    }
  }

  showToast(message: string, type: 'success' | 'error') {
    this.toast = `${type}:${message}`
    setTimeout(() => { this.toast = '' }, 3000)
  }

  render() {
    if (!this.open) return html``
    return html`
      <div class="overlay" @click=${() => this.hide()}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="header">
            <div class="header-left">
              <h2>创建设备</h2>
              <div class="step-indicator">
                <div class="step-dot ${this.step === 'template' ? 'active' : ''}"></div>
                <div class="step-dot ${this.step === 'device' ? 'active' : ''}"></div>
              </div>
            </div>
            <button class="close-btn" @click=${() => this.hide()}>
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 6L6 18M6 6l12 12"/>
              </svg>
            </button>
          </div>

          <div class="body">
            ${this.step === 'template' ? this.renderTemplateStep() : this.renderDeviceStep()}
          </div>

          ${this.step === 'device' ? html`
            <div class="footer">
              <button class="btn btn-secondary" @click=${() => this.step = 'template'}>上一步</button>
              <button class="btn btn-primary" ?disabled=${this.creating} @click=${this.handleCreate}>
                ${this.creating ? '创建中...' : '创建'}
              </button>
            </div>
          ` : ''}
        </div>
      </div>
      ${this.toast ? html`<div class="toast ${this.toast.split(':')[0]}">${this.toast.split(':')[1]}</div>` : ''}
    `
  }

  private renderTemplateStep() {
    const categories = ['', 'sensors', 'controllers', 'cameras', 'gateways', 'others']
    const labels: Record<string, string> = { '': '全部', sensors: '传感器', controllers: '控制器', cameras: '摄像头', gateways: '网关', others: '其他' }
    return html`
      <div class="content">
        <div class="search-bar">
          <input type="text" class="search-input" placeholder="搜索模板..."
            .value=${this.searchQuery} @input=${(e: InputEvent) => { this.searchQuery = (e.target as HTMLInputElement).value; this.filterTemplates() }} />
        </div>
        <div class="category-tabs">
          ${categories.map(c => html`
            <button class="category-tab ${this.category === c ? 'active' : ''}" @click=${() => { this.category = c; this.filterTemplates() }}>
              ${labels[c]}
            </button>
          `)}
        </div>
        <div class="template-grid">
          ${this.filteredTemplates.map(t => html`
            <template-card .template=${t} .onUse=${(tmpl: ProcessedDeviceTemplate) => this.selectTemplate(tmpl)}></template-card>
          `)}
        </div>
      </div>
    `
  }

  private renderDeviceStep() {
    if (!this.selectedTemplate) return html``
    return html`
      <div class="device-step">
        <div class="form-area">
          <device-info-form
            .template=${this.selectedTemplate}
            .value=${this.formData}
            @change=${this.handleFormChange}
          ></device-info-form>
        </div>
        <div class="preview-area">
          <template-preview .template=${this.selectedTemplate}></template-preview>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'create-device-wizard': CreateDeviceWizard }
}
