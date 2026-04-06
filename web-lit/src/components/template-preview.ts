// web-lit/src/components/template-preview.ts
import { LitElement, html} from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import type { ProcessedDeviceTemplate } from '../services/templates'

@customElement('template-preview')
export class TemplatePreview extends LitElement {
  createRenderRoot() { return this }
  

  @property({ type: Object }) template!: ProcessedDeviceTemplate
  @state() activeTab: 'properties' | 'commands' = 'properties'

  render() {
    return html`
      <div class="tabs">
        <div class="tab ${this.activeTab === 'properties' ? 'active' : ''}" @click=${() => this.activeTab = 'properties'}>
          属性 (${this.template.properties?.length || 0})
        </div>
        <div class="tab ${this.activeTab === 'commands' ? 'active' : ''}" @click=${() => this.activeTab = 'commands'}>
          命令 (${this.template.commands?.length || 0})
        </div>
      </div>
      <div class="content">
        ${this.activeTab === 'properties' ? this.renderProperties() : this.renderCommands()}
      </div>
    `
  }

  private renderProperties() {
    const props = this.template.properties || []
    if (props.length === 0) {
      return html`<div class="empty">此模板没有定义属性</div>`
    }
    return props.map(p => html`
      <div class="property-item">
        <span class="property-name">${typeof p.displayName === 'object' ? p.displayName['zh'] || p.name : p.displayName || p.name}</span>
        <div class="property-meta">
          <span class="badge">${p.dataType}</span>
          ${p.unit ? html`<span class="badge">${p.unit}</span>` : ''}
          <span class="badge ${p.isReadOnly ? 'readonly' : 'writable'}">${p.isReadOnly ? '只读' : '可写'}</span>
        </div>
      </div>
    `)
  }

  private renderCommands() {
    const cmds = this.template.commands || []
    if (cmds.length === 0) {
      return html`<div class="empty">此模板没有定义命令</div>`
    }
    return cmds.map(c => html`
      <div class="command-item">
        <span class="command-name">${typeof c.displayName === 'object' ? c.displayName['zh'] || c.name : c.displayName || c.name}</span>
        <div class="command-meta">
          ${c.isRequired ? html`<span class="badge readonly">必需</span>` : ''}
        </div>
      </div>
    `)
  }
}

declare global {
  interface HTMLElementTagNameMap { 'template-preview': TemplatePreview }
}
