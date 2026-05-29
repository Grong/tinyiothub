import { LitElement, html, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import {
  knowledgeApi,
  type KnowledgeDocument,
  type PreviewParseResponse,
} from '../../api/knowledge.js';
import { success, error as toastError } from '../components/toast.js';
import '../../styles/views/knowledge.css';

const STATUS_CONFIG: Record<string, { label: string; cssClass: string }> = {
  parsed: { label: '已解析', cssClass: 'knowledge-doc-card-status--parsed' },
  pending: { label: '待解析', cssClass: 'knowledge-doc-card-status--pending' },
  failed: { label: '解析失败', cssClass: 'knowledge-doc-card-status--failed' },
  parsing: { label: '解析中', cssClass: 'knowledge-doc-card-status--parsing' },
};

const STATUS_FILTERS = [
  { value: '', label: '全部' },
  { value: 'parsed', label: '已解析' },
  { value: 'pending', label: '待解析' },
  { value: 'failed', label: '解析失败' },
];

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString('zh-CN', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  } catch { return iso; }
}

function getExcerpt(content: string): string {
  return content.replace(/^#+\s/gm, '').trim().slice(0, 120);
}

@customElement('knowledge-view')
export class KnowledgeView extends LitElement {
  createRenderRoot() { return this; }

  @state() documents: KnowledgeDocument[] = [];
  @state() loading = true;
  @state() error = '';
  @state() searchQuery = '';
  @state() statusFilter = '';
  @state() tagFilter = '';

  @state() editorOpen = false;
  @state() editingDoc: KnowledgeDocument | null = null;
  @state() editorTitle = '';
  @state() editorContent = '';
  @state() editorTags: string[] = [];
  @state() saving = false;
  @state() previewData: PreviewParseResponse | null = null;
  @state() previewLoading = false;

  private _searchTimer: ReturnType<typeof setTimeout> | null = null;
  private _previewTimer: ReturnType<typeof setTimeout> | null = null;
  private _lastFocusedElement: Element | null = null;

  connectedCallback() {
    super.connectedCallback();
    this.loadDocuments();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    if (this._searchTimer) clearTimeout(this._searchTimer);
    if (this._previewTimer) clearTimeout(this._previewTimer);
  }

  // ── Data ──

  private async loadDocuments() {
    this.loading = true;
    this.error = '';
    try {
      const params: Record<string, string> = {};
      if (this.searchQuery.trim()) params.q = this.searchQuery.trim();
      if (this.statusFilter) params.status = this.statusFilter;
      if (this.tagFilter) params.tags = this.tagFilter;
      const res = await knowledgeApi.listDocuments(params);
      this.documents = res.result?.data ?? [];
    } catch (e: any) {
      this.error = e.message || '加载文档失败';
    } finally {
      this.loading = false;
    }
  }

  private onSearchInput(e: Event) {
    this.searchQuery = (e.target as HTMLInputElement).value;
    if (this._searchTimer) clearTimeout(this._searchTimer);
    this._searchTimer = setTimeout(() => this.loadDocuments(), 200);
  }

  private setStatusFilter(status: string) {
    this.statusFilter = this.statusFilter === status ? '' : status;
    this.loadDocuments();
  }

  // ── Editor ──

  private openNewDocument() {
    this.editingDoc = null;
    this.editorTitle = '';
    this.editorContent = '';
    this.editorTags = [];
    this.previewData = null;
    this.editorOpen = true;
    this.saveFocus();
    this.requestUpdate();
    requestAnimationFrame(() => {
      const title = this.querySelector('.knowledge-editor-title-input') as HTMLInputElement;
      title?.focus();
    });
  }

  private openDocument(doc: KnowledgeDocument) {
    this.editingDoc = doc;
    this.editorTitle = doc.title;
    this.editorContent = doc.content;
    this.editorTags = [...doc.tags];
    this.previewData = null;
    this.editorOpen = true;
    this.saveFocus();
    this.requestUpdate();
    this.debouncedPreview();
    requestAnimationFrame(() => {
      const textarea = this.querySelector('.knowledge-editor-textarea') as HTMLTextAreaElement;
      textarea?.focus();
    });
  }

  private closeEditor() {
    this.editorOpen = false;
    this.editingDoc = null;
    this.editorTitle = '';
    this.editorContent = '';
    this.editorTags = [];
    this.previewData = null;
    this.restoreFocus();
  }

  private saveFocus() {
    this._lastFocusedElement = document.activeElement;
  }

  private restoreFocus() {
    if (this._lastFocusedElement && 'focus' in this._lastFocusedElement) {
      (this._lastFocusedElement as HTMLElement).focus();
    }
  }

  private onEditorKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      this.closeEditor();
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      this.handleSave();
    }
  }

  private debouncedPreview() {
    if (this._previewTimer) clearTimeout(this._previewTimer);
    this._previewTimer = setTimeout(() => this.loadPreview(), 500);
  }

  private async loadPreview() {
    if (!this.editorContent.trim() || !this.editingDoc) return;
    this.previewLoading = true;
    try {
      const res = await knowledgeApi.previewParse(this.editingDoc.id, {
        content: this.editorContent,
      });
      this.previewData = res.result;
    } catch {
      this.previewData = null;
    } finally {
      this.previewLoading = false;
    }
  }

  private onContentChange(e: Event) {
    this.editorContent = (e.target as HTMLTextAreaElement).value;
    if (this.editingDoc) this.debouncedPreview();
  }

  private async handleSave() {
    if (!this.editorTitle.trim()) {
      toastError('请输入文档标题');
      return;
    }
    this.saving = true;
    try {
      if (this.editingDoc) {
        await knowledgeApi.updateDocument(this.editingDoc.id, {
          title: this.editorTitle.trim(),
          content: this.editorContent,
          tags: this.editorTags,
        });
        success('文档已更新');
      } else {
        await knowledgeApi.createDocument({
          title: this.editorTitle.trim(),
          content: this.editorContent,
          tags: this.editorTags,
        });
        success('文档已创建');
      }
      this.closeEditor();
      this.loadDocuments();
    } catch (e: any) {
      toastError(e.message || '保存失败');
    } finally {
      this.saving = false;
    }
  }

  private async handleDelete(doc: KnowledgeDocument) {
    if (!confirm(`确定要删除文档「${doc.title}」吗？此操作不可撤销。`)) return;
    try {
      await knowledgeApi.deleteDocument(doc.id);
      success('文档已删除');
      this.loadDocuments();
    } catch (e: any) {
      toastError(e.message || '删除失败');
    }
  }

  private addTag(tag: string) {
    if (tag && !this.editorTags.includes(tag)) {
      this.editorTags = [...this.editorTags, tag];
    }
  }

  private removeTag(tag: string) {
    this.editorTags = this.editorTags.filter(t => t !== tag);
  }

  private onTagInputKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      const input = e.target as HTMLInputElement;
      const tag = input.value.trim();
      if (tag) this.addTag(tag);
      input.value = '';
    }
    if (e.key === 'Backspace' && !(e.target as HTMLInputElement).value && this.editorTags.length > 0) {
      this.editorTags = this.editorTags.slice(0, -1);
    }
  }

  // ── Render ──

  render() {
    return html`
      <div class="knowledge-view">
        ${this.renderHeader()}
        ${this.renderFilters()}
        ${this.loading
          ? this.renderSkeleton()
          : this.error
            ? this.renderError()
            : this.documents.length === 0
              ? this.renderEmpty()
              : this.renderDocumentGrid()}
        ${this.editorOpen ? this.renderEditor() : nothing}
      </div>
    `;
  }

  private renderHeader() {
    return html`
      <div class="knowledge-header">
        <h2>知识文档</h2>
        <div class="knowledge-header-actions">
          <button class="knowledge-btn knowledge-btn-primary" @click=${this.openNewDocument}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            新建文档
          </button>
        </div>
      </div>
    `;
  }

  private renderFilters() {
    return html`
      <div class="knowledge-filter-bar">
        <div class="knowledge-search">
          <svg class="knowledge-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16">
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          <input type="text" placeholder="搜索文档..." .value=${this.searchQuery} @input=${this.onSearchInput} />
        </div>
        <div class="knowledge-filter-chips">
          ${STATUS_FILTERS.map(f => html`
            <button
              class="knowledge-filter-chip ${this.statusFilter === f.value ? 'active' : ''}"
              @click=${() => this.setStatusFilter(f.value)}>
              ${f.label}
            </button>
          `)}
        </div>
      </div>
    `;
  }

  private renderSkeleton() {
    return html`
      <div class="knowledge-doc-grid">
        ${Array.from({ length: 6 }).map(() => html`
          <div class="knowledge-skeleton-card">
            <div class="knowledge-skeleton-badge"></div>
            <div class="knowledge-skeleton-title"></div>
            <div class="knowledge-skeleton-line"></div>
            <div class="knowledge-skeleton-line knowledge-skeleton-line--short"></div>
          </div>
        `)}
      </div>
    `;
  }

  private renderError() {
    return html`
      <div class="knowledge-error">
        <p class="knowledge-error-message">${this.error}</p>
        <button class="knowledge-btn knowledge-btn-secondary" @click=${this.loadDocuments}>重试</button>
      </div>
    `;
  }

  private renderEmpty() {
    const hasFilters = !!(this.searchQuery || this.statusFilter);
    return html`
      <div class="knowledge-empty">
        <svg class="knowledge-empty-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" width="64" height="64">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <polyline points="14 2 14 8 20 8"/>
          <line x1="16" y1="13" x2="8" y2="13"/>
          <line x1="16" y1="17" x2="8" y2="17"/>
          <line x1="10" y1="9" x2="8" y2="9"/>
        </svg>
        ${hasFilters
          ? html`
            <h3 class="knowledge-empty-title">未找到匹配的文档</h3>
            <p class="knowledge-empty-desc">尝试调整搜索条件或筛选器</p>
          `
          : html`
            <h3 class="knowledge-empty-title">暂无知识文档</h3>
            <p class="knowledge-empty-desc">创建文档来描述您的物联网场景，AI 将自动抽取其中的实体和关系，构建知识图谱。</p>
            <button class="knowledge-btn knowledge-btn-primary" @click=${this.openNewDocument}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16">
                <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
              </svg>
              创建第一篇文档
            </button>
          `}
      </div>
    `;
  }

  private renderDocumentGrid() {
    return html`
      <div class="knowledge-doc-grid">
        ${this.documents.map(doc => this.renderDocumentCard(doc))}
      </div>
    `;
  }

  private renderDocumentCard(doc: KnowledgeDocument) {
    const statusCfg = STATUS_CONFIG[doc.parseStatus] || { label: doc.parseStatus, cssClass: '' };
    return html`
      <div class="knowledge-doc-card" @click=${() => this.openDocument(doc)}>
        <div class="knowledge-doc-card-header">
          <span class="knowledge-doc-card-status ${statusCfg.cssClass}">
            ${doc.parseStatus === 'parsing'
              ? html`<span class="knowledge-spinner"></span>${statusCfg.label}`
              : statusCfg.label}
          </span>
          <button
            class="knowledge-doc-card-delete"
            title="删除"
            @click=${(e: Event) => { e.stopPropagation(); this.handleDelete(doc); }}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
              <polyline points="3 6 5 6 21 6"/>
              <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
            </svg>
          </button>
        </div>
        <div class="knowledge-doc-card-body">
          <h3 class="knowledge-doc-card-title" title=${doc.title}>${doc.title}</h3>
          <p class="knowledge-doc-card-excerpt">${getExcerpt(doc.content) || '暂无内容'}</p>
        </div>
        <div class="knowledge-doc-card-footer">
          <div class="knowledge-doc-card-tags">
            ${doc.tags.length > 0
              ? doc.tags.slice(0, 3).map(t => html`<span class="knowledge-doc-card-tag">${t}</span>`)
              : html`<span class="knowledge-doc-card-tag" style="opacity:0.4">无标签</span>`}
          </div>
          <span class="knowledge-doc-card-date">${formatDate(doc.updatedAt)}</span>
        </div>
      </div>
    `;
  }

  // ── Editor ──

  private renderEditor() {
    return html`
      <div class="knowledge-editor-overlay"
        @click=${(e: Event) => { if (e.target === e.currentTarget) this.closeEditor(); }}
        @keydown=${this.onEditorKeydown}>
        <div class="knowledge-editor" @click=${(e: Event) => e.stopPropagation()}>
          <div class="knowledge-editor-header">
            <h3>${this.editingDoc ? '编辑文档' : '新建文档'}</h3>
            <button class="knowledge-editor-close" @click=${this.closeEditor} aria-label="关闭">&times;</button>
          </div>
          <div class="knowledge-editor-body">
            <div class="knowledge-editor-main">
              <input
                type="text"
                class="knowledge-editor-title-input"
                placeholder="文档标题"
                .value=${this.editorTitle}
                @input=${(e: Event) => { this.editorTitle = (e.target as HTMLInputElement).value; }} />
              <textarea
                class="knowledge-editor-textarea"
                placeholder="在此编写知识文档，AI 将自动抽取实体和关系...

示例格式：

# 园区概况
智慧工厂园区总面积约 12.5 万平方米，包含生产区、仓储区、办公区。

# 空间结构
- 生产区：东侧，5 万㎡，含 A/B/C 三个车间
- 仓储区：北侧，2 万㎡
- 办公区：南侧，1.5 万㎡

# 设备清单
- 数控机床 x20（A 车间）
- 工业机器人 x15（B 车间）
- 温湿度传感器 x50（各区域）"
                .value=${this.editorContent}
                @input=${this.onContentChange}></textarea>
            </div>
            <div class="knowledge-editor-preview">
              <div class="knowledge-editor-preview-header">实时预览</div>
              ${this.renderPreviewContent()}
            </div>
          </div>
          <div class="knowledge-editor-footer">
            <div class="knowledge-editor-tags">
              ${this.editorTags.map(tag => html`
                <span class="knowledge-editor-tag">
                  ${tag}
                  <button class="knowledge-editor-tag-remove" @click=${() => this.removeTag(tag)} aria-label="移除标签 ${tag}">&times;</button>
                </span>
              `)}
              <input
                type="text"
                class="knowledge-editor-tag-input"
                placeholder="输入标签后按回车..."
                @keydown=${this.onTagInputKeydown} />
            </div>
            <div class="knowledge-editor-actions">
              <span style="font-size:12px;color:var(--muted);margin-right:var(--space-2)">Esc 关闭 · Ctrl+Enter 保存</span>
              <button class="knowledge-btn knowledge-btn-secondary" @click=${this.closeEditor}>取消</button>
              <button
                class="knowledge-btn knowledge-btn-primary"
                ?disabled=${this.saving || !this.editorTitle.trim()}
                @click=${this.handleSave}>
                ${this.saving ? html`<span class="knowledge-spinner"></span>保存中...` : '保存'}
              </button>
            </div>
          </div>
        </div>
      </div>
    `;
  }

  private renderPreviewContent() {
    if (this.previewLoading) {
      return html`<div class="knowledge-preview-empty"><span class="knowledge-spinner"></span>解析中...</div>`;
    }

    if (!this.editingDoc && !this.editorContent.trim()) {
      return html`
        <div class="knowledge-preview-empty">
          新建文档保存后，可在此处预览 AI 解析出的实体和关系。
        </div>
      `;
    }

    if (!this.editingDoc && this.editorContent.trim()) {
      return html`
        <div class="knowledge-preview-empty">
          保存文档后，AI 将自动解析实体和关系。
        </div>
      `;
    }

    if (!this.previewData) {
      return html`
        <div class="knowledge-preview-empty">
          ${this.editorContent.trim() ? '未能加载预览数据' : '输入内容后将在此处预览解析结果'}
        </div>
      `;
    }

    const { entities, relations } = this.previewData;

    return html`
      ${entities.length > 0 ? html`
        <div class="knowledge-preview-section">
          <div class="knowledge-preview-section-title">实体 (${entities.length})</div>
          ${entities.map(e => html`
            <div class="knowledge-preview-entity">
              <div class="knowledge-preview-entity-name">${e.name}</div>
              <div class="knowledge-preview-entity-type">${e.entityType}</div>
              ${e.description ? html`<div class="knowledge-preview-entity-desc">${e.description}</div>` : nothing}
            </div>
          `)}
        </div>
      ` : html`<div class="knowledge-preview-empty">未检测到实体</div>`}

      ${relations.length > 0 ? html`
        <div class="knowledge-preview-section">
          <div class="knowledge-preview-section-title">关系 (${relations.length})</div>
          ${relations.map(r => html`
            <div class="knowledge-preview-relation">
              <span>${r.sourceEntityId}</span>
              <span class="knowledge-preview-relation-type">${r.relationType}</span>
              <span>${r.targetEntityId}</span>
            </div>
          `)}
        </div>
      ` : nothing}
    `;
  }
}
