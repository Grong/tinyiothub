import { LitElement, html} from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { navigate } from '../lib/navigate'

interface MarketplaceTemplate {
  id: string
  name: string
  displayName: string
  description: string
  version: string
  category: string
  protocol: string
  manufacturer: string
  tags: string[]
  author: { name: string; email: string }
  downloads: number
  rating: number
  reviews: number
  license: string
  icon?: string
}

interface MarketplaceDriver {
  id: string
  name: string
  version: string
  protocol: string
  description: string
  tags: string[]
  author: { name: string; email: string }
  icon?: string
  downloads: number
  rating: number
  license: string
  homepage?: string
  documentation?: string
}

@customElement('marketplace-page')
export class MarketplacePage extends LitElement {
  createRenderRoot() { return this }
  

  @state() activeTab: 'templates' | 'drivers' = 'templates'
  @state() templates: MarketplaceTemplate[] = []
  @state() drivers: MarketplaceDriver[] = []
  @state() loading = true
  @state() search = ''
  @state() filter = 'all'
  @state() sort = 'popular'

  readonly templateFilters = [
    { value: 'all', label: '全部分类' },
    { value: 'sensor', label: '传感器' },
    { value: 'actuator', label: '执行器' },
  ]

  readonly driverFilters = [
    { value: 'all', label: '全部协议' },
    { value: 'modbus', label: 'Modbus' },
    { value: 'onvif', label: 'ONVIF' },
    { value: 'snmp', label: 'SNMP' },
    { value: 'mqtt', label: 'MQTT' },
  ]

  readonly sortOptions = [
    { value: 'popular', label: '最受欢迎' },
    { value: 'recent', label: '最新' },
    { value: 'rating', label: '评分最高' },
  ]

  async connectedCallback() {
    super.connectedCallback()
    await this.loadData()
  }

  async loadData() {
    this.loading = true
    try {
      if (this.activeTab === 'templates') {
        await this.loadTemplates()
      } else {
        await this.loadDrivers()
      }
    } catch (err) {
      console.error('Failed to load marketplace data:', err)
    } finally {
      this.loading = false
    }
  }

  async loadTemplates() {
    // Mock data for templates - in production this would call the marketplace API
    this.templates = [
      {
        id: 'modbus-temp-1',
        name: 'modbus-temperature-sensor',
        displayName: 'Modbus 温度传感器',
        description: '标准 Modbus RTU/TCP 温度传感器模板，支持多种温度范围和精度',
        version: '1.2.0',
        category: 'sensor',
        protocol: 'modbus',
        manufacturer: 'TinyIoTHub',
        tags: ['温度', '传感器', '工业'],
        author: { name: 'TinyIoTHub', email: '' },
        downloads: 1247,
        rating: 4.8,
        reviews: 56,
        license: 'MIT',
      },
      {
        id: 'onvif-camera-1',
        name: 'onvif-ip-camera',
        displayName: 'ONVIF 网络摄像头',
        description: '支持 ONVIF 协议的网络摄像头模板，适用于安防监控系统',
        version: '2.0.1',
        category: 'sensor',
        protocol: 'onvif',
        manufacturer: 'TinyIoTHub',
        tags: ['摄像头', '安防', 'ONVIF'],
        author: { name: 'TinyIoTHub', email: '' },
        downloads: 892,
        rating: 4.6,
        reviews: 34,
        license: 'MIT',
      },
      {
        id: 'snmp-switch-1',
        name: 'snmp-network-switch',
        displayName: 'SNMP 网络交换机',
        description: '标准 SNMP 网络设备监控模板，支持端口状态和流量统计',
        version: '1.5.0',
        category: 'actuator',
        protocol: 'snmp',
        manufacturer: 'TinyIoTHub',
        tags: ['网络', '交换机', 'SNMP'],
        author: { name: 'TinyIoTHub', email: '' },
        downloads: 456,
        rating: 4.3,
        reviews: 18,
        license: 'MIT',
      },
      {
        id: 'mqtt-sensor-1',
        name: 'mqtt-iot-sensor',
        displayName: 'MQTT 物联网传感器',
        description: '支持 MQTT 协议的通用物联网传感器模板，适合 DIY 设备接入',
        version: '1.8.0',
        category: 'sensor',
        protocol: 'mqtt',
        manufacturer: 'Community',
        tags: ['MQTT', '物联网', 'DIY'],
        author: { name: '社区贡献', email: '' },
        downloads: 2341,
        rating: 4.9,
        reviews: 112,
        license: 'MIT',
      },
    ]
  }

