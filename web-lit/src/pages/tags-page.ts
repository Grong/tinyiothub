import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { apiGet, apiPost, apiDelete } from '../lib/api-client'
import type { PaginatedResponse } from '../lib/api-client'

interface Tag {
  id: string
  name: string
  color: string
  description?: string
  deviceCount: number
  createdAt: string
}

@customElement('tags-page')
export class TagsPage extends LitElement {
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

    .btn-primary {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 10px 16px;
      background: var(--accent);
      color: var(--accent-foreground);
      border: none;
      border-radius: var(--radius-md);
      font-size: 13px;
      font-weight: 600;
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .btn-primary:hover {
      background: var(--accent-hover);
    }

    /* Tags grid */
    .tags-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
      gap: 16px;
    }

    .tag-card {
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      padding: 20px;
      transition: border-color var(--duration-normal) ease;
    }

    .tag-card:hover {
      border-color: var(--border-strong);
    }

    .tag-header {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      margin-bottom: 12px;
    }

    .tag-name {
      display: flex;
      align-items: center;
      gap: 10px;
    }

    .tag-dot {
      width: 12px;
      height: 12px;
      border-radius: 50%;
    }

    .tag-name-text {
      font-size: 15px;
      font-weight: 600;
      color: var(--text-strong);
    }

    .tag-actions {
      display: flex;
      gap: 8px;
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
      transition: background var(--duration-fast) ease, color var(--duration-fast) ease;
    }

    .action-btn:hover {
      background: var(--bg-hover);
      color: var(--text);
    }

    .action-btn.danger:hover {
      background: var(--danger-subtle);
      color: var(--danger);
    }

    .action-btn svg {
      width: 14px;
      height: 14px;
    }

    .tag-description {
      font-size: 13px;
      color: var(--muted);
      margin: 0 0 12px;
      line-height: 1.5;
    }

    .tag-meta {
      display: flex;
      align-items: center;
      gap: 16px;
      font-size: 12px;
      color: var(--muted);
    }

    .tag-count {
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

    /* Modal */
    .modal-overlay {
      position: fixed;
      inset: 0;
      background: rgba(0, 0, 0, 0.5);
      display: flex;
      align-items: center;
      justify-content: center;
      z-index: 100;
    }

    .modal {
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      width: 100%;
      max-width: 480px;
      animation: scale-in 0.2s ease;
    }

    @keyframes scale-in {
      from {
        opacity: 0;
        transform: scale(0.95);
      }
      to {
        opacity: 1;
        transform: scale(1);
      }
    }

    .modal-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 16px 20px;
      border-bottom: 1px solid var(--border);
    }

    .modal-title {
      font-size: 16px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0;
    }

