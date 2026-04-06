import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import { hostStyles } from '../../../../../styles/shared-host'

@customElement('a2ui-card')
export class A2uiCard extends LitElement {
  @property({ type: String }) title = ''

  static styles = [hostStyles, css`
    :host { display: block; }
    .card {
      background: var(--card);
      border-radius: var(--radius);
      padding: 16px;
      box-shadow: var(--glass-shadow-sm);
    }
    .card-title {
      font-size: 0.875rem;
      font-weight: 600;
      margin-bottom: 8px;
      color: var(--text);
    }
  `]

  render() {
    return html`
      <div class="card">
        ${this.title ? html`<div class="card-title">${this.title}</div>` : ''}
        <slot></slot>
      </div>
    `
  }
}