  async loadDrivers() {
    // Mock data for drivers
    this.drivers = [
      {
        id: 'modbus-driver',
        name: 'modbus-driver',
        version: '2.1.0',
        protocol: 'modbus',
        description: 'Modbus RTU/TCP 驱动，支持串口和网络通信',
        tags: ['modbus', 'rtu', 'tcp', '工业'],
        author: { name: 'TinyIoTHub', email: '' },
        downloads: 3421,
        rating: 4.7,
        license: 'Apache-2.0',
        homepage: 'https://tinyiothub.com',
        documentation: 'https://docs.tinyiothub.com',
      },
      {
        id: 'onvif-driver',
        name: 'onvif-driver',
        version: '1.6.0',
        protocol: 'onvif',
        description: 'ONVIF 设备发现和服务管理驱动',
        tags: ['onvif', 'camera', '安防'],
        author: { name: 'TinyIoTHub', email: '' },
        downloads: 1876,
        rating: 4.5,
        license: 'Apache-2.0',
        homepage: 'https://tinyiothub.com',
        documentation: 'https://docs.tinyiothub.com',
      },
      {
        id: 'snmp-driver',
        name: 'snmp-driver',
        version: '1.3.0',
        protocol: 'snmp',
        description: 'SNMP v1/v2c/v3 协议驱动，支持设备发现和监控',
        tags: ['snmp', '监控', '网络'],
        author: { name: 'TinyIoTHub', email: '' },
        downloads: 987,
        rating: 4.4,
        license: 'Apache-2.0',
        homepage: 'https://tinyiothub.com',
        documentation: 'https://docs.tinyiothub.com',
      },
      {
        id: 'mqtt-driver',
        name: 'mqtt-driver',
        version: '3.0.0',
        protocol: 'mqtt',
        description: 'MQTT 3.1.1/5.0 协议驱动，支持 QoS 和 TLS',
        tags: ['mqtt', 'iot', '消息队列'],
        author: { name: 'TinyIoTHub', email: '' },
        downloads: 4562,
        rating: 4.9,
        license: 'Apache-2.0',
        homepage: 'https://tinyiothub.com',
        documentation: 'https://docs.tinyiothub.com',
      },
    ]
  }

  handleTabChange(tab: 'templates' | 'drivers') {
    this.activeTab = tab
    this.search = ''
    this.filter = 'all'
    this.loadData()
  }

  handleSearch(e: Event) {
    this.search = (e.target as HTMLInputElement).value
  }

  handleFilterChange(e: Event) {
    this.filter = (e.target as HTMLSelectElement).value
  }

  getFilteredTemplates(): MarketplaceTemplate[] {
    let items = this.templates
    if (this.search) {
      const q = this.search.toLowerCase()
      items = items.filter(t =>
        t.name.toLowerCase().includes(q) ||
        t.displayName.toLowerCase().includes(q) ||
        t.description.toLowerCase().includes(q)
      )
    }
    if (this.filter !== 'all') {
      items = items.filter(t => t.category === this.filter)
    }
    return items
  }

  getFilteredDrivers(): MarketplaceDriver[] {
    let items = this.drivers
    if (this.search) {
      const q = this.search.toLowerCase()
      items = items.filter(d =>
        d.name.toLowerCase().includes(q) ||
        d.description.toLowerCase().includes(q)
      )
    }
    if (this.filter !== 'all') {
      items = items.filter(d => d.protocol.toLowerCase() === this.filter)
    }
    return items
  }

  render() {
    return html`
      <!-- Hero -->
      <div class="hero">
        <h1 class="hero-title">设备市场</h1>
        <p class="hero-subtitle">
          探索来自社区的优质设备模板与驱动，开箱即用，快速接入传感器、执行器与工业设备
        </p>
      </div>

      <!-- Tabs -->
      <div class="tabs-container">
        <div class="tabs">
          <button
            class="tab ${this.activeTab === 'templates' ? 'active' : ''}"
            @click=${() => this.handleTabChange('templates')}
          >
            设备模板
          </button>
          <button
            class="tab ${this.activeTab === 'drivers' ? 'active' : ''}"
            @click=${() => this.handleTabChange('drivers')}
          >
            驱动
          </button>
        </div>

        <!-- Search & Filters -->
        <div class="search-filters">
          <input
            type="text"
            class="search-input"
            placeholder=${this.activeTab === 'templates' ? '搜索模板...' : '搜索驱动...'}
            .value=${this.search}
            @input=${this.handleSearch}
          />
          <select class="filter-select" .value=${this.filter} @change=${this.handleFilterChange}>
            ${this.activeTab === 'templates'
              ? this.templateFilters.map(f => html`<option value=${f.value}>${f.label}</option>`)
              : this.driverFilters.map(f => html`<option value=${f.value}>${f.label}</option>`)
            }
          </select>
          <select class="filter-select">
            ${this.sortOptions.map(s => html`<option value=${s.value}>${s.label}</option>`)}
          </select>
        </div>
      </div>

      <!-- Content -->
      <div class="content-area">
        ${this.loading ? this.renderLoading() :
          this.activeTab === 'templates'
            ? this.renderTemplates()
            : this.renderDrivers()
        }
      </div>
    `
  }

