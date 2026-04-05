import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { DeviceProperty } from '../../../../types/agent-types'

@customElement('device-card')
export class DeviceCard extends LitElement {
  @property({ type: String }) deviceId = ''
  @property({ type: String }) deviceName = ''
  @property({ type: String }) status: 'online' | 'offline' | 'warning' | 'error' = 'offline'
  @property({ type: String }) deviceType = 'generic'
  @property({ type: String }) protocol = ''
  @property({ type: String }) lastSeen = ''
  @property({ type: Array }) properties: DeviceProperty[] = []
  @property({ type: Boolean }) showActions = true
  @property({ type: Boolean }) compact = false

  static styles = css`
    :host { display: block; }
    .card {
      background: var(--card, #fff);
      border: 1px solid var(--border, #e2e8f0);
      border-radius: var(--radius, 8px);
      padding: 12px;
    }
    .header {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-bottom: 8px;
    }
    .status-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      flex-shrink: 0;
    }
    .status-dot.online { background: var(--ok, #22c55e); }
    .status-dot.offline { background: var(--text-muted, #94a3b8); }
    .status-dot.warning { background: var(--warn, #f59e0b); }
    .status-dot.error { background: var(--danger, #ef4444); }
    .name { font-weight: 600; font-size: 0.875rem; }
    .meta { font-size: 0.75rem; color: var(--text-muted, #888); }
    .props-table {
      width: 100%;
      font-size: 0.75rem;
      border-collapse: collapse;
    }
    .props-table td {
      padding: 2px 0;
    }
    .props-table td:last-child {
      text-align: right;
      font-family: monospace;
    }
    .actions {
      margin-top: 8px;
      display: flex;
      gap: 6px;
    }
    .actions button {
      font-size: 0.75rem;
      padding: 3px 8px;
      border-radius: 4px;
      border: 1px solid var(--border, #e2e8f0);
      background: transparent;
      cursor: pointer;
      color: var(--text, #1a1a1a);
    }
    .actions button:hover { background: var(--bg-elevated, #f8fafc); }
  `

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
