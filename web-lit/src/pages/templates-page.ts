import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { templateApi, type DeviceTemplate, type TemplateCategory } from '../services/templates'
import { navigate } from '../lib/navigate'
import { hostStyles } from '../styles/shared-host'

@customElement('templates-page')
export class TemplatesPage extends LitElement {
  static styles = [hostStyles, css`
    templates-page {
      display: flex;
      flex-direction: column;
      padding: 0;
      background: var(--bg);
      flex: 1;
      min-height: 0;
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

    /* Filters */
    .filters {
      display: flex;
      gap: 12px;
      margin-bottom: 20px;
      flex-wrap: wrap;
    }

    .filter-select {
      padding: 10px 14px;
      border: none;
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      cursor: pointer;
      box-shadow: var(--glass-shadow-sm);
      transition: background var(--duration-fast) ease;
    }

    .filter-select:hover {
      background: var(--bg-hover);
    }

    .search-input {
      padding: 10px 14px;
      border: none;
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 14px;
      min-width: 240px;
      box-shadow: var(--glass-shadow-sm);
      transition: background var(--duration-fast) ease, box-shadow var(--duration-fast) ease;
    }

    .search-input:focus {
      outline: none;
      box-shadow: var(--focus-ring);
    }

    /* Templates grid */
    .templates-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
      gap: 16px;
    }

    .template-card {
      background: var(--card);
      box-shadow: var(--glass-shadow-sm);
      border-radius: var(--radius-lg);
      padding: 20px;
      cursor: pointer;
    }

    .template-card:hover {
      box-shadow: var(--glass-shadow-md);
    }

    .template-header {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      margin-bottom: 12px;
    }

    .template-icon {
      width: 48px;
      height: 48px;
      border-radius: var(--radius-md);
      background: var(--accent-subtle);
      display: flex;
      align-items: center;
      justify-content: center;
      color: var(--accent);
      font-size: 20px;
    }

    .template-badge {
      padding: 4px 8px;
      border-radius: var(--radius-sm);
      font-size: 11px;
      font-weight: 500;
      text-transform: uppercase;
    }

    .template-badge.builtin {
      background: var(--accent-subtle);
      color: var(--accent);
    }

    .template-badge.custom {
      background: var(--bg-muted);
      color: var(--muted);
    }

    .template-name {
      font-size: 16px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0 0 8px;
    }

    .template-description {
      font-size: 13px;
      color: var(--muted);
      margin: 0 0 16px;
      line-height: 1.5;
      display: -webkit-box;
      -webkit-line-clamp: 2;
      -webkit-box-orient: vertical;
      overflow: hidden;
    }

    .template-meta {
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      font-size: 12px;
      color: var(--muted);
    }

    .template-meta-item {
      display: flex;
      align-items: center;
      gap: 4px;
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
      margin: 0;
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

    /* Pagination */
    .pagination {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 16px 0;
    }

    .pagination-info {
      font-size: 13px;
      color: var(--muted);
    }

    .pagination-buttons {
      display: flex;
      gap: 8px;
    }

    .page-btn {
      padding: 8px 12px;
      box-shadow: var(--glass-shadow-sm);
      border: none;
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .page-btn:hover:not(:disabled) {
      background: var(--bg-hover);
    }

    .page-btn:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  `]

  @state() templates: DeviceTemplate[] = []
  @state() categories: TemplateCategory[] = []
  @state() loading = true
  @state() search = ''
  @state() category = ''
  @state() page = 1
  @state() pageSize = 12
  @state() totalCount = 0

  async connectedCallback() {
    super.connectedCallback()
    await this.loadCategories()
    await this.loadTemplates()
  }

  async loadCategories() {
    try {
      const response = await templateApi.getTemplateCategories()
      if (response.result) {
        this.categories = response.result
      }
    } catch (err) {
      console.error('Failed to load categories:', err)
    }
  }

  async loadTemplates() {
    this.loading = true
    try {
      const response = await templateApi.getTemplates({
        page: this.page,
        pageSize: this.pageSize,
        category: this.category || undefined,
        keyword: this.search || undefined,
      })
      if (response.result) {
        // Handle paginated response
        const data = Array.isArray(response.result) ? response.result : (response.result as any).data || []
        this.templates = data
        const total = Array.isArray(response.result) ? response.result.length : (response.result as any).pagination?.totalCount || data.length
        this.totalCount = total
      }
    } catch (err: any) {
      console.error('Failed to load templates:', err)
    } finally {
      this.loading = false
    }
  }

  handleSearch(e: Event) {
    this.search = (e.target as HTMLInputElement).value
    this.page = 1
    this.loadTemplates()
  }

  handleCategoryChange(e: Event) {
    this.category = (e.target as HTMLSelectElement).value
    this.page = 1
    this.loadTemplates()
  }

  handlePageChange(newPage: number) {
    this.page = newPage
    this.loadTemplates()
  }

  viewTemplate(template: DeviceTemplate) {
    navigate(`device-detail?id=${template.id}`)
  }