  renderLoading() {
    return html`<div class="loading"><div class="spinner"></div></div>`
  }

  renderTemplates() {
    const items = this.getFilteredTemplates()
    if (items.length === 0) {
      return html`
        <div class="empty-state">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M9.75 9.75l4.5 4.5m0-4.5l-4.5 4.5M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
          </svg>
          <h3>未找到模板</h3>
          <p>尝试调整搜索条件或筛选器</p>
        </div>
      `
    }

    return html`
      <div class="grid">
        ${items.map(template => html`
          <div class="card">
            <div class="card-header">
              <div class="card-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z"/>
                </svg>
              </div>
              <div class="card-title-area">
                <h3 class="card-title">${template.displayName}</h3>
                <span class="card-subtitle">by ${template.author.name}</span>
              </div>
            </div>
            <p class="card-description">${template.description}</p>
            <div class="card-footer">
              <div class="card-tags">
                ${template.tags.slice(0, 3).map(tag => html`<span class="tag">${tag}</span>`)}
              </div>
              <div class="card-stats">
                <span class="stat">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M4.5 12.75l6 6 9-13.5"/>
                  </svg>
                  ${template.downloads}
                </span>
                <span class="stat">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M11.48 3.499a.562.562 0 011.04 0l2.125 5.111a.563.563 0 00.475.345l5.518.442c.499.04.701.663.321.988l-4.204 3.602a.563.563 0 00-.182.557l1.285 5.385a.562.562 0 01-.84.61l-4.725-2.885a.563.563 0 00-.586 0L6.982 20.54a.562.562 0 01-.84-.61l1.285-5.386a.562.562 0 00-.182-.557l-4.204-3.602a.563.563 0 01.321-.988l5.518-.442a.563.563 0 00.475-.345L11.48 3.5z"/>
                  </svg>
                  ${template.rating}
                </span>
              </div>
            </div>
          </div>
        `)}
      </div>
    `
  }

  renderDrivers() {
    const items = this.getFilteredDrivers()
    if (items.length === 0) {
      return html`
        <div class="empty-state">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M9.75 9.75l4.5 4.5m0-4.5l-4.5 4.5M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
          </svg>
          <h3>未找到驱动</h3>
          <p>尝试调整搜索条件或筛选器</p>
        </div>
      `
    }

    return html`
      <div class="grid">
        ${items.map(driver => html`
          <div class="card">
            <div class="card-header">
              <div class="card-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M5.25 14.25h13.5m-13.5 0a3 3 0 01-3-3m3 3a3 3 0 100 6h13.5a3 3 0 100-6m-16.5-3a3 3 0 013-3h13.5a3 3 0 013 3m-19.5 0a4.5 4.5 0 01.9-2.7L5.737 5.1a3.375 3.375 0 012.7-1.35h7.126c1.062 0 2.062.5 2.7 1.35l2.587 3.45a4.5 4.5 0 01.9 2.7m0 0a3 3 0 01-3 3m0 3h.008v.008h-.008v-.008zm0-6a3 3 0 01-3 3m0 6h.008v.008h-.008v-.008zm-3 6a3 3 0 01-3 3m0 6h.008v.008h-.008v-.008z"/>
                </svg>
              </div>
              <div class="card-title-area">
                <h3 class="card-title">${driver.name}</h3>
                <span class="card-subtitle">${driver.protocol.toUpperCase()} v${driver.version}</span>
              </div>
            </div>
            <p class="card-description">${driver.description}</p>
            <div class="card-footer">
              <div class="card-tags">
                ${driver.tags.slice(0, 3).map(tag => html`<span class="tag">${tag}</span>`)}
              </div>
              <div class="card-stats">
                <span class="stat">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M4.5 12.75l6 6 9-13.5"/>
                  </svg>
                  ${driver.downloads}
                </span>
                <span class="stat">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M11.48 3.499a.562.562 0 011.04 0l2.125 5.111a.563.563 0 00.475.345l5.518.442c.499.04.701.663.321.988l-4.204 3.602a.563.563 0 00-.182.557l1.285 5.385a.562.562 0 01-.84.61l-4.725-2.885a.563.563 0 00-.586 0L6.982 20.54a.562.562 0 01-.84-.61l1.285-5.386a.562.562 0 00-.182-.557l-4.204-3.602a.563.563 0 01.321-.988l5.518-.442a.563.563 0 00.475-.345L11.48 3.5z"/>
                  </svg>
                  ${driver.rating}
                </span>
              </div>
            </div>
          </div>
        `)}
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'marketplace-page': MarketplacePage
  }
}
