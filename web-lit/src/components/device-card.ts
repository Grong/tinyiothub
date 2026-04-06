// web-lit/src/components/device-card.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import type { Device } from '../services/devices'
import type { Tag } from '../types/tag'
import { tagApi } from '../services/tags'
import { navigate } from '../lib/navigate'
import './tag-selector'
import { hostStyles } from '../styles/shared-host'

type DeviceStatus = 'online' | 'offline' | 'error' | 'maintenance'

function getDeviceDisplayName(device: Device): string {
  return device.displayName || device.name
}

function getDeviceProductName(device: Device): string {
  return (device as any).productName || (device as any).product_name || device.protocol || '未知产品'
}

function getStatusLabel(status: DeviceStatus): string {
  const labels: Record<DeviceStatus, string> = {
    online: '在线',
    offline: '离线',
    error: '错误',
    maintenance: '维护',
  }
  return labels[status]
}

function formatTime(isoString: string | undefined): string {
  if (!isoString) return ''
  try {
    const d = new Date(isoString)
    const pad = (n: number) => n.toString().padStart(2, '0')
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
  } catch {
    return ''
  }
}

@customElement('device-card')
export class DeviceCard extends LitElement {
  static styles = [hostStyles, css`
    :host { display: block; }

    .card {
      position: relative;
      display: flex;
      flex-direction: column;
      background: var(--card);
      border-radius: var(--radius-lg);
      box-shadow: var(--glass-shadow-sm);
      cursor: pointer;
      padding: 20px;
      transition: box-shadow var(--duration-normal) var(--ease-out);
    }
    .card:hover {
      box-shadow: var(--glass-shadow-md);
    }

    /* Header — icon + name+meta (left), badge (right) */
    .card-header {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      gap: 12px;
      margin-bottom: 12px;
    }
    .header-left {
      display: flex;
      align-items: center;
      gap: 12px;
      min-width: 0;
      flex: 1;
    }
    .header-info {
      display: flex;
      flex-direction: column;
      min-width: 0;
    }
    .device-icon {
      width: 48px;
      height: 48px;
      border-radius: var(--radius-md);
      background: var(--accent-subtle);
      display: flex;
      align-items: center;
      justify-content: center;
      flex-shrink: 0;
    }
    .device-icon svg {
      width: 20px;
      height: 20px;
    }

    /* Status badge */
    .status-badge {
      padding: 4px 8px;
      border-radius: var(--radius-sm);
      font-size: 11px;
      font-weight: 500;
    }
    .status-badge.online { background: var(--ok-subtle); color: var(--ok); }
    .status-badge.offline { background: var(--bg-muted); color: var(--muted); }
    .status-badge.error { background: var(--danger-subtle); color: var(--danger); }
    .status-badge.maintenance { background: var(--warn-subtle); color: var(--warn); }

    /* Name & meta (inside header) */
    .device-name {
      font-size: 15px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .device-meta {
      display: flex;
      align-items: center;
      gap: 4px;
      font-size: 12px;
      color: var(--muted);
      margin-top: 2px;
    }
    .device-meta span {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    /* Description */
    .description {
      font-size: 13px;
      color: var(--muted);
      line-height: 1.5;
      display: -webkit-box;
      -webkit-line-clamp: 2;
      -webkit-box-orient: vertical;
      overflow: hidden;
      margin-bottom: 12px;
    }
    .description.empty { opacity: 0.6; }

    /* Properties bar */
    .properties {
      display: flex;
      flex-wrap: wrap;
      gap: 4px;
      margin-bottom: 16px;
    }
    .property-badge {
      display: inline-flex;
      align-items: center;
      padding: 2px 6px;
      border-radius: 4px;
      font-size: 10px;
      background: var(--accent-subtle);
      color: var(--accent);
    }
    .property-more {
      display: inline-flex;
      align-items: center;
      padding: 2px 6px;
      border-radius: 4px;
      font-size: 10px;
      background: var(--bg-muted);
      color: var(--muted);
    }

    /* Meta bar (time) */
    .meta-bar {
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      font-size: 12px;
      color: var(--muted);
    }
    .meta-item {
      display: flex;
      align-items: center;
      gap: 4px;
    }
    .meta-item svg {
      width: 14px;
      height: 14px;
      flex-shrink: 0;
    }

    /* Footer — tags + actions */
    .card-footer {
      display: flex;
      align-items: center;
      margin-top: 12px;
      padding-top: 12px;
      box-shadow: 0 -1px 0 var(--card-highlight);
    }
    .tags-area {
      flex: 1;
      min-width: 0;
      overflow: visible;
    }
    .divider {
      width: 1px;
      height: 14px;
      margin: 0 4px;
      flex-shrink: 0;
      box-shadow: 1px 0 0 var(--card-highlight);
    }
    .actions {
      display: flex;
      align-items: center;
      flex-shrink: 0;
    }
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
      opacity: 0;
      transition: opacity 0.15s ease, background 0.15s ease;
    }
    .card:hover .action-btn { opacity: 1; }
    .action-btn:hover { background: var(--bg-hover); color: var(--text); }
    .action-btn.danger:hover { background: var(--danger-subtle); color: var(--danger); }

    /* Status icon colors */
    .icon-online { color: var(--ok); }
    .icon-offline { color: var(--muted); }
    .icon-error { color: var(--danger); }
    .icon-maintenance { color: var(--warn); }
  `]

