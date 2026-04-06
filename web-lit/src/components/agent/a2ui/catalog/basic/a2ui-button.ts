import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import { hostStyles } from '../../../../../styles/shared-host'

@customElement('a2ui-button')
export class A2uiButton extends LitElement {
  @property({ type: String }) label = ''
  @property({ type: String }) variant: 'primary' | 'secondary' | 'danger' = 'primary'
  @property({ type: Boolean }) disabled = false

  static styles = [hostStyles, css`
    :host { display: inline-block; }
    button {
      padding: 6px 16px;
      border-radius: var(--radius);
      font-size: 0.8125rem;
      font-weight: 500;
      cursor: pointer;
      border: 1px solid transparent;
      transition: background 0.15s, border-color 0.15s;
    }
    button.primary {
      background: var(--accent);
      color: #fff;
    }
    button.primary:hover { background: var(--accent-hover); }
    button.secondary {
      background: transparent;
      color: var(--text);
      box-shadow: var(--glass-shadow-sm);
    }
    button.danger {
      background: var(--danger);
      color: #fff;
    }
    button:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  `]

  private _handleClick() {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'click', label: this.label },
      bubbles: true,
      composed: true,
    }))
  }

  render() {
    return html`
      <button
        class="${this.variant}"
        ?disabled="${this.disabled}"
        @click="${this._handleClick}"
      >${this.label}</button>
    `
  }
}
