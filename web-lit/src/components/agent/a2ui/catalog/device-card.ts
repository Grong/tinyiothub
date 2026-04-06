import { LitElement, html} from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { DeviceProperty } from '../../../../types/agent-types'

@customElement('a2ui-device-card')
export class DeviceCard extends LitElement {
  createRenderRoot() { return this }
  @property({ type: String }) deviceId = ''
  @property({ type: String }) deviceName = ''
  @property({ type: String }) status: 'online' | 'offline' | 'warning' | 'error' = 'offline'
  @property({ type: String }) deviceType = 'generic'
  @property({ type: String }) protocol = ''
  @property({ type: String }) lastSeen = ''
  @property({ type: Array }) properties: DeviceProperty[] = []
  @property({ type: Boolean }) showActions = true
  @property({ type: Boolean }) compact = false

  

  private _formatTime(iso: string): string {
    if (!iso) return '-'
    try {
      const d = new Date(iso)
      return d.toLocaleString()
    } catch {
      return iso
    }
  }

  private _handleAction(command: string) {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'command', deviceId: this.deviceId, command },
      bubbles: true, composed: true,
    }))
  }

  render() {
    return html`
      <div class="card">
        <div class="header">
          <span class="status-dot ${this.status}"></span>
          <span class="name">${this.deviceName}</span>
        </div>
        ${!this.compact ? html`
          <div class="meta">${this.deviceType}${this.protocol ? ` · ${this.protocol}` : ''} · ${this._formatTime(this.lastSeen)}</div>
          ${this.properties.length > 0 ? html`
            <table class="props-table">
              ${this.properties.map(p => html`
                <tr>
                  <td>${p.displayName || p.name}</td>
                  <td>${p.currentValue ?? p.value ?? '-'} ${p.unit ?? ''}</td>
                </tr>
              `)}
            </table>
          ` : ''}
          ${this.showActions ? html`
            <div class="actions">
              <button @click="${() => this._handleAction('refresh')}">刷新</button>
              <button @click="${() => this._handleAction('detail')}">详情</button>
            </div>
          ` : ''}
        ` : ''}
      </div>
    `
  }
}
