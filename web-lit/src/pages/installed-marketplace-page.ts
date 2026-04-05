import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'

interface InstalledItem {
  id: string
  name: string
  displayName: string
  description: string
  version: string
  type: 'template' | 'driver'
  category: string
  protocol?: string
  author: { name: string; email: string }
  installedAt: string
  status: 'active' | 'inactive' | 'update_available'
}

@customElement('installed-marketplace-page')
export class InstalledMarketplacePage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
      background: var(--bg);
      min-height: 100%;
    }

    /* Header */
    .page-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      margin-bottom: 24px;
    }

    .page-title {
      font-size: 24px;
      font-weight: 700;
      color: var(--text-strong);
      margin: 0;
    }

    .header-actions {
      display: flex;
      gap: 12px;
    }

    .btn {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 10px 16px;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      font-weight: 500;
      cursor: pointer;
      transition: border-color var(--duration-fast) ease, background var(--duration-fast) ease;
    }

    .btn:hover {
      background: var(--bg-hover);
      border-color: var(--border-strong);
    }

    .btn-primary {
      background: var(--accent);
      border-color: var(--accent);
      color: var(--accent-foreground);
    }

    .btn-primary:hover {
      background: var(--accent-hover);
      border-color: var(--accent-hover);
    }

    /* Tabs */
    .tabs {
      display: flex;
      gap: 4px;
      margin-bottom: 24px;
      border-bottom: 1px solid var(--border);
    }

    .tab {
      padding: 12px 20px;
      border: none;
      background: transparent;
      color: var(--muted);
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      border-bottom: 2px solid transparent;
      margin-bottom: -1px;
      transition: color var(--duration-fast) ease, border-color var(--duration-fast) ease;
    }

    .tab:hover {
      color: var(--text);
    }

    .tab.active {
      color: var(--accent);
      border-bottom-color: var(--accent);
    }

    /* Items list */
    .items-list {
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      overflow: hidden;
    }

    .item {
      display: flex;
      align-items: center;
      gap: 16px;
      padding: 16px 20px;
      border-bottom: 1px solid var(--border);
      transition: background var(--duration-fast) ease;
    }

    .item:last-child {
      border-bottom: none;
    }

    .item:hover {
      background: var(--bg-hover);
    }

    .item-icon {
      width: 44px;
      height: 44px;
      border-radius: var(--radius-md);
      background: var(--accent-subtle);
      display: flex;
      align-items: center;
      justify-content: center;
      color: var(--accent);
      flex-shrink: 0;
    }

    .item-icon svg {
      width: 22px;
      height: 22px;
    }

    .item-content {
      flex: 1;
      min-width: 0;
    }

    .item-header {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 4px;
    }

    .item-name {
      font-size: 14px;
      font-weight: 600;
      color: var(--text-strong);
    }

    .item-badge {
      padding: 2px 8px;
      border-radius: var(--radius-sm);
      font-size: 11px;
      font-weight: 500;
    }

    .item-badge.active {
      background: var(--ok-subtle);
      color: var(--ok);
    }

    .item-badge.inactive {
      background: var(--bg-muted);
      color: var(--muted);
    }

    .item-badge.update {
      background: var(--warn-subtle);
      color: var(--warn);
    }

    .item-meta {
      font-size: 12px;
      color: var(--muted);
    }

    .item-description {
      font-size: 13px;
      color: var(--muted);
      margin: 0;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .item-actions {
      display: flex;
      gap: 8px;
      flex-shrink: 0;
    }

    .action-btn {
      padding: 6px 12px;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 12px;
      cursor: pointer;
      transition: background var(--duration-fast) ease, border-color var(--duration-fast) ease;
    }

    .action-btn:hover {
      background: var(--bg-hover);
      border-color: var(--border-strong);
    }

    .action-btn.danger:hover {
      background: var(--danger-subtle);
      border-color: var(--danger);
      color: var(--danger);
    }

    .action-btn.primary {
      background: var(--accent);
      border-color: var(--accent);
      color: var(--accent-foreground);
    }

    .action-btn.primary:hover {
      background: var(--accent-hover);
    }

    /* Empty state */
    .empty-state {
      text-align: center;
      padding: 64px 24px;
      color: var(--muted);
    }

    .empty-state svg {
      width: 64px;
      height: 64px;
      margin-bottom: 16px;
      opacity: 0.5;
    }

    .empty-state h3 {
      font-size: 16px;
      font-weight: 600;
      color: var(--text);
      margin: 0 0 8px;
    }

    .empty-state p {
      font-size: 14px;
      margin: 0 0 20px;
    }

    /* Loading */
    .loading {
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 64px 24px;
    }

    .spinner {
      width: 32px;
      height: 32px;
      border: 3px solid var(--border);
      border-top-color: var(--accent);
      border-radius: 50%;
      animation: spin 1s linear infinite;
    }

    @keyframes spin {
      to { transform: rotate(360deg); }
    }
  `

  @state() items: InstalledItem[] = []
  @state() loading = true
  @state() activeTab: 'all' | 'templates' | 'drivers' = 'all'

  async connectedCallback() {
    super.connectedCallback()
    await this.loadItems()
  }

  async loadItems() {
    this.loading = true
    try {
      // Mock data for installed items
      this.items = [
        {
          id: 'modbus-temp-1',
          name: 'modbus-temperature-sensor',
          displayName: 'Modbus 温度传感器',
          description: '标准 Modbus RTU/TCP 温度传感器模板',
          version: '1.2.0',
          type: 'template',
          category: 'sensor',
          protocol: 'modbus',
          author: { name: 'TinyIoTHub', email: '' },
          installedAt: '2024-03-15T10:30:00Z',
          status: 'active',
        },
        {
          id: 'onvif-camera-1',
          name: 'onvif-ip-camera',
          displayName: 'ONVIF 网络摄像头',
          description: '支持 ONVIF 协议的网络摄像头模板',
          version: '2.0.0',
          type: 'template',
          category: 'sensor',
          protocol: 'onvif',
          author: { name: 'TinyIoTHub', email: '' },
          installedAt: '2024-03-10T14:20:00Z',
          status: 'update_available',
        },
        {
          id: 'modbus-driver',
          name: 'modbus-driver',
          displayName: 'Modbus 驱动',
          description: 'Modbus RTU/TCP 协议驱动',
          version: '2.1.0',
          type: 'driver',
          category: 'modbus',
          author: { name: 'TinyIoTHub', email: '' },
          installedAt: '2024-02-28T09:15:00Z',
          status: 'active',
        },
        {
          id: 'mqtt-driver',
          name: 'mqtt-driver',
          displayName: 'MQTT 驱动',
          description: 'MQTT 3.1.1/5.0 协议驱动',
          version: '3.0.0',
          type: 'driver',
          category: 'mqtt',
          author: { name: 'TinyIoTHub', email: '' },
          installedAt: '2024-02-20T16:45:00Z',
          status: 'active',
        },
      ]
    } catch (err) {
      console.error('Failed to load installed items:', err)
    } finally {
      this.loading = false
    }
  }

  handleTabChange(tab: 'all' | 'templates' | 'drivers') {
    this.activeTab = tab
  }

  getFilteredItems(): InstalledItem[] {
    if (this.activeTab === 'all') return this.items
    return this.items.filter(item => item.type === this.activeTab.slice(0, -1))
  }

  getStatusText(status: string): string {
    switch (status) {
      case 'active': return '活跃'
      case 'inactive': return '未启用'
      case 'update_available': return '可更新'
      default: return status
    }
  }

  formatDate(dateStr: string): string {
    const date = new Date(dateStr)
    return date.toLocaleDateString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
    })
  }

  render() {
    return html`
      <div class="page-header">
        <h1 class="page-title">已安装</h1>
        <div class="header-actions">
          <button class="btn" @click=${() => this.loadItems()}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99"/>
            </svg>
            刷新
          </button>
          <button class="btn btn-primary" @click=${() => window.location.href = '/marketplace'}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15"/>
            </svg>
            浏览市场
          </button>
        </div>
      </div>

      <div class="tabs">
        <button
          class="tab ${this.activeTab === 'all' ? 'active' : ''}"
          @click=${() => this.handleTabChange('all')}
        >
          全部 (${this.items.length})
        </button>
        <button
          class="tab ${this.activeTab === 'templates' ? 'active' : ''}"
          @click=${() => this.handleTabChange('templates')}
        >
          模板 (${this.items.filter(i => i.type === 'template').length})
        </button>
        <button
          class="tab ${this.activeTab === 'drivers' ? 'active' : ''}"
          @click=${() => this.handleTabChange('drivers')}
        >
          驱动 (${this.items.filter(i => i.type === 'driver').length})
        </button>
      </div>

      ${this.loading ? this.renderLoading() : this.renderItems()}
    `
  }

  renderLoading() {
    return html`
      <div class="loading"><div class="spinner"></div></div>
    `
  }

  renderItems() {
    const items = this.getFilteredItems()
    if (items.length === 0) {
      return html`
        <div class="empty-state">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M20.25 7.5l-.625 10.632a2.25 2.25 0 01-2.247 2.118H6.622a2.25 2.25 0 01-2.247-2.118L3.75 7.5M10 11.25h4M3.375 7.5h17.25c.621 0 1.125-.504 1.125-1.125v-1.5c0-.621-.504-1.125-1.125-1.125H3.375c-.621 0-1.125.504-1.125 1.125v1.5c0 .621.504 1.125 1.125 1.125z"/>
          </svg>
          <h3>暂无已安装项目</h3>
          <p>从市场安装模板或驱动来扩展功能</p>
          <button class="btn btn-primary" @click=${() => window.location.href = '/marketplace'}>
            浏览市场
          </button>
        </div>
      `
    }

    return html`
      <div class="items-list">
        ${items.map(item => html`
          <div class="item">
            <div class="item-icon">
              ${item.type === 'template' ? html`
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z"/>
                </svg>
              ` : html`
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M5.25 14.25h13.5m-13.5 0a3 3 0 01-3-3m3 3a3 3 0 100 6h13.5a3 3 0 100-6m-16.5-3a3 3 0 013-3h13.5a3 3 0 013 3m-19.5 0a4.5 4.5 0 01.9-2.7L5.737 5.1a3.375 3.375 0 012.7-1.35h7.126c1.062 0 2.062.5 2.7 1.35l2.587 3.45a4.5 4.5 0 01.9 2.7m0 0a3 3 0 01-3 3m0 3h.008v.008h-.008v-.008zm0-6a3 3 0 01-3 3m0 6h.008v.008h-.008v-.008zm-3 6a3 3 0 01-3 3m0 6h.008v.008h-.008v-.008z"/>
                </svg>
              `}
            </div>
            <div class="item-content">
              <div class="item-header">
                <span class="item-name">${item.displayName}</span>
                <span class="item-badge ${item.status}">${this.getStatusText(item.status)}</span>
              </div>
              <div class="item-meta">
                ${item.type === 'template' ? '模板' : '驱动'} · v${item.version} · 安装于 ${this.formatDate(item.installedAt)}
              </div>
              <p class="item-description">${item.description}</p>
            </div>
            <div class="item-actions">
              ${item.status === 'update_available' ? html`
                <button class="action-btn primary">更新</button>
              ` : ''}
              <button class="action-btn">配置</button>
              <button class="action-btn danger">卸载</button>
            </div>
          </div>
        `)}
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'installed-marketplace-page': InstalledMarketplacePage
  }
}
