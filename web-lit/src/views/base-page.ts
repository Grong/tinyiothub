/**
 * BasePage - Unified page component base class
 *
 * All pages should extend this class to get:
 * - Standardized loading/error/empty states
 * - Common navigation actions
 * - Consistent styling patterns
 * - Auth state from app context
 */

import { LitElement, html, css, TemplateResult } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { navigate } from '../lib/navigate'

@customElement('base-page')
export class BasePage extends LitElement {
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

  static styles = css`
    base-page {
      display: block;
      padding: 0;
      background: var(--bg);
    }

    /* Page header */
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

    /* Buttons */
    .btn {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 10px 16px;
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      font-weight: 500;
      cursor: pointer;
      box-shadow: var(--glass-shadow-sm);
      transition: box-shadow var(--duration-fast) ease, background var(--duration-fast) ease;
    }

    .btn:hover {
      background: var(--bg-hover);
      box-shadow: var(--glass-shadow);
    }

    .btn-primary {
      background: var(--accent);
      color: var(--accent-foreground);
      box-shadow: 0 1px 3px var(--accent-subtle);
    }

    .btn-primary:hover {
      background: var(--accent-hover);
      box-shadow: 0 2px 12px var(--accent-glow);
    }

    /* Loading state */
    .loading-state {
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

    /* Error state */
    .error-state {
      text-align: center;
      padding: 64px 24px;
      color: var(--muted);
    }

    .error-state svg {
      width: 64px;
      height: 64px;
      margin-bottom: 16px;
      opacity: 0.5;
    }

    .error-state h3 {
      font-size: 16px;
      font-weight: 600;
      color: var(--text);
      margin: 0 0 8px;
    }

    .error-state p {
      font-size: 14px;
      margin: 0 0 20px;
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

    /* Card styles */
    .card {
      background: var(--card);
      border-radius: var(--radius-lg);
      overflow: hidden;
      box-shadow: var(--glass-shadow-sm);
    }

    .card-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 16px 20px;
      box-shadow: 0 1px 0 var(--card-highlight);
    }

    .card-title {
      font-size: 15px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0;
    }

    .card-body {
      padding: 20px;
    }

    /* Form styles */
    .form-group {
      margin-bottom: 16px;
    }

    .form-label {
      display: block;
      font-size: 13px;
      font-weight: 500;
      color: var(--text);
      margin-bottom: 6px;
    }

    .form-input {
      width: 100%;
      padding: 10px 14px;
      border: none;
      border-bottom: 1px solid var(--input);
      border-radius: var(--radius-md) var(--radius-md) 0 0;
      background: var(--card);
      color: var(--text);
      font-size: 14px;
      transition: border-color var(--duration-fast) ease;
    }

    .form-input:focus {
      outline: none;
      border-bottom-color: var(--accent);
    }

    .form-input::placeholder {
      color: var(--muted);
    }

    /* Filter bar */
    .filter-bar {
      display: flex;
      gap: 12px;
      margin-bottom: 20px;
      flex-wrap: wrap;
    }

    .filter-select {
      padding: 10px 14px;
      border: none;
      border-bottom: 1px solid var(--input);
      border-radius: var(--radius-md) var(--radius-md) 0 0;
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      cursor: pointer;
    }
  `

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
