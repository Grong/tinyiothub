import { LitElement, html} from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('a2ui-button')
export class A2uiButton extends LitElement {
  createRenderRoot() { return this }
  @property({ type: String }) label = ''
  @property({ type: String }) variant: 'primary' | 'secondary' | 'danger' = 'primary'
  @property({ type: Boolean }) disabled = false

  

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
