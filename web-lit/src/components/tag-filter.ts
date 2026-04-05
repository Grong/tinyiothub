// web-lit/src/components/tag-filter.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { tagApi, type Tag } from '../services/tags'

@customElement('tag-filter')
export class TagFilter extends LitElement {
  static styles = css`
    :host { display: inline-flex; }
    .filter-container {
      position: relative;
    }
    .filter-btn {
      display: flex;
      align-items: center;
      gap: 6px;
      padding: 8px 12px;
      background: var(--card);
      border: none;
      border-radius: var(--radius-md);
      color: var(--text);
      font-size: 13px;
      cursor: pointer;
      box-shadow: var(--glass-shadow-sm);
    }
    .filter-btn:hover { background: var(--bg-hover); }
    .filter-btn.active { background: var(--accent-subtle); color: var(--accent); }
    .dropdown {
      position: absolute;
      top: 100%;
      left: 0;
      margin-top: 4px;
      min-width: 200px;
      background: var(--card);
      border-radius: var(--radius-md);
      box-shadow: var(--shadow-lg);
      z-index: 100;
      padding: 8px;
    }
    .tag-item {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px;
      border-radius: var(--radius-sm);
      cursor: pointer;
    }
    .tag-item:hover { background: var(--bg-hover); }
    .tag-item.selected { background: var(--accent-subtle); }
    .tag-color {
      width: 8px;
      height: 8px;
      border-radius: 50%;
    }
    .tag-name { font-size: 13px; color: var(--text); }
    .tag-count {
      margin-left: auto;
      font-size: 11px;
      color: var(--muted);
    }
  `

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
