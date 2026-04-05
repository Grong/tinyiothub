// web-lit/src/components/device-card.ts
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { Device } from '../services/devices'

@customElement('device-card')
export class DeviceCard extends LitElement {
  static styles = css`
    :host { display: block; }
    .card {
      background: var(--card);
      border-radius: var(--radius-lg);
      box-shadow: var(--shadow-sm);
      overflow: hidden;
      cursor: pointer;
      transition: transform 0.15s ease, box-shadow 0.15s ease;
    }
    .card:hover {
      transform: translateY(-2px);
      box-shadow: var(--shadow-md);
    }
    .card-left-bar {
      position: absolute;
      left: 0;
      top: 0;
      bottom: 0;
      width: 4px;
    }
    .card-content { padding: 16px; position: relative; }
    .card-header {
      display: flex;
      justify-content: space-between;
      align-items: flex-start;
      margin-bottom: 8px;
    }
    .device-name {
      font-size: 14px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .protocol-badge {
      font-size: 10px;
      padding: 2px 6px;
      border-radius: var(--radius-sm);
      background: var(--bg-muted);
      color: var(--muted);
      text-transform: uppercase;
    }
    .device-address {
      font-size: 12px;
      color: var(--muted);
      font-family: var(--mono);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      margin-bottom: 12px;
    }
    .card-footer {
      display: flex;
      justify-content: space-between;
      align-items: center;
    }
    .status {
      display: flex;
      align-items: center;
      gap: 6px;
      font-size: 12px;
    }
    .status-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
    }
    .status-dot.online { background: var(--ok); }
    .status-dot.offline { background: var(--muted); }
    .status-dot.error { background: var(--danger); }
    .status-dot.maintenance { background: var(--warn); }
    .actions { display: flex; gap: 4px; }
    .action-btn {
      width: 28px;
      height: 28px;
      display: flex;
      align-items: center;
      justify-content: center;
      border: none;
      border-radius: var(--radius-sm);
      background: transparent;
      color: var(--muted);
      cursor: pointer;
    }
    .action-btn:hover { background: var(--bg-hover); color: var(--text); }
    .action-btn.danger:hover { background: var(--danger-subtle); color: var(--danger); }
  `

  @property({ type: Object }) device!: Device
  @property({ type: Function }) onEdit!: (d: Device) => void
  @property({ type: Function }) onDelete!: (d: Device) => void

  private get deviceTypeColor(): string {
    const type = this.device.protocol?.toLowerCase() || ''
    if (type.includes('modbus')) return 'var(--accent)'
    if (type.includes('onvif')) return 'var(--ok)'
    if (type.includes('snmp')) return 'var(--warn)'
    if (type.includes('mqtt')) return 'var(--info)'
    return 'var(--muted)'
  }

  private handleEdit(e: Event) {
    e.stopPropagation()
    this.onEdit(this.device)
  }

  private handleDelete(e: Event) {
    e.stopPropagation()
    this.onDelete(this.device)
  }

  private handleClick() {
    window.history.pushState({}, '', `/device-detail?id=${this.device.id}`)
    window.dispatchEvent(new PopStateEvent('popstate'))
  }

  render() {
    const status = this.device.status || 'offline'
    return html`
      <div class="card" @click=${this.handleClick}>
        <div class="card-left-bar" style="background: ${this.deviceTypeColor}"></div>
        <div class="card-content">
          <div class="card-header">
            <h3 class="device-name">${this.device.name}</h3>
            ${this.device.protocol ? html`<span class="protocol-badge">${this.device.protocol}</span>` : ''}
          </div>
          ${this.device.address ? html`<div class="device-address">${this.device.address}</div>` : ''}
          <div class="card-footer">
            <div class="status">
              <span class="status-dot ${status}"></span>
              <span>${status === 'online' ? '在线' : status === 'offline' ? '离线' : status === 'maintenance' ? '维护' : '错误'}</span>
            </div>
            <div class="actions">
              <button class="action-btn" title="编辑" @click=${this.handleEdit}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931z"/>
                </svg>
              </button>
              <button class="action-btn danger" title="删除" @click=${this.handleDelete}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0"/>
                </svg>
              </button>
            </div>
          </div>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'device-card': DeviceCard }
}
