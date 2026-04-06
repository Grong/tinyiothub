/**
 * BasePage - Unified page component base class
 *
 * All pages should extend this class to get:
 * - Standardized loading/error/empty states
 * - Common navigation actions
 * - Consistent styling patterns
 * - Auth state from app context
 */

import { LitElement, html, TemplateResult } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { navigate } from '../lib/navigate'

@customElement('base-page')
export class BasePage extends LitElement {
  createRenderRoot() { return this }
  // Page metadata
  @property({ type: String }) pageTitle = ''
  @property({ type: Boolean }) showPageHeader = true

  // Loading/error/empty state
  @state() protected loading = false
  @state() protected error: string | null = null
  @state() protected empty = false
  @state() protected emptyMessage = '暂无数据'

  // Page header actions (can be overridden by child pages)
  protected headerActions: TemplateResult | null = null

  

  // Navigation helper — use navigate() from lib/navigate directly
  protected goTo(route: string) {
    navigate(route)
  }

  // Loading state helpers
  protected setLoading(loading: boolean) {
    this.loading = loading
    if (loading) {
      this.error = null
    }
  }

  protected setError(error: string | null) {
    this.error = error
    this.loading = false
  }

  protected setEmpty(empty: boolean, message = '暂无数据') {
    this.empty = empty
    this.emptyMessage = message
    this.loading = false
  }

  // Render helpers
  protected renderLoading(): TemplateResult {
    return html`
      <div class="loading-state">
        <div class="spinner"></div>
      </div>
    `
  }

  protected renderError(message?: string): TemplateResult {
    return html`
      <div class="error-state">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z"/>
        </svg>
        <h3>加载失败</h3>
        <p>${message || this.error || '未知错误'}</p>
        <button class="btn btn-primary" @click=${this.onRetry}>重试</button>
      </div>
    `
  }

  protected renderEmpty(message?: string): TemplateResult {
    return html`
      <div class="empty-state">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
        </svg>
        <h3>${message || this.emptyMessage}</h3>
      </div>
    `
  }

  protected renderPageHeader(title: string, actions?: TemplateResult): TemplateResult {
    if (!this.showPageHeader) return html``

    return html`
      <div class="page-header">
        <h1 class="page-title">${title}</h1>
        ${actions ? html`<div class="header-actions">${actions}</div>` : ''}
      </div>
    `
  }

  // Override in child pages to handle retry
  protected onRetry() {
    this.loading = false
    this.error = null
    this.loadData()
  }

  // Override in child pages to provide data loading logic
  protected loadData() {
    // Subclasses should override this method
  }
}
