// web-lit/src/components/template-card.ts
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { ProcessedDeviceTemplate } from '../services/templates'
import { hostStyles } from '../styles/shared-host'

@customElement('template-card')
export class TemplateCard extends LitElement {
  static styles = [hostStyles, css`
    :host { display: block; cursor: pointer; }
    .card {
      background: var(--card);
      border-radius: var(--radius-lg);
      padding: 16px;
      border: 2px solid transparent;
      transition: border-color 0.15s ease, transform 0.15s ease;
    }
    .card:hover {
      border-color: var(--accent);
      transform: translateY(-2px);
    }
    .card-header {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 8px;
    }
    .category-icon {
      width: 40px;
      height: 40px;
      display: flex;
      align-items: center;
      justify-content: center;
      border-radius: var(--radius-md);
      font-size: 20px;
      background: var(--bg-muted);
    }
    .template-name {
      font-size: 14px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0;
    }
    .template-meta {
      font-size: 12px;
      color: var(--muted);
    }
    .template-description {
      font-size: 13px;
      color: var(--text);
      margin: 8px 0;
      display: -webkit-box;
      -webkit-line-clamp: 2;
      -webkit-box-orient: vertical;
      overflow: hidden;
    }
    .template-tags {
      display: flex;
      flex-wrap: wrap;
      gap: 4px;
      margin-top: 8px;
    }
    .tag {
      font-size: 10px;
      padding: 2px 6px;
      border-radius: var(--radius-sm);
      background: var(--bg-muted);
      color: var(--muted);
    }
  `]

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
