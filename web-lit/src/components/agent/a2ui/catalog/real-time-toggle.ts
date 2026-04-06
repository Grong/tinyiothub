import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import { hostStyles } from '../../../../styles/shared-host'

@customElement('real-time-toggle')
export class RealTimeToggle extends LitElement {
  @property({ type: Boolean }) enabled = false
  @property({ type: String }) label = '实时更新'
  @property({ type: String }) connectionStatus: 'connected' | 'connecting' | 'disconnected' = 'disconnected'

  static styles = [hostStyles, css`
    :host { display: inline-block; }
    .container {
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .toggle {
      width: 36px;
      height: 20px;
      border-radius: 10px;
      border: none;
      cursor: pointer;
      position: relative;
      transition: background 0.2s;
    }
    .toggle.on { background: var(--ok); }
    .toggle.off { background: var(--border); }
    .toggle::after {
      content: '';
      position: absolute;
      top: 2px;
      width: 16px;
      height: 16px;
      border-radius: 50%;
      background: var(--card);
      transition: left 0.2s;
    }
    .toggle.on::after { left: 18px; }
    .toggle.off::after { left: 2px; }
    .label { font-size: 0.8125rem; }
    .status-dot {
      width: 6px;
      height: 6px;
      border-radius: 50%;
    }
    .status-dot.connected { background: var(--ok); }
    .status-dot.connecting { background: var(--warn); }
    .status-dot.disconnected { background: var(--text-muted); }
  `]

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
