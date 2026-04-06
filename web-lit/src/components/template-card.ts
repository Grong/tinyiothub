// web-lit/src/components/template-card.ts
import { LitElement, html} from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { ProcessedDeviceTemplate } from '../services/templates'

@customElement('template-card')
export class TemplateCard extends LitElement {
  createRenderRoot() { return this }
  

  @property({ type: Object }) template!: ProcessedDeviceTemplate
  @property({ type: Function }) onUse!: (t: ProcessedDeviceTemplate) => void

  private getCategoryIcon(): string {
    const icons: Record<string, string> = {
      sensors: '🌡️',
      controllers: '🎛️',
      cameras: '📷',
      gateways: '🌐',
      default: '📦',
    }
    return icons[this.template.category] || icons.default
  }

  private handleClick() {
    this.onUse(this.template)
  }

  render() {
    const t = this.template
    return html`
      <div class="card" @click=${this.handleClick}>
        <div class="card-header">
          <div class="category-icon">${this.getCategoryIcon()}</div>
          <div>
            <h3 class="template-name">${typeof t.displayName === 'object' ? t.displayName['zh'] || t.displayName['en'] || t.name : t.displayName || t.name}</h3>
            <div class="template-meta">${t.manufacturer || ''} ${t.deviceType || ''}</div>
          </div>
        </div>
        ${t.description ? html`<p class="template-description">${typeof t.description === 'object' ? Object.values(t.description)[0] : t.description}</p>` : ''}
        <div class="template-tags">
          ${t.driverName ? html`<span class="tag">${t.driverName}</span>` : ''}
          ${t.protocolType ? html`<span class="tag">${t.protocolType}</span>` : ''}
          ${t.version ? html`<span class="tag">v${t.version}</span>` : ''}
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'template-card': TemplateCard }
}
