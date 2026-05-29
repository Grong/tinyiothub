import { LitElement, html, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import {
  workspaceResourceApi,
  type WorkspaceResource,
  type ResourceSearchResult,
} from '../../api/workspace-resources.js';
import { success, error as toastError } from '../components/toast.js';
import '../../styles/views/workspace-resources.css';

// ── Resource type config ──
const RESOURCE_TYPE_CONFIG: Record<string, { label: string; color: string; icon: string }> = {
  scene: { label: '3D 场景', color: '#00d4aa', icon: 'cube' },
  device_model: { label: '设备模型', color: '#3b82f6', icon: 'cpu' },
  image: { label: '图片', color: '#f59e0b', icon: 'image' },
  document: { label: '文档', color: '#8b5cf6', icon: 'file-text' },
};

const TYPE_OPTIONS = [
  { value: '', label: '全部类型' },
  { value: 'scene', label: '3D 场景' },
  { value: 'device_model', label: '设备模型' },
  { value: 'image', label: '图片' },
  { value: 'document', label: '文档' },
];

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString('zh-CN', {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return iso;
  }
}

function parseTags(tags: string[] | string): string[] {
  if (Array.isArray(tags)) return tags;
  try {
    const parsed = JSON.parse(tags);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

@customElement('view-workspace-resources')
export class ViewWorkspaceResources extends LitElement {
  createRenderRoot() {
    return this;
  }

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

  // Modal states
  @state() showCreateModal = false;
  @state() showEditModal = false;
  @state() showDeleteModal = false;
  @state() showPreviewModal = false;
  @state() selectedResource: WorkspaceResource | null = null;

  // Form states
  @state() formName = '';
  @state() formType = 'scene';
  @state() formDescription = '';
  @state() formTags = '';
  @state() formMetadata = '';
  @state() formSubmitting = false;

  // Focus management
  private lastFocusedElement: Element | null = null;
  private boundKeydown = this.onKeydown.bind(this);

  connectedCallback() {
    super.connectedCallback();
    this.loadResources();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.removeModalListeners();
  }

  // ── Modal keyboard / focus management ──

  private addModalListeners() {
    document.addEventListener('keydown', this.boundKeydown);
  }

  private removeModalListeners() {
    document.removeEventListener('keydown', this.boundKeydown);
  }

  private onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (this.showCreateModal) this.closeCreateModal();
      else if (this.showEditModal) this.closeEditModal();
      else if (this.showDeleteModal) this.closeDeleteModal();
      else if (this.showPreviewModal) this.closePreviewModal();
    }
    if (e.key === 'Tab') {
      this.trapFocus(e);
    }
  }

  private trapFocus(e: KeyboardEvent) {
    const modal = this.renderRoot.querySelector('.wsr-modal') as HTMLElement | null;
    if (!modal) return;
    const focusable = modal.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
    );
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }

  private saveFocus() {
    this.lastFocusedElement = document.activeElement;
  }

  private restoreFocus() {
    if (this.lastFocusedElement && 'focus' in this.lastFocusedElement) {
      (this.lastFocusedElement as HTMLElement).focus();
    }
  }

  private openModal(type: 'create' | 'edit' | 'delete' | 'preview', resource?: WorkspaceResource) {
    this.saveFocus();
    this.addModalListeners();
    if (type === 'create') this.showCreateModal = true;
    if (type === 'edit') {
      this.selectedResource = resource ?? null;
      if (resource) {
        this.formName = resource.name;
        this.formType = resource.resourceType;
        this.formDescription = resource.description ?? '';
        this.formTags = parseTags(resource.tags).join(', ');
        this.formMetadata = resource.metadata ?? '';
      }
      this.showEditModal = true;
    }
    if (type === 'delete') {
      this.selectedResource = resource ?? null;
      this.showDeleteModal = true;
    }
    if (type === 'preview') {
      this.selectedResource = resource ?? null;
      this.showPreviewModal = true;
    }
    // Focus first focusable element after modal renders
    requestAnimationFrame(() => {
      const modal = this.renderRoot.querySelector('.wsr-modal') as HTMLElement | null;
      if (modal) {
        const focusable = modal.querySelector<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        );
        focusable?.focus();
      }
    });
  }

  // ── Data ──

  private async loadResources() {
    this.loading = true;
    this.error = '';
    try {
      const res = await workspaceResourceApi.listResources({
        resourceType: this.filterType || undefined,
        page: this.page,
        pageSize: this.pageSize,
      });
      this.resources = res.result?.data ?? [];
      this.total = res.result?.pagination?.totalCount ?? 0;
      this.searchMode = false;
    } catch (e: any) {
      this.error = e.message || '加载资源失败';
    } finally {
      this.loading = false;
    }
  }

  private async doSearch() {
    if (!this.searchQuery.trim()) {
      this.loadResources();
      return;
    }
    this.loading = true;
    this.error = '';
    try {
      const results = await workspaceResourceApi.searchResources({
        q: this.searchQuery.trim(),
        type: this.filterType || undefined,
        limit: 20,
      });
      this.searchResults = results.result ?? [];
      this.searchMode = true;
    } catch (e: any) {
      this.error = e.message || '搜索失败';
    } finally {
      this.loading = false;
    }
  }

  private async deleteResource() {
    if (!this.selectedResource) return;
    this.formSubmitting = true;
    try {
      await workspaceResourceApi.deleteResource(this.selectedResource.id);
      success('资源已删除');
      this.closeDeleteModal();
      this.loadResources();
    } catch (e: any) {
      toastError(e.message || '删除失败');
    } finally {
      this.formSubmitting = false;
    }
  }

  private async submitCreate() {
    if (!this.formName.trim()) {
      toastError('请输入资源名称');
      return;
    }
    this.formSubmitting = true;
    try {
      await workspaceResourceApi.createResource({
        resourceType: this.formType,
        name: this.formName.trim(),
        description: this.formDescription.trim() || undefined,
        tags: this.formTags
          .split(',')
          .map((t) => t.trim())
          .filter(Boolean),
        metadata: this.formMetadata.trim() || undefined,
      });
      success('资源创建成功');
      this.closeCreateModal();
      this.loadResources();
    } catch (e: any) {
      toastError(e.message || '创建失败');
    } finally {
      this.formSubmitting = false;
    }
  }

  private async submitEdit() {
    if (!this.selectedResource || !this.formName.trim()) {
      toastError('请输入资源名称');
      return;
    }
    this.formSubmitting = true;
    try {
      await workspaceResourceApi.updateResource(this.selectedResource.id, {
        name: this.formName.trim(),
        description: this.formDescription.trim() || undefined,
        tags: this.formTags
          .split(',')
          .map((t) => t.trim())
          .filter(Boolean),
        metadata: this.formMetadata.trim() || undefined,
      });
      success('资源更新成功');
      this.closeEditModal();
      this.loadResources();
    } catch (e: any) {
      toastError(e.message || '更新失败');
    } finally {
      this.formSubmitting = false;
    }
  }

  private closeCreateModal() {
    this.showCreateModal = false;
    this.formSubmitting = false;
    this.removeModalListeners();
    this.restoreFocus();
  }

  private closeEditModal() {
    this.showEditModal = false;
    this.selectedResource = null;
    this.formSubmitting = false;
    this.removeModalListeners();
    this.restoreFocus();
  }

  private closeDeleteModal() {
    this.showDeleteModal = false;
    this.selectedResource = null;
    this.removeModalListeners();
    this.restoreFocus();
  }

  private closePreviewModal() {
    this.showPreviewModal = false;
    this.selectedResource = null;
    this.removeModalListeners();
    this.restoreFocus();
  }

  private onTypeFilterChange(e: Event) {
    const target = e.target as HTMLSelectElement;
    this.filterType = target.value;
    this.page = 1;
    if (this.searchMode && this.searchQuery.trim()) {
      this.doSearch();
    } else {
      this.loadResources();
    }
  }

  private onSearchInput(e: Event) {
    this.searchQuery = (e.target as HTMLInputElement).value;
  }

  private onSearchKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      this.doSearch();
    }
  }

  private onPageChange(newPage: number) {
    this.page = newPage;
    this.loadResources();
  }

  private totalPages() {
    return Math.max(1, Math.ceil(this.total / this.pageSize));
  }

  private get displayedResources(): WorkspaceResource[] {
    if (this.searchMode) return this.searchResults;
    return this.resources;
  }

  // ── Render ──

  render() {
    return html`
      <div class="wsr-container">
        ${this.renderHeader()} ${this.renderToolbar()}
        ${this.loading
          ? this.renderSkeleton()
          : this.error
            ? this.renderError()
            : this.renderGrid()}
        ${!this.searchMode && !this.loading && !this.error ? this.renderPagination() : nothing}
        ${this.showCreateModal ? this.renderCreateModal() : nothing}
        ${this.showEditModal ? this.renderEditModal() : nothing}
        ${this.showDeleteModal ? this.renderDeleteModal() : nothing}
        ${this.showPreviewModal ? this.renderPreviewModal() : nothing}
      </div>
    `;
  }

  private renderHeader() {
    return html`
      <div class="wsr-header">
        <h1 class="wsr-title">
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            width="28"
            height="28"
          >
            <path
              d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"
            />
            <polyline points="3.27 6.96 12 12.01 20.73 6.96" />
            <line x1="12" y1="22.08" x2="12" y2="12" />
          </svg>
          <span class="wsr-title__text">资源库</span>
        </h1>
        <p class="wsr-subtitle">管理工作空间的多媒体资源 — 3D 场景、设备模型、图片与文档</p>
      </div>
    `;
  }

  private renderToolbar() {
    return html`
      <div class="wsr-toolbar">
        <div class="wsr-search">
          <svg
            class="wsr-search-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            width="18"
            height="18"
          >
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            class="wsr-search-input"
            type="text"
            placeholder="搜索资源名称、描述或标签..."
            .value=${this.searchQuery}
            @input=${this.onSearchInput}
            @keydown=${this.onSearchKeydown}
          />
          ${this.searchQuery
            ? html`<button
                class="wsr-search-clear"
                @click=${() => {
                  this.searchQuery = '';
                  this.loadResources();
                }}
              >
                清除
              </button>`
            : nothing}
        </div>
        <select class="wsr-filter" @change=${this.onTypeFilterChange}>
          ${TYPE_OPTIONS.map(
            (opt) =>
              html`<option value=${opt.value} ?selected=${this.filterType === opt.value}>
                ${opt.label}
              </option>`,
          )}
        </select>
        <button class="wsr-btn-primary" @click=${() => this.openModal('create')}>
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            width="16"
            height="16"
          >
            <line x1="12" y1="5" x2="12" y2="19" />
            <line x1="5" y1="12" x2="19" y2="12" />
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
        <div class="wsr-empty">
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1"
            width="64"
            height="64"
          >
            <path
              d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"
            />
          </svg>
          <p>${this.searchMode ? '未找到匹配的资源' : '暂无资源，点击「新建资源」创建第一个'}</p>
        </div>
      `;
    }

    return html`
      <div class="wsr-grid">${items.map((res) => this.renderCard(res))}</div>
      ${this.searchMode
        ? html`<div class="wsr-search-badge">搜索模式 · ${items.length} 个结果</div>`
        : nothing}
    `;
  }

  private renderCard(res: WorkspaceResource) {
    const typeConfig = RESOURCE_TYPE_CONFIG[res.resourceType] || {
      label: res.resourceType,
      color: '#6b7280',
      icon: 'box',
    };
    const tags = parseTags(res.tags);

    return html`
      <div class="wsr-card" data-type=${res.resourceType}>
        <div class="wsr-card-header">
          <span class="wsr-type-badge" style="--type-color: ${typeConfig.color}">
            ${typeConfig.label}
          </span>
          <div class="wsr-card-actions">
            <button
              class="wsr-action-btn"
              title="预览"
              @click=${() => this.openModal('preview', res)}
            >
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                width="14"
                height="14"
              >
                <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                <circle cx="12" cy="12" r="3" />
              </svg>
            </button>
            <button class="wsr-action-btn" title="编辑" @click=${() => this.openModal('edit', res)}>
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                width="14"
                height="14"
              >
                <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
                <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
              </svg>
            </button>
            <button
              class="wsr-action-btn wsr-action-danger"
              title="删除"
              @click=${() => this.openModal('delete', res)}
            >
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                width="14"
                height="14"
              >
                <polyline points="3 6 5 6 21 6" />
                <path
                  d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"
                />
              </svg>
            </button>
          </div>
        </div>

        <div class="wsr-card-body">
          <h3 class="wsr-card-name" title=${res.name}>${res.name}</h3>
          ${res.description
            ? html`<p class="wsr-card-desc">${res.description}</p>`
            : html`<p class="wsr-card-desc wsr-card-desc--empty">暂无描述</p>`}
        </div>

        <div class="wsr-card-footer">
          <div class="wsr-card-tags">
            ${tags.length > 0
              ? tags.slice(0, 3).map((tag) => html`<span class="wsr-tag">${tag}</span>`)
              : html`<span class="wsr-tag wsr-tag--empty">无标签</span>`}
            ${tags.length > 3 ? html`<span class="wsr-tag">+${tags.length - 3}</span>` : nothing}
          </div>
          <span class="wsr-card-date">${formatDate(res.updatedAt)}</span>
        </div>
      </div>
    `;
  }

  private renderSkeleton() {
    return html`
      <div class="wsr-grid">
        ${Array.from({ length: 6 }).map(
          () => html`
            <div class="wsr-card wsr-card--skeleton">
              <div class="wsr-skeleton-badge"></div>
              <div class="wsr-skeleton-title"></div>
              <div class="wsr-skeleton-line"></div>
              <div class="wsr-skeleton-line wsr-skeleton-line--short"></div>
            </div>
          `,
        )}
      </div>
    `;
  }

  private renderError() {
    return html`
      <div class="wsr-error">
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          width="48"
          height="48"
        >
          <circle cx="12" cy="12" r="10" />
          <line x1="12" y1="8" x2="12" y2="12" />
          <line x1="12" y1="16" x2="12.01" y2="16" />
        </svg>
        <p>${this.error}</p>
        <button class="wsr-btn-secondary" @click=${this.loadResources}>重试</button>
      </div>
    `;
  }

  private renderPagination() {
    const totalPages = this.totalPages();
    if (totalPages <= 1) return nothing;

    const pages: (number | string)[] = [];
    for (let i = 1; i <= totalPages; i++) {
      if (i === 1 || i === totalPages || (i >= this.page - 1 && i <= this.page + 1)) {
        pages.push(i);
      } else if (pages[pages.length - 1] !== '...') {
        pages.push('...');
      }
    }

    return html`
      <div class="wsr-pagination">
        <button
          class="wsr-page-btn"
          ?disabled=${this.page <= 1}
          @click=${() => this.onPageChange(this.page - 1)}
        >
          上一页
        </button>
        ${pages.map((p) =>
          p === '...'
            ? html`<span class="wsr-page-ellipsis">...</span>`
            : html`
                <button
                  class="wsr-page-btn ${p === this.page ? 'wsr-page-btn--active' : ''}"
                  @click=${() => this.onPageChange(p as number)}
                >
                  ${p}
                </button>
              `,
        )}
        <button
          class="wsr-page-btn"
          ?disabled=${this.page >= totalPages}
          @click=${() => this.onPageChange(this.page + 1)}
        >
          下一页
        </button>
        <span class="wsr-page-info">共 ${this.total} 条</span>
      </div>
    `;
  }

  // ── Modals ──

  private renderCreateModal() {
    return this.renderFormModal('新建资源', this.submitCreate, () => this.closeCreateModal());
  }

  private renderEditModal() {
    return this.renderFormModal('编辑资源', this.submitEdit, () => this.closeEditModal());
  }

  private renderFormModal(title: string, onSubmit: () => void, onClose: () => void) {
    const isEdit = this.showEditModal;
    return html`
      <div
        class="wsr-modal-overlay"
        @click=${onClose}
        role="dialog"
        aria-modal="true"
        aria-label=${title}
      >
        <div class="wsr-modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="wsr-modal-header">
            <h2>${title}</h2>
            <button class="wsr-modal-close" @click=${onClose} aria-label="关闭">&times;</button>
          </div>
          <div class="wsr-modal-body">
            <label class="wsr-field">
              <span>资源名称 *</span>
              <input
                type="text"
                .value=${this.formName}
                @input=${(e: Event) => (this.formName = (e.target as HTMLInputElement).value)}
                placeholder="例如：工厂三楼车间模型"
              />
            </label>
            <label class="wsr-field">
              <span>资源类型</span>
              <select
                .value=${this.formType}
                @change=${(e: Event) => (this.formType = (e.target as HTMLSelectElement).value)}
                ?disabled=${isEdit}
              >
                ${TYPE_OPTIONS.filter((o) => o.value).map(
                  (opt) => html`<option value=${opt.value}>${opt.label}</option>`,
                )}
              </select>
            </label>
            <label class="wsr-field">
              <span>描述</span>
              <textarea
                .value=${this.formDescription}
                @input=${(e: Event) =>
                  (this.formDescription = (e.target as HTMLTextAreaElement).value)}
                rows="3"
                placeholder="资源的详细描述..."
              ></textarea>
            </label>
            <label class="wsr-field">
              <span>标签（逗号分隔）</span>
              <input
                type="text"
                .value=${this.formTags}
                @input=${(e: Event) => (this.formTags = (e.target as HTMLInputElement).value)}
                placeholder="factory, floor-3, temperature"
              />
            </label>
            <label class="wsr-field">
              <span>元数据（JSON）</span>
              <textarea
                .value=${this.formMetadata}
                @input=${(e: Event) =>
                  (this.formMetadata = (e.target as HTMLTextAreaElement).value)}
                rows="3"
                placeholder='{"floors": [...], "deviceInstances": [...]}'
              ></textarea>
            </label>
          </div>
          <div class="wsr-modal-footer">
            <button class="wsr-btn-secondary" @click=${onClose}>取消</button>
            <button
              class="wsr-btn-primary"
              ?disabled=${this.formSubmitting || !this.formName.trim()}
              @click=${onSubmit}
            >
              ${this.formSubmitting ? '保存中...' : '保存'}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  private renderDeleteModal() {
    return html`
      <div
        class="wsr-modal-overlay"
        @click=${this.closeDeleteModal}
        role="dialog"
        aria-modal="true"
        aria-label="确认删除"
      >
        <div class="wsr-modal wsr-modal--narrow" @click=${(e: Event) => e.stopPropagation()}>
          <div class="wsr-modal-header">
            <h2>确认删除</h2>
            <button class="wsr-modal-close" @click=${this.closeDeleteModal} aria-label="关闭">
              &times;
            </button>
          </div>
          <div class="wsr-modal-body">
            <p>
              确定要删除资源 <strong>${this.selectedResource?.name}</strong>
              吗？此操作不可撤销。
            </p>
          </div>
          <div class="wsr-modal-footer">
            <button class="wsr-btn-secondary" @click=${this.closeDeleteModal}>取消</button>
            <button
              class="wsr-btn-danger"
              ?disabled=${this.formSubmitting}
              @click=${this.deleteResource}
            >
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
    const typeConfig = RESOURCE_TYPE_CONFIG[res.resourceType] || {
      label: res.resourceType,
      color: '#6b7280',
    };

    return html`
      <div
        class="wsr-modal-overlay"
        @click=${this.closePreviewModal}
        role="dialog"
        aria-modal="true"
        aria-label="资源预览"
      >
        <div class="wsr-modal wsr-modal--wide" @click=${(e: Event) => e.stopPropagation()}>
          <div class="wsr-modal-header">
            <h2>
              <span class="wsr-type-badge" style="--type-color: ${typeConfig.color}"
                >${typeConfig.label}</span
              >
              ${res.name}
            </h2>
            <button class="wsr-modal-close" @click=${this.closePreviewModal} aria-label="关闭">
              &times;
            </button>
          </div>
          <div class="wsr-modal-body">
            <div class="wsr-preview-grid">
              <div class="wsr-preview-info">
                ${res.description
                  ? html`<p class="wsr-preview-desc">${res.description}</p>`
                  : nothing}
                <div class="wsr-preview-meta">
                  <div class="wsr-meta-row"><span>ID</span><code>${res.id}</code></div>
                  <div class="wsr-meta-row"><span>文件路径</span><code>${res.filePath}</code></div>
                  <div class="wsr-meta-row">
                    <span>创建时间</span><span>${formatDate(res.createdAt)}</span>
                  </div>
                  <div class="wsr-meta-row">
                    <span>更新时间</span><span>${formatDate(res.updatedAt)}</span>
                  </div>
                </div>
                <div class="wsr-preview-tags">
                  ${parseTags(res.tags).map((tag) => html`<span class="wsr-tag">${tag}</span>`)}
                </div>
                ${res.metadata
                  ? html`
                      <details class="wsr-preview-json">
                        <summary>元数据</summary>
                        <pre>${res.metadata}</pre>
                      </details>
                    `
                  : nothing}
              </div>
              ${res.resourceType === 'scene'
                ? html`
                    <div class="wsr-preview-scene">
                      <div class="wsr-preview-scene-placeholder">
                        <svg
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1"
                          width="48"
                          height="48"
                        >
                          <path
                            d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"
                          />
                        </svg>
                        <p>3D 场景预览</p>
                        <span class="wsr-preview-hint"
                          >在 AI 聊天中发送「显示场景 ${res.name}」即可渲染</span
                        >
                      </div>
                    </div>
                  `
                : html`
                    <div class="wsr-preview-media">
                      <div class="wsr-preview-media-placeholder">
                        <svg
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1"
                          width="48"
                          height="48"
                        >
                          <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
                          <circle cx="8.5" cy="8.5" r="1.5" />
                          <polyline points="21 15 16 10 5 21" />
                        </svg>
                        <p>${typeConfig.label}预览</p>
                      </div>
                    </div>
                  `}
            </div>
          </div>
        </div>
      </div>
    `;
  }
}
