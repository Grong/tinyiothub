import { LitElement, html} from 'lit'
import { customElement, state } from 'lit/decorators.js'
import {
  $currentWorkspaceId,
  $workspaces,
  selectWorkspace,
  setWorkspaces,
  type Workspace,
} from '../stores/workspace-store'
import { workspaceApi } from '../services/workspace'

@customElement('workspace-picker')
export class WorkspacePicker extends LitElement {
  createRenderRoot() { return this }
  @state() private workspaces: Workspace[] = []
  @state() private currentId: string | null = null
  @state() private loading = true
  @state() private showCreate = false
  @state() private newName = ''
  @state() private creating = false
  @state() private deleting: string | null = null
  @state() private error = ''

  private _unsubWorkspaces: (() => void) | null = null
  private _unsubCurrentId: (() => void) | null = null

  

  connectedCallback() {
    super.connectedCallback()
    this._unsubWorkspaces = $workspaces.subscribe(ws => {
      this.workspaces = [...ws]
    })
    this._unsubCurrentId = $currentWorkspaceId.subscribe(id => {
      this.currentId = id
    })
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    this._unsubWorkspaces?.()
    this._unsubCurrentId?.()
  }

  async firstUpdated() {
    await this.loadWorkspaces()
  }

  private async loadWorkspaces() {
    try {
      const res = await workspaceApi.list()
      if (res.result) {
        setWorkspaces(res.result)
      }
    } catch (e) {
      console.warn('[workspace-picker] Failed to load workspaces:', e)
    } finally {
      this.loading = false
    }
  }

  private handleChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value
    selectWorkspace(value || null)
  }

  private toggleCreate() {
    this.showCreate = !this.showCreate
    this.error = ''
    this.newName = ''
  }

  private async handleCreate() {
    const name = this.newName.trim()
    if (!name) return
    this.creating = true
    this.error = ''
    try {
      const res = await workspaceApi.create({ name })
      if (res.result) {
        await this.loadWorkspaces()
        selectWorkspace(res.result.id)
        this.showCreate = false
        this.newName = ''
      }
    } catch (e: any) {
      this.error = e?.message || '创建工作空间失败'
    } finally {
      this.creating = false
    }
  }

  private async handleDelete(ws: Workspace, e: Event) {
    e.stopPropagation()
    if (!confirm(`确定删除工作空间「${ws.name}」？`)) return
    this.deleting = ws.id
    this.error = ''
    try {
      await workspaceApi.delete(ws.id)
      if (this.currentId === ws.id) {
        selectWorkspace(null)
      }
      await this.loadWorkspaces()
    } catch (e: any) {
      this.error = e?.message || '删除工作空间失败'
    } finally {
      this.deleting = null
    }
  }

  private handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      this.handleCreate()
    } else if (e.key === 'Escape') {
      this.toggleCreate()
    }
  }

  private renderEmptyState() {
    return html`
      <div class="picker">
        <div class="picker-header">
          <span class="picker-label">工作空间</span>
        </div>
        ${this.showCreate ? html`
          <div class="create-form">
            <input
              type="text"
              placeholder="工作空间名称"
              .value=${this.newName}
              @input=${(e: Event) => { this.newName = (e.target as HTMLInputElement).value }}
              @keydown=${this.handleKeydown}
              ?disabled=${this.creating}
              autofocus
            />
            <div class="form-actions">
              <button class="btn btn-ghost" @click=${this.toggleCreate} ?disabled=${this.creating}>取消</button>
              <button class="btn btn-primary" @click=${this.handleCreate} ?disabled=${this.creating || !this.newName.trim()}>
                ${this.creating ? '创建中...' : '创建'}
              </button>
            </div>
          </div>
        ` : html`
          <button class="btn-create-empty" @click=${this.toggleCreate}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 4.5v15m7.5-7.5h-15"/>
            </svg>
            创建工作空间
          </button>
        `}
        ${this.error ? html`<div class="error-msg">${this.error}</div>` : ''}
      </div>
    `
  }

  render() {
    if (this.loading) return html``

    if (this.workspaces.length === 0) {
      return this.renderEmptyState()
    }

    return html`
      <div class="picker">
        <div class="picker-header">
          <span class="picker-label">工作空间</span>
          <div class="picker-actions">
            <button class="icon-btn" @click=${this.toggleCreate} title="${this.showCreate ? '取消' : '新建工作空间'}">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                ${this.showCreate
                  ? html`<path d="M6 18L18 6M6 6l12 12"/>`
                  : html`<path d="M12 4.5v15m7.5-7.5h-15"/>`}
              </svg>
            </button>
          </div>
        </div>

        <div class="workspace-row">
          <select @change=${this.handleChange} .value=${this.currentId ?? ''}>
            <option value="">全部工作空间</option>
            ${this.workspaces.map(
              w => html`<option value=${w.id}>${w.name}</option>`
            )}
          </select>
          ${this.currentId ? html`
            <button
              class="icon-btn delete-btn"
              title="删除当前工作空间"
              ?disabled=${this.deleting === this.currentId}
              @click=${(e: Event) => {
                const ws = this.workspaces.find(w => w.id === this.currentId)
                if (ws) this.handleDelete(ws, e)
              }}
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0"/>
              </svg>
            </button>
          ` : ''}
        </div>

        ${this.showCreate ? html`
          <div class="create-form">
            <input
              type="text"
              placeholder="工作空间名称"
              .value=${this.newName}
              @input=${(e: Event) => { this.newName = (e.target as HTMLInputElement).value }}
              @keydown=${this.handleKeydown}
              ?disabled=${this.creating}
              autofocus
            />
            <div class="form-actions">
              <button class="btn btn-ghost" @click=${this.toggleCreate} ?disabled=${this.creating}>取消</button>
              <button class="btn btn-primary" @click=${this.handleCreate} ?disabled=${this.creating || !this.newName.trim()}>
                ${this.creating ? '创建中...' : '创建'}
              </button>
            </div>
          </div>
        ` : ''}

        ${this.error ? html`<div class="error-msg">${this.error}</div>` : ''}
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'workspace-picker': WorkspacePicker
  }
}
