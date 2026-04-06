// web-lit/src/components/tag-filter.ts
import { LitElement, html} from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { tagApi, type Tag } from '../services/tags'

@customElement('tag-filter')
export class TagFilter extends LitElement {
  createRenderRoot() { return this }
  

  @property({ type: String }) value = ''
  @property({ type: String }) placeholder = '选择标签'
  @state() tags: Tag[] = []
  @state() open = false
  @state() loading = true

  async connectedCallback() {
    super.connectedCallback()
    await this.loadTags()
  }

  async loadTags() {
    try {
      const response = await tagApi.getTags('device')
      if (response.result) {
        this.tags = response.result
      }
    } catch {
      this.tags = []
    } finally {
      this.loading = false
    }
  }

  toggleDropdown() { this.open = !this.open }

  selectTag(tag: Tag) {
    this.value = tag.id
    this.open = false
    this.dispatchEvent(new CustomEvent('change', { detail: tag.id }))
  }

  clearFilter() {
    this.value = ''
    this.open = false
    this.dispatchEvent(new CustomEvent('change', { detail: '' }))
  }

  render() {
    const selectedTag = this.tags.find(t => t.id === this.value)
    return html`
      <div class="filter-container">
        <button class="filter-btn ${this.value ? 'active' : ''}" @click=${this.toggleDropdown}>
          <span>${selectedTag?.name || this.placeholder}</span>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M6 9l6 6 6-6"/>
          </svg>
        </button>
        ${this.open ? html`
          <div class="dropdown">
            <div class="tag-item ${!this.value ? 'selected' : ''}" @click=${this.clearFilter}>
              <span class="tag-color" style="background: var(--muted)"></span>
              <span class="tag-name">全部</span>
            </div>
            ${this.tags.map(tag => html`
              <div class="tag-item ${tag.id === this.value ? 'selected' : ''}" @click=${() => this.selectTag(tag)}>
                <span class="tag-color" style="background: ${tag.color}"></span>
                <span class="tag-name">${tag.name}</span>
              </div>
            `)}
          </div>
        ` : ''}
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'tag-filter': TagFilter }
}
