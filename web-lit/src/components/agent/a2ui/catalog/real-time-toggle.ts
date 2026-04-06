import { LitElement, html} from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('real-time-toggle')
export class RealTimeToggle extends LitElement {
  createRenderRoot() { return this }
  @property({ type: Boolean }) enabled = false
  @property({ type: String }) label = '实时更新'
  @property({ type: String }) connectionStatus: 'connected' | 'connecting' | 'disconnected' = 'disconnected'

  

  private _handleToggle() {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'toggle', value: !this.enabled },
      bubbles: true, composed: true,
    }))
  }

  render() {
    return html`
      <div class="container">
        <button
          class="toggle ${this.enabled ? 'on' : 'off'}"
          @click="${this._handleToggle}"
        ></button>
        <span class="label">${this.label}</span>
        <span class="status-dot ${this.connectionStatus}"></span>
      </div>
    `
  }
}
