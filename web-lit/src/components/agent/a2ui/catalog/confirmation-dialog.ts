import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { hostStyles } from '../../../../styles/shared-host'

@customElement('confirmation-dialog')
export class ConfirmationDialog extends LitElement {
  @property({ type: String }) title = '确认'
  @property({ type: String }) message = ''
  @property({ type: String }) confirmLabel = '确认'
  @property({ type: String }) cancelLabel = '取消'
  @property({ type: String }) severity: 'info' | 'warning' | 'danger' = 'info'
  @property({ type: Number }) timeout = 0
  @state() private remaining = 0

  private _timer: number | null = null

  static styles = [hostStyles, css`
    :host { display: block; }
    .overlay {
      position: fixed;
      inset: 0;
      background: rgba(0,0,0,0.4);
      display: flex;
      align-items: center;
      justify-content: center;
      z-index: 1000;
    }
    .dialog {
      background: var(--card);
      border-radius: var(--radius);
      padding: 20px;
      min-width: 300px;
      max-width: 400px;
      box-shadow: var(--glass-shadow-sm);
    }
    .title { font-size: 1rem; font-weight: 600; margin-bottom: 8px; }
    .message { font-size: 0.875rem; color: var(--text-muted); margin-bottom: 16px; }
    .buttons { display: flex; gap: 8px; justify-content: flex-end; }
    button {
      padding: 6px 16px;
      border-radius: 6px;
      box-shadow: var(--glass-shadow-sm);
      background: transparent;
      cursor: pointer;
      font-size: 0.8125rem;
    }
    button.confirm { color: var(--text-on-accent); box-shadow: none; }
    button.confirm.info { background: var(--accent); }
    button.confirm.warning { background: var(--warn); }
    button.confirm.danger { background: var(--danger); }
  `]

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