    .modal-close {
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

    .modal-close:hover {
      background: var(--bg-hover);
      color: var(--text);
    }

    .modal-body {
      padding: 20px;
    }

    .form-group {
      margin-bottom: 16px;
    }

    .form-label {
      display: block;
      font-size: 13px;
      font-weight: 500;
      color: var(--text);
      margin-bottom: 8px;
    }

    .form-input {
      width: 100%;
      padding: 10px 14px;
      border: 1px solid var(--input);
      border-radius: var(--radius-md);
      background: var(--bg);
      color: var(--text);
      font-size: 14px;
    }

    .form-input:focus {
      outline: none;
      border-color: var(--accent);
    }

    .color-options {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
    }

    .color-option {
      width: 32px;
      height: 32px;
      border-radius: 50%;
      border: 2px solid transparent;
      cursor: pointer;
      transition: transform var(--duration-fast) ease, border-color var(--duration-fast) ease;
    }

    .color-option:hover {
      transform: scale(1.1);
    }

    .color-option.selected {
      border-color: var(--text);
    }

    .modal-footer {
      display: flex;
      justify-content: flex-end;
      gap: 12px;
      padding: 16px 20px;
      border-top: 1px solid var(--border);
    }

    .btn {
      padding: 10px 16px;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      font-weight: 500;
      cursor: pointer;
    }

    .btn:hover {
      background: var(--bg-hover);
    }

    .btn-primary {
      background: var(--accent);
      border-color: var(--accent);
      color: var(--accent-foreground);
    }

    .btn-primary:hover {
      background: var(--accent-hover);
    }
  `

  @state() tags: Tag[] = []
  @state() loading = true
  @state() showModal = false
  @state() editingTag: Tag | null = null

  // Form state
  @state() formName = ''
  @state() formDescription = ''
  @state() formColor = '#3b82f6'

  readonly colorOptions = [
    '#3b82f6', '#22c55e', '#f59e0b', '#ef4444',
    '#8b5cf6', '#ec4899', '#06b6d4', '#f97316'
  ]

  async connectedCallback() {
    super.connectedCallback()
    await this.loadTags()
  }

  async loadTags() {
    this.loading = true
    try {
      const response = await apiGet<PaginatedResponse<Tag>>('tags')
      if (response.result) {
        this.tags = response.result.data || []
      }
    } catch (err: any) {
      console.error('Failed to load tags:', err)
    } finally {
      this.loading = false
    }
  }

  openCreateModal() {
    this.editingTag = null
    this.formName = ''
    this.formDescription = ''
    this.formColor = '#3b82f6'
    this.showModal = true
  }

  openEditModal(tag: Tag) {
    this.editingTag = tag
    this.formName = tag.name
    this.formDescription = tag.description || ''
    this.formColor = tag.color
    this.showModal = true
  }

  closeModal() {
    this.showModal = false
    this.editingTag = null
  }

  async saveTag() {
    if (!this.formName.trim()) return

    try {
      if (this.editingTag) {
        await apiPost<Tag>(`tags/${this.editingTag.id}`, {
          name: this.formName,
          description: this.formDescription,
          color: this.formColor,
        })
      } else {
        await apiPost<Tag>('tags', {
          name: this.formName,
          description: this.formDescription,
          color: this.formColor,
        })
      }
      this.closeModal()
      await this.loadTags()
    } catch (err: any) {
      alert(err.message || '保存失败')
    }
  }

  async deleteTag(tag: Tag) {
    if (!confirm(`确定要删除标签 "${tag.name}" 吗？`)) return

    try {
      await apiDelete<void>(`tags/${tag.id}`)
      await this.loadTags()
    } catch (err: any) {
      alert(err.message || '删除失败')
    }
  }

  render() {
    return html`
      <div class="page-header">
        <h1 class="page-title">标签管理</h1>
        <button class="btn-primary" @click=${this.openCreateModal}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15"/>
          </svg>
          创建标签
        </button>
      </div>

      ${this.loading ? html`
        <div style="text-align: center; padding: 64px; color: var(--muted);">加载中...</div>
      ` : this.tags.length === 0 ? html`
        <div class="empty-state">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M9.568 3H5.25A2.25 2.25 0 003 5.25v4.318c0 .597.237 1.17.659 1.591l9.581 9.581c.699.699 1.78.872 2.607.33a18.095 18.095 0 005.223-5.223c.542-.827.369-1.908-.33-2.607L11.16 3.66A2.25 2.25 0 009.568 3z"/>
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 6h.008v.008H6V6z"/>
          </svg>
          <h3>暂无标签</h3>
          <p>点击"创建标签"添加您的第一个标签</p>
        </div>
      ` : html`
        <div class="tags-grid">
          ${this.tags.map(tag => html`
            <div class="tag-card">
              <div class="tag-header">
                <div class="tag-name">
                  <span class="tag-dot" style="background: ${tag.color}"></span>
                  <span class="tag-name-text">${tag.name}</span>
                </div>
                <div class="tag-actions">
                  <button class="action-btn" title="编辑" @click=${() => this.openEditModal(tag)}>
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125"/>
                    </svg>
                  </button>
                  <button class="action-btn danger" title="删除" @click=${() => this.deleteTag(tag)}>
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0"/>
                    </svg>
                  </button>
                </div>
              </div>
              ${tag.description ? html`<p class="tag-description">${tag.description}</p>` : ''}
              <div class="tag-meta">
                <span class="tag-count">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M9 17.25v1.007a3 3 0 01-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0115 18.257V17.25m6-12V15a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 15V5.25m18 0A2.25 2.25 0 0018.75 3H5.25A2.25 2.25 0 003 5.25m18 0V12a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 12V5.25"/>
                  </svg>
                  ${tag.deviceCount} 设备
                </span>
              </div>
            </div>
          `)}
        </div>
      `}

      ${this.showModal ? html`
        <div class="modal-overlay" @click=${(e: Event) => e.target === e.currentTarget && this.closeModal()}>
          <div class="modal">
            <div class="modal-header">
              <h3 class="modal-title">${this.editingTag ? '编辑标签' : '创建标签'}</h3>
              <button class="modal-close" @click=${this.closeModal}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
                </svg>
              </button>
            </div>
            <div class="modal-body">
              <div class="form-group">
                <label class="form-label">名称</label>
                <input
                  type="text"
                  class="form-input"
                  placeholder="请输入标签名称"
                  .value=${this.formName}
                  @input=${(e: InputEvent) => this.formName = (e.target as HTMLInputElement).value}
                />
              </div>
              <div class="form-group">
                <label class="form-label">描述</label>
                <input
                  type="text"
                  class="form-input"
                  placeholder="请输入标签描述（可选）"
                  .value=${this.formDescription}
                  @input=${(e: InputEvent) => this.formDescription = (e.target as HTMLInputElement).value}
                />
              </div>
              <div class="form-group">
                <label class="form-label">颜色</label>
                <div class="color-options">
                  ${this.colorOptions.map(color => html`
                    <div
                      class="color-option ${this.formColor === color ? 'selected' : ''}"
                      style="background: ${color}"
                      @click=${() => this.formColor = color}
                    ></div>
                  `)}
                </div>
              </div>
            </div>
            <div class="modal-footer">
              <button class="btn" @click=${this.closeModal}>取消</button>
              <button class="btn btn-primary" @click=${() => this.saveTag()}>保存</button>
            </div>
          </div>
        </div>
      ` : ''}
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'tags-page': TagsPage
  }
}
