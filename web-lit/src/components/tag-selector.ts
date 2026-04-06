// web-lit/src/components/tag-selector.ts
import { LitElement, html} from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { tagApi, type Tag } from '../services/tags'

@customElement('tag-selector')
export class TagSelector extends LitElement {
  createRenderRoot() { return this }
  

  @property({ type: String }) targetId = ''
  @property({ type: Array }) initialTags: Tag[] = []
  @property({ type: Function }) onChange!: () => void

  @state() private open = false
  @state() private allTags: Tag[] = []
  @state() private selectedIds: Set<string> = new Set()
  @state() private keyword = ''
  @state() private loading = true
  @state() private saving = false

  private get originalIds(): Set<string> {
    return new Set(this.initialTags.map(t => t.id))
  }

  private get filteredTags(): Tag[] {
    if (!this.keyword) return this.allTags
    return this.allTags.filter(t => t.name.toLowerCase().includes(this.keyword.toLowerCase()))
  }

  private get showCreate(): boolean {
    if (!this.keyword) return false
    return !this.allTags.some(t => t.name.toLowerCase() === this.keyword.toLowerCase())
  }

  async connectedCallback() {
    super.connectedCallback()
    this.selectedIds = new Set(this.initialTags.map(t => t.id))
    await this.loadAllTags()
  }

  updated(changed: Map<string, unknown>) {
    if (changed.has('initialTags')) {
      this.selectedIds = new Set(this.initialTags.map(t => t.id))
    }
  }

  private async loadAllTags() {
    try {
      const res = await tagApi.getTags('device')
      this.allTags = res.result || []
    } catch {
      this.allTags = []
    } finally {
      this.loading = false
    }
  }

  private toggleDropdown() {
    if (this.open) {
      this.closeAndSave()
    } else {
      this.selectedIds = new Set(this.initialTags.map(t => t.id))
      this.keyword = ''
      this.open = true
    }
  }

  private toggleTag(tag: Tag) {
    const next = new Set(this.selectedIds)
    if (next.has(tag.id)) {
      next.delete(tag.id)
    } else {
      next.add(tag.id)
    }
    this.selectedIds = next
  }

  private async closeAndSave() {
    this.open = false
    if (this.saving) return

    const original = this.originalIds
    const addIds: string[] = []
    const removeIds: string[] = []

    for (const id of this.selectedIds) {
      if (!original.has(id)) addIds.push(id)
    }
    for (const id of original) {
      if (!this.selectedIds.has(id)) removeIds.push(id)
    }

    if (addIds.length === 0 && removeIds.length === 0) return

    this.saving = true
    try {
      await Promise.all([
        ...addIds.map(id => tagApi.bindTag(id, this.targetId)),
        ...removeIds.map(id => tagApi.unbindTag(id, this.targetId)),
      ])
      if (this.onChange) this.onChange()
    } catch (e) {
      console.error('Failed to save tag bindings:', e)
    } finally {
      this.saving = false
    }
  }

  private async createTag() {
    const name = this.keyword.trim()
    if (!name) return
    try {
      const res = await tagApi.createTag(name, 'device')
      if (res.result) {
        this.allTags = [...this.allTags, res.result]
        const next = new Set(this.selectedIds)
        next.add(res.result.id)
        this.selectedIds = next
        this.keyword = ''
      }
    } catch (e) {
      console.error('Failed to create tag:', e)
    }
  }

  private handleBackdropClick(e: Event) {
    e.stopPropagation()
    this.closeAndSave()
  }

  render() {
    return html`
      <div class="trigger" @click=${this.toggleDropdown}>
        ${this.initialTags.length > 0
          ? this.initialTags.map(t => html`<span class="pill">${t.name}</span>`)
          : html`<span class="empty-pill">添加标签</span>`
        }
        <button class="add-btn" @click=${(e: Event) => { e.stopPropagation(); this.toggleDropdown() }}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
            <path d="M12 5v14M5 12h14"/>
          </svg>
        </button>
      </div>
      ${this.open ? html`
        <div class="dropdown" @click=${(e: Event) => e.stopPropagation()}>
          <input
            class="search-input"
            type="text"
            placeholder="搜索或创建标签..."
            .value=${this.keyword}
            @input=${(e: Event) => { this.keyword = (e.target as HTMLInputElement).value }}
            @keydown=${(e: KeyboardEvent) => { if (e.key === 'Enter' && this.showCreate) this.createTag() }}
          />
          <div class="tag-list">
            ${this.loading
              ? html`<div class="loading-row">加载中...</div>`
              : this.filteredTags.length === 0 && !this.showCreate
                ? html`<div class="empty-row">暂无标签</div>`
                : this.filteredTags.map(tag => html`
                  <div class="tag-row" @click=${() => this.toggleTag(tag)}>
                    <div class="checkbox ${this.selectedIds.has(tag.id) ? 'checked' : ''}">
                      <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="3">
                        <polyline points="20 6 9 17 4 12"/>
                      </svg>
                    </div>
                    <span class="tag-row-name">${tag.name}</span>
                  </div>
                `)
            }
          </div>
          ${this.showCreate ? html`
            <div class="create-row" @click=${() => this.createTag()}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M12 5v14M5 12h14"/>
              </svg>
              <span>创建 "${this.keyword}"</span>
            </div>
          ` : ''}
        </div>
        <div style="position:fixed;inset:0;z-index:199" @click=${this.handleBackdropClick}></div>
      ` : ''}
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'tag-selector': TagSelector }
}