  getCategoryDisplayName(cat: TemplateCategory): string {
    try {
      const display = cat.displayName
      if (typeof display === 'object' && display !== null) {
        return (display as Record<string, string>).zh || Object.values(display as object)[0] || cat.name
      }
      return cat.name
    } catch {
      return cat.name
    }
  }

  render() {
    return html`
      <div class="page-header">
        <h1 class="page-title">设备模板</h1>
      </div>

      <div class="filters">
        <input
          type="text"
          class="search-input"
          placeholder="搜索模板..."
          .value=${this.search}
          @input=${this.handleSearch}
        />
        <select class="filter-select" .value=${this.category} @change=${this.handleCategoryChange}>
          <option value="">全部分类</option>
          ${this.categories.map(cat => html`
            <option value=${cat.name}>${this.getCategoryDisplayName(cat)}</option>
          `)}
        </select>
      </div>

      ${this.loading ? this.renderLoading() : this.templates.length === 0 ? this.renderEmpty() : this.renderTemplateGrid()}

      ${!this.loading && this.templates.length > 0 ? this.renderPagination() : ''}
    `
  }

  renderLoading() {
    return html`<div class="loading"><div class="spinner"></div></div>`
  }

  renderEmpty() {
    return html`
      <div class="empty-state">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M9 12h3.75M9 15h3.75M9 18h3.75m3 .75H18a2.25 2.25 0 002.25-2.25V6.108c0-1.135-.845-2.098-1.976-2.192a48.424 48.424 0 00-1.123-.08m-5.801 0c-.065.21-.1.433-.1.664 0 .414.336.75.75.75h4.5a.75.75 0 00.75-.75 2.25 2.25 0 00-.1-.664m-5.801 0A2.251 2.251 0 0113.5 2.25H15c1.012 0 1.867.668 2.15 1.586m-5.8 0c-.376.023-.75.05-1.124.08C9.095 4.01 8.25 4.973 8.25 6.108V8.25m0 0H4.875c-.621 0-1.125.504-1.125 1.125v11.25c0 .621.504 1.125 1.125 1.125h9.75c.621 0 1.125-.504 1.125-1.125V9.375c0-.621-.504-1.125-1.125-1.125H8.25zM6.75 12h.008v.008H6.75V12zm0 3h.008v.008H6.75V15zm0 3h.008v.008H6.75V18z"/>
        </svg>
        <h3>暂无模板</h3>
        <p>设备模板可以帮助您快速创建设备</p>
      </div>
    `
  }

  renderTemplateGrid() {
    return html`
      <div class="templates-grid">
        ${this.templates.map(template => html`
          <div class="template-card" @click=${() => this.viewTemplate(template)}>
            <div class="template-header">
              <div class="template-icon">
                <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z"/>
                </svg>
              </div>
              <span class="template-badge ${template.isBuiltin ? 'builtin' : 'custom'}">
                ${template.isBuiltin ? '内置' : '自定义'}
              </span>
            </div>
            <h3 class="template-name">${template.displayName || template.name}</h3>
            <p class="template-description">${template.description || '暂无描述'}</p>
            <div class="template-meta">
              <span class="template-meta-item">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                </svg>
                ${template.deviceType || '-'}
              </span>
              <span class="template-meta-item">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M5.25 14.25h13.5m-13.5 0a3 3 0 01-3-3m3 3a3 3 0 100 6h13.5a3 3 0 100-6m-16.5-3a3 3 0 013-3h13.5a3 3 0 013 3m-19.5 0a4.5 4.5 0 01.9-2.7L5.737 5.1a3.375 3.375 0 012.7-1.35h7.126c1.062 0 2.062.5 2.7 1.35l2.587 3.45a4.5 4.5 0 01.9 2.7m0 0a3 3 0 01-3 3m0 3h.008v.008h-.008v-.008zm0-6a3 3 0 01-3 3m0 6h.008v.008h-.008v-.008zm-3 6a3 3 0 01-3 3m0 6h.008v.008h-.008v-.008z"/>
                </svg>
                ${template.protocolType || '-'}
              </span>
              <span class="template-meta-item">v${template.version}</span>
            </div>
          </div>
        `)}
      </div>
    `
  }

  renderPagination() {
    const totalPages = Math.ceil(this.totalCount / this.pageSize)
    return html`
      <div class="pagination">
        <span class="pagination-info">
          显示 ${(this.page - 1) * this.pageSize + 1} - ${Math.min(this.page * this.pageSize, this.totalCount)}，共 ${this.totalCount} 条
        </span>
        <div class="pagination-buttons">
          <button class="page-btn" ?disabled=${this.page <= 1} @click=${() => this.handlePageChange(this.page - 1)}>上一页</button>
          <button class="page-btn" ?disabled=${this.page >= totalPages} @click=${() => this.handlePageChange(this.page + 1)}>下一页</button>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'templates-page': TemplatesPage
  }
}
