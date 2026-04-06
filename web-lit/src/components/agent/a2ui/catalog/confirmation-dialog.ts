import { LitElement, html} from 'lit'
import { customElement, property, state } from 'lit/decorators.js'

@customElement('confirmation-dialog')
export class ConfirmationDialog extends LitElement {
  createRenderRoot() { return this }
  @property({ type: String }) title = '确认'
  @property({ type: String }) message = ''
  @property({ type: String }) confirmLabel = '确认'
  @property({ type: String }) cancelLabel = '取消'
  @property({ type: String }) severity: 'info' | 'warning' | 'danger' = 'info'
  @property({ type: Number }) timeout = 0
  @state() private remaining = 0

  private _timer: number | null = null

  

  connectedCallback() {
    super.connectedCallback()
    if (this.timeout > 0) {
      this.remaining = this.timeout
      this._timer = window.setInterval(() => {
        this.remaining--
        if (this.remaining <= 0) {
          this._handleTimeout()
        }
      }, 1000)
    }
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    if (this._timer) clearInterval(this._timer)
  }

  private _handleConfirm() {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'confirm' },
      bubbles: true, composed: true,
    }))
  }

  private _handleCancel() {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'cancel' },
      bubbles: true, composed: true,
    }))
  }

  private _handleTimeout() {
    if (this._timer) clearInterval(this._timer)
    this._timer = null
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'timeout' },
      bubbles: true, composed: true,
    }))
  }

  render() {
    return html`
      <div class="overlay" @click="${this._handleCancel}">
        <div class="dialog" @click="${(e: Event) => e.stopPropagation()}">
          <div class="title">${this.title}</div>
          <div class="message">${this.message}</div>
          <div class="buttons">
            <button @click="${this._handleCancel}">${this.cancelLabel}</button>
            <button
              class="confirm ${this.severity}"
              @click="${this._handleConfirm}"
            >${this.confirmLabel}${this.timeout > 0 ? ` (${this.remaining}s)` : ''}</button>
          </div>
        </div>
      </div>
    `
  }
}
