// web-lit/src/components/create-device-wizard.ts
import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { templateApi, transformDeviceTemplate, type ProcessedDeviceTemplate } from '../services/templates'
import { deviceApi } from '../services/devices'
import './template-card'
import './template-preview'
import './device-info-form'

type WizardStep = 'template' | 'device'

@customElement('create-device-wizard')
export class CreateDeviceWizard extends LitElement {
  static styles = css`
    :host { display: block; }
    .overlay {
      position: fixed;
      inset: 0;
      z-index: 1000;
      background: rgba(0, 0, 0, 0.6);
      backdrop-filter: blur(4px);
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .modal {
      background: var(--bg);
      width: 95vw;
      max-width: 1200px;
      height: 85vh;
      border-radius: var(--radius-lg);
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }
    .header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 20px 24px;
      border-bottom: 1px solid var(--border);
    }
    .header h2 {
      font-size: 18px;
      font-weight: 600;
      margin: 0;
    }
    .close-btn {
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
    }
    .close-btn:hover { background: var(--bg-hover); }
    .body { flex: 1; display: flex; overflow: hidden; }
    .step-indicator {
      display: flex;
      gap: 8px;
      margin-left: 24px;
    }
    .step-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--muted);
    }
    .step-dot.active { background: var(--accent); }
    .content { flex: 1; overflow-y: auto; padding: 24px; }
    .search-bar {
      max-width: 400px;
      margin-bottom: 24px;
    }
    .search-input {
      width: 100%;
      padding: 10px 14px;
      background: var(--card);
      border: none;
      border-radius: var(--radius-md);
      color: var(--text);
      font-size: 14px;
    }
    .category-tabs {
      display: flex;
      gap: 8px;
      margin-bottom: 24px;
    }
    .category-tab {
      padding: 6px 12px;
      border-radius: var(--radius-md);
      font-size: 13px;
      background: var(--card);
      color: var(--muted);
      cursor: pointer;
      border: none;
    }
    .category-tab.active {
      background: var(--accent);
      color: white;
    }
    .template-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
      gap: 16px;
    }
    .device-step {
      display: flex;
      height: 100%;
    }
    .form-area {
      flex: 1;
      padding: 24px;
      overflow-y: auto;
    }
    .preview-area {
      width: 400px;
      border-left: 1px solid var(--border);
      background: var(--card);
    }
    .footer {
      display: flex;
      justify-content: flex-end;
      gap: 12px;
      padding: 16px 24px;
      border-top: 1px solid var(--border);
    }
    .btn {
      padding: 10px 20px;
      border-radius: var(--radius-md);
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      border: none;
    }
    .btn-secondary { background: var(--bg-secondary); color: var(--text); }
    .btn-primary { background: var(--accent); color: white; }
    .btn-primary:disabled { opacity: 0.6; cursor: not-allowed; }
    .toast {
      position: fixed;
      bottom: 24px;
      left: 50%;
      transform: translateX(-50%);
      padding: 12px 24px;
      background: var(--card);
      border-radius: var(--radius-md);
      box-shadow: var(--shadow-lg);
      z-index: 2000;
    }
    .toast.success { border-left: 4px solid var(--ok); }
    .toast.error { border-left: 4px solid var(--danger); }
  `

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
      await deviceApi.createDevice({
        name: data.name,
        displayName: data.name,
        description: data.description,
        address: data.address,
        position: data.position,
        driverName: data.driverName,
        driverOptions: Object.keys(driverOptions).length > 0 ? JSON.stringify(driverOptions) : undefined,
        propertyValues: {},
        enabledCommands: this.selectedTemplate?.commands?.map(c => c.name) || [],
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
            <div style="display: flex; align-items: center; gap: 16px;">
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
