import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('a2ui-card')
export class A2uiCard extends LitElement {
  @property({ type: String }) title = ''

  static styles = css`
    :host { display: block; }
    .card {
      background: var(--card, #fff);
      border: 1px solid var(--border, #e2e8f0);
      border-radius: var(--radius, 8px);
      padding: 16px;
      box-shadow: 0 1px 3px rgba(0,0,0,0.08);
    }
    .card-title {
      font-size: 0.875rem;
      font-weight: 600;
      margin-bottom: 8px;
      color: var(--text, #1a1a1a);
    }
  `

  render() {
    return html`
      <div class="card">
        ${this.title ? html`<div class="card-title">${this.title}</div>` : ''}
        <slot></slot>
      </div>
    `
  }
}
