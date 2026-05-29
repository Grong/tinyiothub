import { LitElement, html, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import {
  workspaceResourceApi,
  type WorkspaceResource,
  type ResourceSearchResult,
} from '../../api/workspace-resources.js';
import { success, error as toastError } from '../components/toast.js';
import '../../styles/views/workspace-resources.css';

const RESOURCE_TYPE_CONFIG: Record<string, { label: string; color: string }> = {
  scene: { label: '3D 场景', color: '#00d4aa' },
  device_model: { label: '设备模型', color: '#3b82f6' },
  image: { label: '图片', color: '#f59e0b' },
  document: { label: '文档', color: '#8b5cf6' },
};

const TYPE_OPTIONS = [
  { value: '', label: '全部' },
  { value: 'scene', label: '3D 场景' },
  { value: 'device_model', label: '设备模型' },
  { value: 'image', label: '图片' },
  { value: 'document', label: '文档' },
];

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString('zh-CN', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  } catch { return iso; }
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function parseTags(tags: string[] | string): string[] {
  if (Array.isArray(tags)) return tags;
  try { const p = JSON.parse(tags); return Array.isArray(p) ? p : []; } catch { return []; }
}

function extractTagsFromFilename(fileName: string): string[] {
  const name = fileName.replace(/\.[^.]+$/, '');
  const parts = name.split(/[-_\s.]+/).filter(Boolean);
  const words = parts.flatMap(p => p.split(/(?<=[a-z])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])|(?<=[一-龥])(?=[a-zA-Z])|(?<=[a-zA-Z])(?=[一-龥])/));
  return [...new Set(words.filter(w => w.length > 1 && !/^\d+$/.test(w)))];
}

@customElement('view-workspace-resources')
export class ViewWorkspaceResources extends LitElement {
  createRenderRoot() { return this; }

  @state() loading = true;
  @state() error = '';
  @state() resources: WorkspaceResource[] = [];
  @state() total = 0;
  @state() page = 1;
  @state() pageSize = 12;
  @state() filterType = '';
  @state() searchQuery = '';
  @state() searchMode = false;
  @state() searchResults: ResourceSearchResult[] = [];

  @state() showCreateModal = false;
  @state() showEditModal = false;
  @state() showDeleteModal = false;
  @state() showPreviewModal = false;
  @state() selectedResource: WorkspaceResource | null = null;

  @state() formName = '';
  @state() formType = 'scene';
  @state() formDescription = '';
  @state() formTagsList: string[] = [];
  @state() formMetadata = '';
  @state() formSubmitting = false;
  @state() formNameError = '';

  // File upload
  @state() formFile: File | null = null;
  @state() uploadProgress = 0;
  @state() uploadDragover = false;
  @state() aiGeneratingTags = false;

  private lastFocusedElement: Element | null = null;
  private boundKeydown = this.onKeydown.bind(this);
  private _searchTimer: ReturnType<typeof setTimeout> | null = null;
  private _lastDeleted: WorkspaceResource | null = null;

  connectedCallback() {
    super.connectedCallback();
    this.loadResources();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.removeModalListeners();
    if (this._searchTimer) clearTimeout(this._searchTimer);
  }

  // ── Modal keyboard / focus ──

  private addModalListeners() { document.addEventListener('keydown', this.boundKeydown); }
  private removeModalListeners() { document.removeEventListener('keydown', this.boundKeydown); }