  @property({ type: Object }) device!: Device
  @property({ type: Function }) onEdit!: (d: Device) => void
  @property({ type: Function }) onDelete!: (d: Device) => void
  @state() private loadedTags: Tag[] = []

  private get status(): DeviceStatus {
    // status string is a computed property — may not be sent by API.
    // Derive from state number: 1=online, 0=offline, 2=error, 3=maintenance, <0=error
    if (this.device.status) return this.device.status as DeviceStatus
    const s = this.device.state
    if (s === 1) return 'online'
    if (s === 2) return 'error'
    if (s === 3) return 'maintenance'
    if (s !== undefined && s < 0) return 'error'
    return 'offline'
  }

  updated(changed: Map<string, unknown>) {
    if (changed.has('device') && this.device?.id) {
      this.loadTags()
    }
  }

  private async loadTags() {
    try {
      const res = await tagApi.getResourceTags(this.device.id)
      this.loadedTags = res.result || []
    } catch {
      this.loadedTags = []
    }
  }

  private get displayName(): string {
    return getDeviceDisplayName(this.device)
  }

  private get productName(): string {
    return getDeviceProductName(this.device)
  }

  private get updatedTime(): string {
    return formatTime(this.device.updatedAt || this.device.createdAt)
  }

  private get statusIcon() {
    const s = this.status
    switch (s) {
      case 'online':
        return html`<svg class="icon-online" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12.55a11 11 0 0 1 14.08 0M1.42 9a16 16 0 0 1 21.16 0M8.53 16.11a6 6 0 0 1 6.95 0M12 20h.01"/></svg>`
      case 'error':
        return html`<svg class="icon-error" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>`
      case 'maintenance':
        return html`<svg class="icon-maintenance" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>`
      default:
        return html`<svg class="icon-offline" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="1" y1="1" x2="23" y2="23"/><path d="M16.72 11.06A10.94 10.94 0 0 1 19 12.55M5 12.55a10.94 10.94 0 0 1 5.17-2.39M10.71 5.05A16 16 0 0 1 22.58 9M1.42 9a15.91 15.91 0 0 1 4.7-2.88M8.53 16.11a6 6 0 0 1 6.95 0M12 20h.01"/></svg>`
    }
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
    navigate(`device-detail?id=${this.device.id}`)
  }

  render() {
    const status = this.status
    const desc = this.device.description || ''
    const MAX_PROPS = 3
    const props: { name: string; value: string }[] = (this.device as any).properties?.slice(0, MAX_PROPS) || []

    return html`
      <div class="card" @click=${this.handleClick}>
        <!-- Header -->
        <div class="card-header">
          <div class="header-left">
            <div class="device-icon">${this.statusIcon}</div>
            <div class="header-info">
              <div class="device-name" title=${this.displayName}>${this.displayName}</div>
              <div class="device-meta">
                <span title=${this.productName}>${this.productName}</span>
              </div>
            </div>
          </div>
          <span class="status-badge ${status}">${getStatusLabel(status)}</span>
        </div>

        <!-- Description -->
        <div class="description ${!desc ? 'empty' : ''}" title=${desc}>
          ${desc || '暂无描述'}
        </div>

        <!-- Properties -->
        ${props.length > 0 ? html`
          <div class="properties">
            ${props.map(p => html`<span class="property-badge">${p.name}: ${p.value}</span>`)}
            ${((this.device as any).properties?.length || 0) > MAX_PROPS ? html`
              <span class="property-more">+${(this.device as any).properties.length - MAX_PROPS}</span>
            ` : ''}
          </div>
        ` : ''}

        <!-- Meta bar -->
        ${this.updatedTime ? html`
          <div class="meta-bar">
            <span class="meta-item">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
              更新于 ${this.updatedTime}
            </span>
          </div>
        ` : ''}

        <!-- Footer -->
        <div class="card-footer">
          <div class="tags-area" @click=${(e: Event) => e.stopPropagation()}>
            <tag-selector
              .targetId=${this.device.id}
              .initialTags=${this.loadedTags}
              .onChange=${() => this.loadTags()}
            ></tag-selector>
          </div>
          <div class="divider"></div>
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
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'device-card': DeviceCard }
}
