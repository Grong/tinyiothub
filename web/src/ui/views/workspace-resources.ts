import { LitElement, html, css, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import {
  workspaceResourceApi,
  type WorkspaceResource,
  type ResourceSearchResult,
} from '../../api/workspace-resources.js';
import { success, error as toastError } from '../components/toast.js';

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

  connectedCallback() {
    super.connectedCallback();
    this.loadResources();
  }

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
      this.showDeleteModal = false;
      this.selectedResource = null;
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

  private openCreateModal() {
    this.formName = '';
    this.formType = 'scene';
    this.formDescription = '';
    this.formTags = '';
    this.formMetadata = '';
    this.showCreateModal = true;
  }

  private closeCreateModal() {
    this.showCreateModal = false;
    this.formSubmitting = false;
  }

  private openEditModal(res: WorkspaceResource) {
    this.selectedResource = res;
    this.formName = res.name;
    this.formType = res.resourceType;
    this.formDescription = res.description ?? '';
    this.formTags = parseTags(res.tags).join(', ');
    this.formMetadata = res.metadata ?? '';
    this.showEditModal = true;
  }

  private closeEditModal() {
    this.showEditModal = false;
    this.selectedResource = null;
    this.formSubmitting = false;
  }

  private openDeleteModal(res: WorkspaceResource) {
    this.selectedResource = res;
    this.showDeleteModal = true;
  }

  private closeDeleteModal() {
    this.showDeleteModal = false;
    this.selectedResource = null;
  }

  private openPreviewModal(res: WorkspaceResource) {
    this.selectedResource = res;
    this.showPreviewModal = true;
  }

  private closePreviewModal() {
    this.showPreviewModal = false;
    this.selectedResource = null;
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
          资源库
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
        <button class="wsr-btn-primary" @click=${this.openCreateModal}>
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
            <button class="wsr-action-btn" title="预览" @click=${() => this.openPreviewModal(res)}>
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
            <button class="wsr-action-btn" title="编辑" @click=${() => this.openEditModal(res)}>
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
              @click=${() => this.openDeleteModal(res)}
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
            : html`<button
                class="wsr-page-btn ${p === this.page ? 'wsr-page-btn--active' : ''}"
                @click=${() => this.onPageChange(p as number)}
              >
                ${p}
              </button>`,
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
    return this.renderFormModal('新建资源', this.submitCreate, this.closeCreateModal);
  }

  private renderEditModal() {
    return this.renderFormModal('编辑资源', this.submitEdit, this.closeEditModal);
  }

  private renderFormModal(title: string, onSubmit: () => void, onClose: () => void) {
    const isEdit = this.showEditModal;
    return html`
      <div class="wsr-modal-overlay" @click=${onClose}>
        <div class="wsr-modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="wsr-modal-header">
            <h2>${title}</h2>
            <button class="wsr-modal-close" @click=${onClose}>&times;</button>
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
      <div class="wsr-modal-overlay" @click=${this.closeDeleteModal}>
        <div class="wsr-modal wsr-modal--narrow" @click=${(e: Event) => e.stopPropagation()}>
          <div class="wsr-modal-header">
            <h2>确认删除</h2>
            <button class="wsr-modal-close" @click=${this.closeDeleteModal}>&times;</button>
          </div>
          <div class="wsr-modal-body">
            <p>
              确定要删除资源 <strong>${this.selectedResource?.name}</strong> 吗？此操作不可撤销。
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
      <div class="wsr-modal-overlay" @click=${this.closePreviewModal}>
        <div class="wsr-modal wsr-modal--wide" @click=${(e: Event) => e.stopPropagation()}>
          <div class="wsr-modal-header">
            <h2>
              <span class="wsr-type-badge" style="--type-color: ${typeConfig.color}"
                >${typeConfig.label}</span
              >
              ${res.name}
            </h2>
            <button class="wsr-modal-close" @click=${this.closePreviewModal}>&times;</button>
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

  // ── Styles ──
  static styles = css`
    :host {
      display: block;
      padding: 24px;
      color: var(--text, #e5e7eb);
      --wsr-bg: #0b0f17;
      --wsr-card-bg: #111827;
      --wsr-card-border: rgba(255, 255, 255, 0.06);
      --wsr-card-hover: #1a2236;
      --wsr-muted: #6b7280;
      --wsr-accent: #00d4aa;
      --wsr-danger: #ef4444;
      --wsr-radius: 10px;
      --wsr-shadow: 0 4px 24px rgba(0, 0, 0, 0.4);
    }

    .wsr-container {
      max-width: 1400px;
      margin: 0 auto;
    }

    /* Header */
    .wsr-header {
      margin-bottom: 24px;
    }
    .wsr-title {
      display: flex;
      align-items: center;
      gap: 12px;
      font-size: 24px;
      font-weight: 600;
      margin: 0 0 6px;
      color: var(--text, #e5e7eb);
    }
    .wsr-title svg {
      color: var(--wsr-accent);
    }
    .wsr-subtitle {
      margin: 0;
      color: var(--wsr-muted);
      font-size: 14px;
    }

    /* Toolbar */
    .wsr-toolbar {
      display: flex;
      gap: 12px;
      align-items: center;
      margin-bottom: 20px;
      flex-wrap: wrap;
    }
    .wsr-search {
      position: relative;
      flex: 1;
      min-width: 240px;
      max-width: 480px;
    }
    .wsr-search-icon {
      position: absolute;
      left: 12px;
      top: 50%;
      transform: translateY(-50%);
      color: var(--wsr-muted);
      pointer-events: none;
    }
    .wsr-search-input {
      width: 100%;
      padding: 10px 12px 10px 38px;
      background: var(--wsr-card-bg);
      border: 1px solid var(--wsr-card-border);
      border-radius: var(--wsr-radius);
      color: var(--text, #e5e7eb);
      font-size: 14px;
      outline: none;
      transition:
        border-color 0.2s,
        box-shadow 0.2s;
    }
    .wsr-search-input:focus {
      border-color: var(--wsr-accent);
      box-shadow: 0 0 0 3px rgba(0, 212, 170, 0.1);
    }
    .wsr-search-input::placeholder {
      color: var(--wsr-muted);
    }
    .wsr-search-clear {
      position: absolute;
      right: 8px;
      top: 50%;
      transform: translateY(-50%);
      background: transparent;
      border: none;
      color: var(--wsr-muted);
      font-size: 12px;
      cursor: pointer;
      padding: 4px 8px;
    }
    .wsr-search-clear:hover {
      color: var(--text, #e5e7eb);
    }

    .wsr-filter {
      padding: 10px 14px;
      background: var(--wsr-card-bg);
      border: 1px solid var(--wsr-card-border);
      border-radius: var(--wsr-radius);
      color: var(--text, #e5e7eb);
      font-size: 14px;
      cursor: pointer;
      outline: none;
    }
    .wsr-filter:focus {
      border-color: var(--wsr-accent);
    }
    .wsr-filter option {
      background: var(--wsr-card-bg);
    }

    .wsr-btn-primary,
    .wsr-btn-secondary,
    .wsr-btn-danger {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 10px 18px;
      border: none;
      border-radius: var(--wsr-radius);
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      transition:
        opacity 0.15s,
        transform 0.1s;
    }
    .wsr-btn-primary:active,
    .wsr-btn-secondary:active,
    .wsr-btn-danger:active {
      transform: scale(0.98);
    }
    .wsr-btn-primary:disabled,
    .wsr-btn-secondary:disabled,
    .wsr-btn-danger:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }

    .wsr-btn-primary {
      background: var(--wsr-accent);
      color: #000;
    }
    .wsr-btn-primary:hover {
      opacity: 0.9;
    }

    .wsr-btn-secondary {
      background: var(--wsr-card-bg);
      color: var(--text, #e5e7eb);
      border: 1px solid var(--wsr-card-border);
    }
    .wsr-btn-secondary:hover {
      background: var(--wsr-card-hover);
    }

    .wsr-btn-danger {
      background: var(--wsr-danger);
      color: #fff;
    }
    .wsr-btn-danger:hover {
      opacity: 0.9;
    }

    /* Grid */
    .wsr-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
      gap: 16px;
    }

    /* Card */
    .wsr-card {
      background: var(--wsr-card-bg);
      border: 1px solid var(--wsr-card-border);
      border-radius: var(--wsr-radius);
      padding: 16px;
      display: flex;
      flex-direction: column;
      gap: 12px;
      transition:
        transform 0.15s,
        border-color 0.2s,
        box-shadow 0.2s;
      cursor: default;
    }
    .wsr-card:hover {
      border-color: rgba(0, 212, 170, 0.2);
      box-shadow: var(--wsr-shadow);
      transform: translateY(-2px);
    }
    .wsr-card[data-type='scene'] {
      border-left: 3px solid #00d4aa;
    }
    .wsr-card[data-type='device_model'] {
      border-left: 3px solid #3b82f6;
    }
    .wsr-card[data-type='image'] {
      border-left: 3px solid #f59e0b;
    }
    .wsr-card[data-type='document'] {
      border-left: 3px solid #8b5cf6;
    }

    .wsr-card-header {
      display: flex;
      justify-content: space-between;
      align-items: flex-start;
    }
    .wsr-type-badge {
      font-size: 11px;
      font-weight: 600;
      padding: 3px 10px;
      border-radius: 999px;
      background: color-mix(in srgb, var(--type-color, #6b7280) 12%, transparent);
      color: var(--type-color, #6b7280);
      letter-spacing: 0.02em;
    }
    .wsr-card-actions {
      display: flex;
      gap: 4px;
      opacity: 0;
      transition: opacity 0.15s;
    }
    .wsr-card:hover .wsr-card-actions {
      opacity: 1;
    }
    .wsr-action-btn {
      display: flex;
      align-items: center;
      justify-content: center;
      width: 28px;
      height: 28px;
      border: none;
      border-radius: 6px;
      background: transparent;
      color: var(--wsr-muted);
      cursor: pointer;
      transition:
        background 0.15s,
        color 0.15s;
    }
    .wsr-action-btn:hover {
      background: rgba(255, 255, 255, 0.06);
      color: var(--text, #e5e7eb);
    }
    .wsr-action-danger:hover {
      color: var(--wsr-danger);
      background: rgba(239, 68, 68, 0.1);
    }

    .wsr-card-body {
      flex: 1;
      min-height: 0;
    }
    .wsr-card-name {
      font-size: 15px;
      font-weight: 600;
      margin: 0 0 6px;
      color: var(--text, #e5e7eb);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .wsr-card-desc {
      font-size: 13px;
      color: var(--wsr-muted);
      margin: 0;
      line-height: 1.5;
      display: -webkit-box;
      -webkit-line-clamp: 2;
      -webkit-box-orient: vertical;
      overflow: hidden;
    }
    .wsr-card-desc--empty {
      opacity: 0.5;
      font-style: italic;
    }

    .wsr-card-footer {
      display: flex;
      justify-content: space-between;
      align-items: center;
      gap: 8px;
      margin-top: auto;
      padding-top: 8px;
      border-top: 1px solid var(--wsr-card-border);
    }
    .wsr-card-tags {
      display: flex;
      gap: 4px;
      flex-wrap: wrap;
    }
    .wsr-tag {
      font-size: 11px;
      padding: 2px 8px;
      border-radius: 4px;
      background: rgba(255, 255, 255, 0.05);
      color: var(--wsr-muted);
    }
    .wsr-tag--empty {
      opacity: 0.4;
    }
    .wsr-card-date {
      font-size: 11px;
      color: var(--wsr-muted);
      white-space: nowrap;
    }

    /* Skeleton */
    .wsr-card--skeleton {
      gap: 10px;
      pointer-events: none;
    }
    .wsr-skeleton-badge {
      width: 60px;
      height: 20px;
      border-radius: 999px;
      background: linear-gradient(
        90deg,
        rgba(255, 255, 255, 0.04) 25%,
        rgba(255, 255, 255, 0.08) 50%,
        rgba(255, 255, 255, 0.04) 75%
      );
      background-size: 200% 100%;
      animation: wsr-skeleton 1.5s ease-in-out infinite;
    }
    .wsr-skeleton-title {
      width: 70%;
      height: 16px;
      border-radius: 4px;
      background: linear-gradient(
        90deg,
        rgba(255, 255, 255, 0.04) 25%,
        rgba(255, 255, 255, 0.08) 50%,
        rgba(255, 255, 255, 0.04) 75%
      );
      background-size: 200% 100%;
      animation: wsr-skeleton 1.5s ease-in-out infinite;
    }
    .wsr-skeleton-line {
      width: 100%;
      height: 12px;
      border-radius: 4px;
      background: linear-gradient(
        90deg,
        rgba(255, 255, 255, 0.04) 25%,
        rgba(255, 255, 255, 0.08) 50%,
        rgba(255, 255, 255, 0.04) 75%
      );
      background-size: 200% 100%;
      animation: wsr-skeleton 1.5s ease-in-out infinite;
    }
    .wsr-skeleton-line--short {
      width: 50%;
    }
    @keyframes wsr-skeleton {
      0% {
        background-position: 200% 0;
      }
      100% {
        background-position: -200% 0;
      }
    }

    /* Empty / Error */
    .wsr-empty,
    .wsr-error {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 16px;
      padding: 80px 24px;
      color: var(--wsr-muted);
      text-align: center;
    }
    .wsr-empty svg,
    .wsr-error svg {
      opacity: 0.3;
    }
    .wsr-empty p,
    .wsr-error p {
      margin: 0;
      font-size: 15px;
    }
    .wsr-error {
      color: var(--wsr-danger);
    }
    .wsr-error svg {
      opacity: 0.5;
      color: var(--wsr-danger);
    }

    /* Search badge */
    .wsr-search-badge {
      text-align: center;
      margin-top: 12px;
      font-size: 13px;
      color: var(--wsr-muted);
    }

    /* Pagination */
    .wsr-pagination {
      display: flex;
      justify-content: center;
      align-items: center;
      gap: 6px;
      margin-top: 24px;
      flex-wrap: wrap;
    }
    .wsr-page-btn {
      padding: 6px 12px;
      background: var(--wsr-card-bg);
      border: 1px solid var(--wsr-card-border);
      border-radius: 6px;
      color: var(--text, #e5e7eb);
      font-size: 13px;
      cursor: pointer;
      transition: background 0.15s;
    }
    .wsr-page-btn:hover:not(:disabled) {
      background: var(--wsr-card-hover);
    }
    .wsr-page-btn:disabled {
      opacity: 0.4;
      cursor: not-allowed;
    }
    .wsr-page-btn--active {
      background: var(--wsr-accent);
      color: #000;
      border-color: var(--wsr-accent);
      font-weight: 600;
    }
    .wsr-page-ellipsis {
      color: var(--wsr-muted);
      padding: 6px;
      font-size: 13px;
    }
    .wsr-page-info {
      font-size: 13px;
      color: var(--wsr-muted);
      margin-left: 8px;
    }

    /* Modal */
    .wsr-modal-overlay {
      position: fixed;
      inset: 0;
      background: rgba(0, 0, 0, 0.7);
      backdrop-filter: blur(4px);
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 24px;
      z-index: 1000;
      animation: wsr-fade-in 0.2s ease;
    }
    @keyframes wsr-fade-in {
      from {
        opacity: 0;
      }
      to {
        opacity: 1;
      }
    }
    .wsr-modal {
      background: var(--wsr-card-bg);
      border: 1px solid var(--wsr-card-border);
      border-radius: 14px;
      width: 100%;
      max-width: 560px;
      max-height: 90vh;
      overflow: hidden;
      display: flex;
      flex-direction: column;
      box-shadow: var(--wsr-shadow);
      animation: wsr-slide-up 0.25s cubic-bezier(0.16, 1, 0.3, 1);
    }
    .wsr-modal--narrow {
      max-width: 420px;
    }
    .wsr-modal--wide {
      max-width: 860px;
    }
    @keyframes wsr-slide-up {
      from {
        opacity: 0;
        transform: translateY(20px) scale(0.98);
      }
      to {
        opacity: 1;
        transform: translateY(0) scale(1);
      }
    }
    .wsr-modal-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 18px 20px;
      border-bottom: 1px solid var(--wsr-card-border);
    }
    .wsr-modal-header h2 {
      margin: 0;
      font-size: 17px;
      font-weight: 600;
      display: flex;
      align-items: center;
      gap: 10px;
    }
    .wsr-modal-close {
      background: transparent;
      border: none;
      color: var(--wsr-muted);
      font-size: 22px;
      line-height: 1;
      cursor: pointer;
      padding: 4px;
      width: 32px;
      height: 32px;
      display: flex;
      align-items: center;
      justify-content: center;
      border-radius: 8px;
      transition:
        background 0.15s,
        color 0.15s;
    }
    .wsr-modal-close:hover {
      background: rgba(255, 255, 255, 0.06);
      color: var(--text, #e5e7eb);
    }
    .wsr-modal-body {
      padding: 20px;
      overflow-y: auto;
      flex: 1;
    }
    .wsr-modal-footer {
      display: flex;
      justify-content: flex-end;
      gap: 10px;
      padding: 14px 20px;
      border-top: 1px solid var(--wsr-card-border);
    }

    /* Form fields */
    .wsr-field {
      display: flex;
      flex-direction: column;
      gap: 6px;
      margin-bottom: 16px;
    }
    .wsr-field:last-child {
      margin-bottom: 0;
    }
    .wsr-field span {
      font-size: 13px;
      font-weight: 500;
      color: var(--text, #e5e7eb);
    }
    .wsr-field input,
    .wsr-field select,
    .wsr-field textarea {
      padding: 10px 14px;
      background: var(--wsr-bg);
      border: 1px solid var(--wsr-card-border);
      border-radius: 8px;
      color: var(--text, #e5e7eb);
      font-size: 14px;
      outline: none;
      transition:
        border-color 0.2s,
        box-shadow 0.2s;
      font-family: inherit;
    }
    .wsr-field input:focus,
    .wsr-field select:focus,
    .wsr-field textarea:focus {
      border-color: var(--wsr-accent);
      box-shadow: 0 0 0 3px rgba(0, 212, 170, 0.08);
    }
    .wsr-field input::placeholder,
    .wsr-field textarea::placeholder {
      color: var(--wsr-muted);
      opacity: 0.6;
    }
    .wsr-field select:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
    .wsr-field textarea {
      resize: vertical;
      min-height: 60px;
    }

    /* Preview modal content */
    .wsr-preview-grid {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 20px;
    }
    @media (max-width: 640px) {
      .wsr-preview-grid {
        grid-template-columns: 1fr;
      }
    }
    .wsr-preview-info {
      display: flex;
      flex-direction: column;
      gap: 14px;
    }
    .wsr-preview-desc {
      margin: 0;
      font-size: 14px;
      color: var(--wsr-muted);
      line-height: 1.6;
    }
    .wsr-preview-meta {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .wsr-meta-row {
      display: flex;
      justify-content: space-between;
      align-items: center;
      font-size: 13px;
      padding: 6px 0;
      border-bottom: 1px solid var(--wsr-card-border);
    }
    .wsr-meta-row span:first-child {
      color: var(--wsr-muted);
    }
    .wsr-meta-row code {
      font-family: ui-monospace, SFMono-Regular, monospace;
      font-size: 12px;
      background: var(--wsr-bg);
      padding: 2px 8px;
      border-radius: 4px;
      color: var(--wsr-accent);
      max-width: 200px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .wsr-preview-tags {
      display: flex;
      gap: 6px;
      flex-wrap: wrap;
    }
    .wsr-preview-json {
      background: var(--wsr-bg);
      border-radius: 8px;
      border: 1px solid var(--wsr-card-border);
    }
    .wsr-preview-json summary {
      padding: 10px 14px;
      font-size: 13px;
      font-weight: 500;
      cursor: pointer;
      user-select: none;
    }
    .wsr-preview-json pre {
      margin: 0;
      padding: 10px 14px;
      font-size: 12px;
      color: var(--wsr-muted);
      overflow-x: auto;
      border-top: 1px solid var(--wsr-card-border);
    }

    .wsr-preview-scene,
    .wsr-preview-media {
      background: var(--wsr-bg);
      border-radius: 10px;
      border: 1px solid var(--wsr-card-border);
      min-height: 240px;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .wsr-preview-scene-placeholder,
    .wsr-preview-media-placeholder {
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 8px;
      color: var(--wsr-muted);
      text-align: center;
      padding: 24px;
    }
    .wsr-preview-hint {
      font-size: 12px;
      opacity: 0.6;
      max-width: 200px;
    }
  `;
}