  private onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (this.showCreateModal) this.closeCreateModal();
      else if (this.showEditModal) this.closeEditModal();
      else if (this.showDeleteModal) this.closeDeleteModal();
      else if (this.showPreviewModal) this.closePreviewModal();
    }
    if (e.key === 'Tab') this.trapFocus(e);
  }

  private trapFocus(e: KeyboardEvent) {
    const modal = this.renderRoot.querySelector('.modal-box') as HTMLElement | null;
    if (!modal) return;
    const focusable = modal.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])');
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  }

  private saveFocus() { this.lastFocusedElement = document.activeElement; }
  private restoreFocus() {
    if (this.lastFocusedElement && 'focus' in this.lastFocusedElement)
      (this.lastFocusedElement as HTMLElement).focus();
  }

  private openModal(type: 'create' | 'edit' | 'delete' | 'preview', resource?: WorkspaceResource) {
    this.saveFocus();
    this.addModalListeners();
    if (type === 'create') {
      this.formName = ''; this.formType = 'scene'; this.formDescription = '';
      this.formTagsList = []; this.formMetadata = ''; this.formNameError = '';
      this.formFile = null; this.uploadProgress = 0;
      this.showCreateModal = true;
    }
    if (type === 'edit') {
      this.selectedResource = resource ?? null;
      if (resource) {
        this.formName = resource.name; this.formType = resource.resourceType;
        this.formDescription = resource.description ?? '';
        this.formTagsList = parseTags(resource.tags);
        this.formMetadata = resource.metadata ?? '';
      }
      this.formNameError = ''; this.formFile = null; this.uploadProgress = 0;
      this.showEditModal = true;
    }
    if (type === 'delete') { this.selectedResource = resource ?? null; this.showDeleteModal = true; }
    if (type === 'preview') { this.selectedResource = resource ?? null; this.showPreviewModal = true; }
    requestAnimationFrame(() => {
      const modal = this.renderRoot.querySelector('.modal-box') as HTMLElement | null;
      const focusable = modal?.querySelector<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])');
      focusable?.focus();
    });
  }

  // ── Data ──

  private async loadResources() {
    this.loading = true; this.error = '';
    try {
      const res = await workspaceResourceApi.listResources({
        resourceType: this.filterType || undefined, page: this.page, pageSize: this.pageSize });
      this.resources = res.result?.data ?? [];
      this.total = res.result?.pagination?.totalCount ?? 0;
      this.searchMode = false;
    } catch (e: any) { this.error = e.message || '加载资源失败'; }
    finally { this.loading = false; }
  }

  private async doSearch() {
    if (!this.searchQuery.trim()) { this.loadResources(); return; }
    this.loading = true; this.error = '';
    try {
      const results = await workspaceResourceApi.searchResources({
        q: this.searchQuery.trim(), type: this.filterType || undefined, limit: 20 });
      this.searchResults = results.result ?? [];
      this.searchMode = true;
    } catch (e: any) { this.error = e.message || '搜索失败'; }
    finally { this.loading = false; }
  }

  private async deleteResource() {
    if (!this.selectedResource) return;
    const resource = this.selectedResource;
    this.formSubmitting = true;
    try {
      await workspaceResourceApi.deleteResource(resource.id);
      this._lastDeleted = resource;
      success('资源已删除', { label: '撤销', onClick: () => this.undoDelete() });
      this.closeDeleteModal(); this.loadResources();
    } catch (e: any) { toastError(e.message || '删除失败'); }
    finally { this.formSubmitting = false; }
  }

  private async undoDelete() {
    if (!this._lastDeleted) return;
    const resource = this._lastDeleted;
    this._lastDeleted = null;
    try {
      await workspaceResourceApi.createResource({
        resourceType: resource.resourceType, name: resource.name,
        description: resource.description ?? undefined, tags: resource.tags,
        metadata: resource.metadata ?? undefined });
      success('已撤销删除'); this.loadResources();
    } catch (e: any) { this._lastDeleted = resource; toastError(e.message || '撤销失败'); }
  }

  private async submitCreate() {
    if (!this.formName.trim()) { this.formNameError = '请输入资源名称'; return; }
    this.formNameError = ''; this.formSubmitting = true;
    try {
      let filePath: string | undefined;
      if (this.formFile) {
        const uploadRes = await workspaceResourceApi.uploadFile(this.formFile, (pct) => {
          this.uploadProgress = pct;
        });
        filePath = uploadRes.result?.filePath;
      }
      await workspaceResourceApi.createResource({
        resourceType: this.formType, name: this.formName.trim(),
        description: this.formDescription.trim() || undefined,
        tags: this.formTagsList, metadata: this.formMetadata.trim() || undefined,
        filePath,
      });
      success('资源创建成功'); this.closeCreateModal(); this.loadResources();
    } catch (e: any) { toastError(e.message || '创建失败'); }
    finally { this.formSubmitting = false; this.uploadProgress = 0; }
  }

  private async submitEdit() {
    if (!this.selectedResource || !this.formName.trim()) { this.formNameError = '请输入资源名称'; return; }
    this.formNameError = ''; this.formSubmitting = true;
    try {
      await workspaceResourceApi.updateResource(this.selectedResource.id, {
        name: this.formName.trim(), description: this.formDescription.trim() || undefined,
        tags: this.formTagsList, metadata: this.formMetadata.trim() || undefined,
      });
      success('资源更新成功'); this.closeEditModal(); this.loadResources();
    } catch (e: any) { toastError(e.message || '更新失败'); }
    finally { this.formSubmitting = false; }
  }

  private closeCreateModal() { this.showCreateModal = false; this.formSubmitting = false; this.formNameError = ''; this.removeModalListeners(); this.restoreFocus(); }
  private closeEditModal() { this.showEditModal = false; this.selectedResource = null; this.formSubmitting = false; this.formNameError = ''; this.removeModalListeners(); this.restoreFocus(); }
  private closeDeleteModal() { this.showDeleteModal = false; this.selectedResource = null; this.removeModalListeners(); this.restoreFocus(); }
  private closePreviewModal() { this.showPreviewModal = false; this.selectedResource = null; this.removeModalListeners(); this.restoreFocus(); }

  private setFilter(type: string) {
    this.filterType = type; this.page = 1;
    this.searchMode && this.searchQuery.trim() ? this.doSearch() : this.loadResources();
  }

  private onSearchInput(e: Event) {
    this.searchQuery = (e.target as HTMLInputElement).value;
    if (this._searchTimer) clearTimeout(this._searchTimer);
    this._searchTimer = setTimeout(() => this.doSearch(), 200);
  }

  private onPageChange(newPage: number) { this.page = newPage; this.loadResources(); }
  private totalPages() { return Math.max(1, Math.ceil(this.total / this.pageSize)); }
  private get displayedResources(): WorkspaceResource[] {
    return this.searchMode ? this.searchResults : this.resources;
  }

  // ── Tag input ──

  private onTagInputKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      const input = e.target as HTMLInputElement;
      const tag = input.value.trim();
      if (tag && !this.formTagsList.includes(tag)) this.formTagsList = [...this.formTagsList, tag];
      input.value = '';
    }
    if (e.key === 'Backspace' && !(e.target as HTMLInputElement).value && this.formTagsList.length > 0)
      this.formTagsList = this.formTagsList.slice(0, -1);
  }

  private removeFormTag(tag: string) { this.formTagsList = this.formTagsList.filter(t => t !== tag); }

  private onFormKeydown(e: KeyboardEvent, onSubmit: () => void) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); onSubmit(); }
  }

  // ── File upload ──

  private onFileSelect(e: Event) {
    const input = e.target as HTMLInputElement;
    if (input.files?.length) {
      this.formFile = input.files[0];
      this.autoExtractTags(this.formFile.name);
    }
  }

  private onDragOver(e: DragEvent) { e.preventDefault(); this.uploadDragover = true; }
  private onDragLeave() { this.uploadDragover = false; }
  private onDrop(e: DragEvent) {
    e.preventDefault(); this.uploadDragover = false;
    if (e.dataTransfer?.files.length) {
      this.formFile = e.dataTransfer.files[0];
      this.autoExtractTags(this.formFile.name);
    }
  }

  private autoExtractTags(fileName: string) {
    if (this.formTagsList.length > 0) return;
    const extracted = extractTagsFromFilename(fileName);
    if (extracted.length > 0) this.formTagsList = extracted;
  }

  private async generateAITags() {
    if (!this.formFile) return;
    this.aiGeneratingTags = true;
    try {
      const res = await workspaceResourceApi.suggestTags({
        name: this.formFile.name,
        resourceType: this.formType,
        description: this.formDescription.trim() || undefined,
      });
      const tags = res.result ?? [];
      if (tags.length > 0) {
        const merged = [...new Set([...this.formTagsList, ...tags])];
        this.formTagsList = merged;
        success('AI 标签已生成');
      }
    } catch (e: any) {
      toastError('AI 生成标签失败: ' + (e.message || '未知错误'));
    } finally {
      this.aiGeneratingTags = false;
    }
  }

  private removeFile() { this.formFile = null; this.uploadProgress = 0; }

  private get isFormDirty(): boolean {
    return !!this.formName.trim() || !!this.formDescription.trim() ||
      this.formTagsList.length > 0 || !!this.formMetadata.trim() || !!this.formFile;
  }

  // ── Render ──

  render() {
    return html`
      <div class="resource-view">
        ${this.renderToolbar()}
        ${this.loading ? this.renderSkeleton() : this.error ? this.renderError() : this.renderGrid()}
        ${!this.searchMode && !this.loading && !this.error ? this.renderPagination() : nothing}
        ${this.showCreateModal ? this.renderCreateModal() : nothing}
        ${this.showEditModal ? this.renderEditModal() : nothing}
        ${this.showDeleteModal ? this.renderDeleteModal() : nothing}
        ${this.showPreviewModal ? this.renderPreviewModal() : nothing}
      </div>
    `;
  }

  private renderToolbar() {
    return html`
      <div class="filter-bar">
        <div class="resource-search">
          <svg class="resource-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16">
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          <input type="text" placeholder="搜索资源..." .value=${this.searchQuery} @input=${this.onSearchInput} />
          ${this.searchQuery ? html`
            <button class="resource-search-clear" @click=${() => { this.searchQuery = ''; if (this._searchTimer) clearTimeout(this._searchTimer); this.loadResources(); }}>清除</button>
          ` : nothing}
        </div>
        <div class="resource-filter-chips">
          ${TYPE_OPTIONS.map(opt => html`
            <button class="resource-filter-chip ${this.filterType === opt.value ? 'resource-filter-chip--active' : ''}"
              @click=${() => this.setFilter(opt.value)}>${opt.label}</button>
          `)}
        </div>
        <button class="btn primary" @click=${() => this.openModal('create')}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16">
            <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
          </svg>
          新建资源
        </button>
      </div>
    `;
  }

  private renderGrid() {
    const items = this.displayedResources;
    if (items.length === 0) {
      return html`
        <div class="empty-center">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" width="48" height="48" style="opacity:0.3;margin-bottom:12px">
            <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>
          </svg>
          <p class="empty-center__text">${this.searchMode ? '未找到匹配的资源' : '暂无资源'}</p>
          ${!this.searchMode ? html`
            <button class="btn primary" style="margin-top:16px" @click=${() => this.openModal('create')}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16">
                <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
              </svg>
              新建资源
            </button>
          ` : nothing}
        </div>
      `;
    }
    return html`
      <div class="resource-grid">${items.map(res => this.renderCard(res))}</div>
      ${this.searchMode ? html`<div class="resource-search-badge">搜索模式 · ${items.length} 个结果</div>` : nothing}
    `;
  }

  private renderCard(res: WorkspaceResource) {
    const cfg = RESOURCE_TYPE_CONFIG[res.resourceType] || { label: res.resourceType, color: '#6b7280' };
    const tags = parseTags(res.tags);
    return html`
      <div class="card resource-card" data-type=${res.resourceType} @click=${() => this.openModal('preview', res)}>
        <div class="resource-card-header">
          <span class="resource-type-badge" style="--type-color: ${cfg.color}">${cfg.label}</span>
          <div class="resource-card-actions">
            <button class="resource-action-btn" title="预览" @click=${(e: Event) => { e.stopPropagation(); this.openModal('preview', res); }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
            </button>
            <button class="resource-action-btn" title="编辑" @click=${(e: Event) => { e.stopPropagation(); this.openModal('edit', res); }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
            </button>
            <button class="resource-action-btn resource-action-btn--danger" title="删除" @click=${(e: Event) => { e.stopPropagation(); this.openModal('delete', res); }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
            </button>
          </div>
        </div>
        <div class="resource-card-body">
          <h3 class="resource-card-name" title=${res.name}>${res.name}</h3>
          ${res.description ? html`<p class="resource-card-desc">${res.description}</p>`
            : html`<p class="resource-card-desc resource-card-desc--empty">暂无描述</p>`}
        </div>
        <div class="resource-card-footer">
          <div class="resource-card-tags">
            ${tags.length > 0
              ? [...tags.slice(0, 3), ...(tags.length > 3 ? [`+${tags.length - 3}`] : [])].map(t => html`<span class="resource-tag">${t}</span>`)
              : html`<span class="resource-tag resource-tag--empty">无标签</span>`}
          </div>
          <span class="resource-card-date">${formatDate(res.updatedAt)}</span>
        </div>
      </div>
    `;
  }

  private renderSkeleton() {
    return html`
      <div class="resource-grid">
        ${Array.from({ length: 6 }).map(() => html`
          <div class="card resource-card resource-card--skeleton">
            <div class="resource-skeleton-badge"></div>
            <div class="resource-skeleton-title"></div>
            <div class="resource-skeleton-line"></div>
            <div class="resource-skeleton-line resource-skeleton-line--short"></div>
          </div>
        `)}
      </div>
    `;
  }

  private renderError() {
    return html`
      <div class="page-error">
        <p class="page-error__message">${this.error}</p>
        <button class="btn-secondary" @click=${this.loadResources}>重试</button>
      </div>
    `;
  }

  private renderPagination() {
    const totalPages = this.totalPages();
    if (totalPages <= 1) return nothing;
    const pages: (number | string)[] = [];
    for (let i = 1; i <= totalPages; i++) {
      if (i === 1 || i === totalPages || (i >= this.page - 1 && i <= this.page + 1)) pages.push(i);
      else if (pages[pages.length - 1] !== '...') pages.push('...');
    }
    const start = (this.page - 1) * this.pageSize + 1;
    const end = Math.min(this.page * this.pageSize, this.total);
    return html`
      <div class="resource-pagination">
        <button class="btn-secondary" ?disabled=${this.page <= 1} @click=${() => this.onPageChange(this.page - 1)}>上一页</button>
        ${pages.map(p => p === '...'
          ? html`<span class="resource-page-ellipsis">...</span>`
          : html`<button class="btn-secondary ${p === this.page ? 'resource-page-btn--active' : ''}" @click=${() => this.onPageChange(p as number)}>${p}</button>`)}
        <button class="btn-secondary" ?disabled=${this.page >= totalPages} @click=${() => this.onPageChange(this.page + 1)}>下一页</button>
        <span class="resource-page-info">${start}-${end} / 共 ${this.total} 条</span>
      </div>
    `;
  }

  // ── Modals ──

  private renderCreateModal() {
    return this.renderFormModal('新建资源', this.submitCreate, () => this.closeCreateModal(), true);
  }

  private renderEditModal() {
    return this.renderFormModal('编辑资源', this.submitEdit, () => this.closeEditModal(), false);
  }

  private renderFormModal(title: string, onSubmit: () => void, onClose: () => void, isCreate: boolean) {
    const isEdit = !isCreate;
    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label=${title}
        @click=${(e: Event) => { if (e.target === e.currentTarget && !this.isFormDirty) onClose(); }}>
        <div class="modal-box" @click=${(e: Event) => e.stopPropagation()} @keydown=${(e: KeyboardEvent) => this.onFormKeydown(e, onSubmit)}>
          <div class="modal-header">
            <h3>${title}</h3>
            <button class="modal-close" @click=${onClose} aria-label="关闭">&times;</button>
          </div>
          <div class="modal-body">
            ${isCreate ? this.renderUploadZone() : nothing}
            <label class="field ${this.formNameError ? 'field--error' : ''}">
              <span>资源名称 <span style="color:var(--danger)">*</span></span>
              <input type="text" .value=${this.formName}
                @input=${(e: Event) => { this.formName = (e.target as HTMLInputElement).value; if (this.formNameError) this.formNameError = ''; }}
                placeholder="例如：工厂三楼车间模型" />
              ${this.formNameError ? html`<span class="form-error">${this.formNameError}</span>` : nothing}
            </label>
            <label class="field">
              <span>资源类型</span>
              <select .value=${this.formType} @change=${(e: Event) => this.formType = (e.target as HTMLSelectElement).value} ?disabled=${isEdit}>
                ${TYPE_OPTIONS.filter(o => o.value).map(opt => html`<option value=${opt.value}>${opt.label}</option>`)}
              </select>
            </label>
            <label class="field">
              <span>描述</span>
              <textarea .value=${this.formDescription} @input=${(e: Event) => this.formDescription = (e.target as HTMLTextAreaElement).value} rows="3" placeholder="资源的详细描述..."></textarea>
            </label>
            <label class="field">
              <span>标签</span>
              <div class="resource-chip-input" @click=${(e: Event) => { (e.currentTarget as HTMLElement).querySelector('input')?.focus(); }}>
                ${this.formTagsList.map(tag => html`
                  <span class="resource-chip">${tag}<button class="resource-chip-remove" @click=${(e: Event) => { e.stopPropagation(); this.removeFormTag(tag); }} aria-label="移除标签 ${tag}">&times;</button></span>
                `)}
                <input type="text" class="resource-chip-input-field" placeholder="按回车添加标签..." @keydown=${this.onTagInputKeydown} />
              </div>
              ${isCreate && this.formFile ? html`
                <button type="button" class="resource-ai-tags-btn" ?disabled=${this.aiGeneratingTags} @click=${this.generateAITags}>
                  ${this.aiGeneratingTags
                    ? html`<span class="resource-spinner"></span>AI 生成中...`
                    : html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><path d="M12 2a4 4 0 0 1 4 4v1h2a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1h2V6a4 4 0 0 1 4-4z"/></svg>
                      ✨ AI 生成标签`}
                </button>
              ` : nothing}
            </label>
            <details class="resource-field-advanced">
              <summary>元数据（JSON，可选）</summary>
              <textarea class="resource-field-advanced__input" .value=${this.formMetadata} @input=${(e: Event) => this.formMetadata = (e.target as HTMLTextAreaElement).value} rows="4" placeholder='{"key": "value"}'></textarea>
            </details>
            ${this.uploadProgress > 0 && this.uploadProgress < 100 ? html`
              <div class="resource-upload-progress">
                <div class="resource-upload-progress-bar" style="width:${this.uploadProgress}%"></div>
                <span>上传中 ${this.uploadProgress}%</span>
              </div>
            ` : nothing}
          </div>
          <div class="modal-footer">
            <span class="form-hint" style="margin-right:auto">Esc 关闭 · Ctrl+Enter 提交</span>
            <button class="btn-secondary" @click=${onClose}>取消</button>
            <button class="btn primary" ?disabled=${this.formSubmitting || !this.formName.trim()} @click=${onSubmit}>
              ${this.formSubmitting ? html`<span class="resource-spinner"></span>保存中...` : '保存'}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  private renderUploadZone() {
    if (this.formFile) {
      return html`
        <div class="resource-upload-preview">
          <div class="resource-upload-file-info">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="20" height="20">
              <path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><polyline points="13 2 13 9 20 9"/>
            </svg>
            <span class="resource-upload-file-name">${this.formFile.name}</span>
            <span class="resource-upload-file-size">${formatFileSize(this.formFile.size)}</span>
          </div>
          <button class="resource-upload-remove" @click=${this.removeFile} aria-label="移除文件">&times;</button>
        </div>
      `;
    }
    return html`
      <div class="resource-upload-zone ${this.uploadDragover ? 'resource-upload-zone--active' : ''}"
        @dragover=${this.onDragOver} @dragleave=${this.onDragLeave} @drop=${this.onDrop}
        @click=${() => { const input = this.renderRoot.querySelector('.resource-upload-input') as HTMLInputElement; input?.click(); }}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="24" height="24">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>
        </svg>
        <span>拖拽文件到此处，或<span class="resource-upload-link">点击浏览</span></span>
        <input type="file" class="resource-upload-input" @change=${this.onFileSelect} hidden />
      </div>
    `;
  }

  private renderDeleteModal() {
    return html`
      <div class="modal-overlay" @click=${this.closeDeleteModal} role="dialog" aria-modal="true" aria-label="确认删除">
        <div class="modal-box" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <h3>确认删除</h3>
            <button class="modal-close" @click=${this.closeDeleteModal} aria-label="关闭">&times;</button>
          </div>
          <div class="modal-body">
            <p class="modal-desc">确定要删除资源 <strong>${this.selectedResource?.name}</strong> 吗？此操作不可撤销。</p>
          </div>
          <div class="modal-footer">
            <button class="btn-secondary" @click=${this.closeDeleteModal}>取消</button>
            <button class="btn primary" ?disabled=${this.formSubmitting} @click=${this.deleteResource} style="background:var(--danger);border-color:var(--danger)">
              ${this.formSubmitting ? '删除中...' : '删除'}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  private renderPreviewModal() {
    const res = this.selectedResource;
    if (!res) return nothing;
    const cfg = RESOURCE_TYPE_CONFIG[res.resourceType] || { label: res.resourceType, color: '#6b7280' };
    return html`
      <div class="modal-overlay" @click=${this.closePreviewModal} role="dialog" aria-modal="true" aria-label="资源预览">
        <div class="modal-box modal--wide" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <h3><span class="resource-type-badge" style="--type-color:${cfg.color}">${cfg.label}</span> ${res.name}</h3>
            <button class="modal-close" @click=${this.closePreviewModal} aria-label="关闭">&times;</button>
          </div>
          <div class="modal-body">
            <div class="resource-preview-grid">
              <div class="resource-preview-info">
                ${res.description ? html`<p class="resource-preview-desc">${res.description}</p>` : nothing}
                <div class="resource-preview-meta">
                  <div class="resource-meta-row"><span>ID</span><code>${res.id}</code></div>
                  <div class="resource-meta-row"><span>文件路径</span><code>${res.filePath}</code></div>
                  <div class="resource-meta-row"><span>创建</span><span>${formatDate(res.createdAt)}</span></div>
                  <div class="resource-meta-row"><span>更新</span><span>${formatDate(res.updatedAt)}</span></div>
                </div>
                <div class="resource-preview-tags">
                  ${parseTags(res.tags).map(t => html`<span class="resource-tag">${t}</span>`)}
                </div>
                ${res.metadata ? html`
                  <details class="resource-preview-json">
                    <summary>元数据</summary>
                    <pre>${res.metadata}</pre>
                  </details>
                ` : nothing}
              </div>
              <div class="resource-preview-thumb">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" width="48" height="48" style="opacity:0.3">
                  ${res.resourceType === 'scene'
                    ? html`<path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>`
                    : html`<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/>`}
                </svg>
                <p style="color:var(--muted);font-size:13px;margin-top:8px">${cfg.label} 预览</p>
              </div>
            </div>
          </div>
        </div>
      </div>
    `;
  }
}
