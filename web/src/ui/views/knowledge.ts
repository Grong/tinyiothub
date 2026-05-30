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
  parsed: { label: '已解析', cssClass: 'status--parsed' },
  pending: { label: '待解析', cssClass: 'status--pending' },
  failed: { label: '解析失败', cssClass: 'status--failed' },
  parsing: { label: '解析中', cssClass: 'status--parsing' },
};

const STATUS_OPTIONS = [
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

@customElement('view-knowledge')
export class KnowledgeView extends LitElement {
  createRenderRoot() { return this; }

  @state() loading = true;
  @state() error = '';
  @state() documents: KnowledgeDocument[] = [];
  @state() searchQuery = '';
  @state() statusFilter = '';

  // Editor modal
  @state() showEditor = false;
  @state() editingDoc: KnowledgeDocument | null = null;
  @state() editorTitle = '';
  @state() editorContent = '';
  @state() editorTags: string[] = [];
  @state() saving = false;
  @state() previewData: PreviewParseResponse | null = null;
  @state() previewLoading = false;
  @state() uploading = false;

  private _searchTimer: ReturnType<typeof setTimeout> | null = null;
  private _previewTimer: ReturnType<typeof setTimeout> | null = null;
  private _lastFocused: Element | null = null;

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

  private toggleStatusFilter(status: string) {
    this.statusFilter = this.statusFilter === status ? '' : status;
    this.loadDocuments();
  }

  // ── Editor ──

  private openEditor(doc?: KnowledgeDocument) {
    this._lastFocused = document.activeElement;
    this.editingDoc = doc ?? null;
    this.editorTitle = doc?.title ?? '';
    this.editorContent = doc?.content ?? '';
    this.editorTags = doc?.tags ? [...doc.tags] : [];
    this.previewData = null;
    this.showEditor = true;
    this.requestUpdate();
    // Focus title on new doc, textarea on existing
    requestAnimationFrame(() => {
      if (doc) {
        (this.querySelector('.kg-editor-textarea') as HTMLTextAreaElement)?.focus();
      } else {
        (this.querySelector('.kg-editor-title') as HTMLInputElement)?.focus();
      }
    });
  }

  private closeEditor() {
    this.showEditor = false;
    this.editingDoc = null;
    if (this._lastFocused && 'focus' in this._lastFocused) {
      (this._lastFocused as HTMLElement).focus();
    }
  }

  private onEditorKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') this.closeEditor();
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      this.handleSave();
    }
  }

  private onContentChange(e: Event) {
    this.editorContent = (e.target as HTMLTextAreaElement).value;
    if (this.editingDoc) {
      if (this._previewTimer) clearTimeout(this._previewTimer);
      this._previewTimer = setTimeout(() => this.loadPreview(), 500);
    }
  }

  private async loadPreview() {
    if (!this.editorContent.trim() || !this.editingDoc) return;
    this.previewLoading = true;
    try {
      const res = await knowledgeApi.previewParse(this.editingDoc.id, { content: this.editorContent });
      this.previewData = res.result;
    } catch {
      this.previewData = null;
    } finally {
      this.previewLoading = false;
    }
  }

  private async handleSave() {
    if (!this.editorTitle.trim()) { toastError('请输入文档标题'); return; }
    this.saving = true;
    try {
      if (this.editingDoc) {
        await knowledgeApi.updateDocument(this.editingDoc.id, {
          title: this.editorTitle.trim(), content: this.editorContent, tags: this.editorTags,
        });
        success('文档已更新');
      } else {
        await knowledgeApi.createDocument({
          title: this.editorTitle.trim(), content: this.editorContent, tags: this.editorTags,
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

  private async handleParse(doc: KnowledgeDocument, e: Event) {
    e.stopPropagation();
    try {
      await knowledgeApi.triggerParse(doc.id);
      success('解析任务已启动');
      this.loadDocuments();
    } catch (err: any) {
      toastError(err.message || '启动解析失败');
    }
  }

  // ── Tags ──

  private addTag(tag: string) {
    if (tag && !this.editorTags.includes(tag)) {
      this.editorTags = [...this.editorTags, tag];
    }
  }

  private removeTag(tag: string) {
    this.editorTags = this.editorTags.filter(t => t !== tag);
  }

  private onTagKeydown(e: KeyboardEvent) {
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

  // ── File upload ──

  private async onFileUpload(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    this.uploading = true;
    try {
      const result = await knowledgeApi.uploadFile(file);
      const filePath = (result.result as any).filePath || '';
      const ext = file.name.split('.').pop()?.toLowerCase() || '';

      let md = '';
      if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp'].includes(ext)) {
        md = `![${file.name}](${filePath})`;
      } else if (['mp4', 'webm', 'mov'].includes(ext)) {
        md = `<video src="${filePath}" controls></video>`;
      } else if (['glb', 'gltf'].includes(ext)) {
        md = `\`\`\`3d\n${filePath}\n\`\`\``;
      } else {
        md = `[${file.name}](${filePath})`;
      }

      const textarea = this.querySelector('.kg-editor-textarea') as HTMLTextAreaElement;
      if (textarea) {
        const start = textarea.selectionStart;
        const end = textarea.selectionEnd;
        this.editorContent =
          this.editorContent.slice(0, start) + '\n' + md + '\n' + this.editorContent.slice(end);
        requestAnimationFrame(() => {
          textarea.focus();
          textarea.selectionStart = textarea.selectionEnd = start + md.length + 2;
        });
      }
      success(`已上传 ${file.name}`);
    } catch (e: any) {
      toastError(e.message || '上传失败');
    } finally {
      this.uploading = false;
      input.value = '';
    }
  }

  private onEditorDrop(e: DragEvent) {
    e.preventDefault();
    const file = e.dataTransfer?.files?.[0];
    if (!file) return;
    const dt = new DataTransfer();
    dt.items.add(file);
    const input = document.createElement('input');
    input.type = 'file';
    input.files = dt.files;
    input.onchange = (ev) => this.onFileUpload(ev);
    input.dispatchEvent(new Event('change'));
  }

  private onEditorDragOver(e: DragEvent) {
    e.preventDefault();
  }

  // ═══════════════════════════════════════════
  //  RENDER
  // ═══════════════════════════════════════════

  render() {
    return html`
      <div class="knowledge-view">
        ${this.renderHeader()}
        ${this.renderFilterBar()}
        ${this.loading
          ? this.renderSkeleton()
          : this.error
            ? this.renderError()
            : this.documents.length === 0
              ? this.renderEmpty()
              : this.renderGrid()}
        ${this.showEditor ? this.renderEditorModal() : nothing}
      </div>
    `;
  }

  // ── Header ──

  private renderHeader() {
    return html`
      <div class="knowledge-header">
        <h2>知识文档</h2>
        <div class="knowledge-header-actions">
          <label class="btn btn-secondary" style="cursor:pointer">
            <input type="file" hidden @change=${this.onFileUpload}
              accept="image/*,.glb,.gltf,.mp4,.webm,.pdf,.zip" />
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
              <polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>
            </svg>
            ${this.uploading ? '上传中...' : '上传文件'}
          </label>
          <button class="btn btn-primary" @click=${() => this.openEditor()}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            新建文档
          </button>
        </div>
      </div>
    `;
  }

  // ── Filter bar ──

  private renderFilterBar() {
    return html`
      <div class="knowledge-filter-bar">
        <div class="knowledge-search">
          <svg class="knowledge-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16">
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          <input type="text" placeholder="搜索文档..." .value=${this.searchQuery} @input=${this.onSearchInput} />
        </div>
        <div class="knowledge-filter-chips">
          ${STATUS_OPTIONS.map(o => html`
            <button class="knowledge-filter-chip ${this.statusFilter === o.value ? 'active' : ''}"
              @click=${() => this.toggleStatusFilter(o.value)}>${o.label}</button>
          `)}
        </div>
      </div>
    `;
  }

  // ── Grid ──

  private renderGrid() {
    return html`
      <div class="knowledge-grid">
        ${this.documents.map(doc => this.renderCard(doc))}
      </div>
    `;
  }

  private renderCard(doc: KnowledgeDocument) {
    const status = STATUS_CONFIG[doc.parseStatus] || { label: doc.parseStatus, cssClass: '' };
    return html`
      <div class="knowledge-card card" @click=${() => this.openEditor(doc)}>
        <div class="knowledge-card-header">
          <span class="knowledge-status ${status.cssClass}">
            ${doc.parseStatus === 'parsing' ? html`<span class="knowledge-spinner"></span>` : nothing}
            ${status.label}
          </span>
          <div class="knowledge-card-actions">
            ${doc.parseStatus !== 'parsing'
              ? html`<button class="knowledge-action-btn" title="解析"
                  @click=${(e: Event) => this.handleParse(doc, e)}>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13">
                    <polygon points="5 3 19 12 5 21 5 3"/>
                  </svg>
                </button>`
              : nothing}
            <button class="knowledge-action-btn knowledge-action-btn--danger" title="删除"
              @click=${(e: Event) => { e.stopPropagation(); this.handleDelete(doc); }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13">
                <polyline points="3 6 5 6 21 6"/>
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
              </svg>
            </button>
          </div>
        </div>
        <div class="knowledge-card-body">
          <h3 class="knowledge-card-title">${doc.title}</h3>
          <p class="knowledge-card-desc">${doc.content.replace(/^#+\s/gm, '').trim().slice(0, 120) || '暂无内容'}</p>
        </div>
        <div class="knowledge-card-footer">
          <div class="knowledge-card-tags">
            ${doc.tags.length > 0
              ? doc.tags.slice(0, 3).map(t => html`<span class="knowledge-tag">${t}</span>`)
              : html`<span class="knowledge-tag knowledge-tag--empty">无标签</span>`}
            ${doc.tags.length > 3 ? html`<span class="knowledge-tag">+${doc.tags.length - 3}</span>` : nothing}
          </div>
          <span class="knowledge-card-date">${formatDate(doc.updatedAt)}</span>
        </div>
      </div>
    `;
  }

  // ── Empty / Error / Loading ──

  private renderSkeleton() {
    return html`
      <div class="knowledge-grid">
        ${Array.from({ length: 6 }).map(() => html`
          <div class="knowledge-card card knowledge-card--skeleton">
            <div class="knowledge-skeleton-badge"></div>
            <div class="knowledge-skeleton-line knowledge-skeleton-line--lg"></div>
            <div class="knowledge-skeleton-line"></div>
            <div class="knowledge-skeleton-line knowledge-skeleton-line--sm"></div>
          </div>
        `)}
      </div>
    `;
  }

  private renderError() {
    return html`
      <div class="empty-center">
        <p class="empty-center__text" style="color:var(--danger)">${this.error}</p>
        <button class="btn btn-secondary" @click=${this.loadDocuments}>重试</button>
      </div>
    `;
  }

  private renderEmpty() {
    return html`
      <div class="knowledge-empty">
        <svg class="knowledge-empty-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" width="56" height="56">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <polyline points="14 2 14 8 20 8"/>
          <line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/>
        </svg>
        <h3>暂无知识文档</h3>
        <p>创建 Markdown 文档来描述您的物联网场景，AI 将自动抽取空间、设备、关系等结构化知识。</p>
        <div class="knowledge-empty-actions">
          <button class="btn btn-primary" @click=${() => this.openEditor()}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            新建文档
          </button>
          <label class="btn btn-secondary" style="cursor:pointer">
            <input type="file" hidden @change=${this.onFileUpload}
              accept="image/*,.glb,.gltf,.mp4,.webm,.pdf,.zip" />
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
              <polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>
            </svg>
            上传文件
          </label>
        </div>
      </div>
    `;
  }

  // ── Editor Modal ──

  private renderEditorModal() {
    return html`
      <div class="modal-overlay" @click=${(e: Event) => { if (e.target === e.currentTarget) this.closeEditor(); }}
        @keydown=${this.onEditorKeydown}>
        <div class="modal-box knowledge-editor-modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <h3>${this.editingDoc ? '编辑文档' : '新建文档'}</h3>
            <button class="modal-close" @click=${this.closeEditor}>&times;</button>
          </div>
          <div class="modal-body knowledge-editor-body">
            <input type="text" class="kg-editor-title"
              placeholder="文档标题" .value=${this.editorTitle}
              @input=${(e: Event) => { this.editorTitle = (e.target as HTMLInputElement).value; }} />
            <div class="kg-editor-main">
              <textarea class="kg-editor-textarea"
                placeholder="在此编写 Markdown 知识文档..."
                .value=${this.editorContent}
                @input=${this.onContentChange}
                @drop=${this.onEditorDrop}
                @dragover=${this.onEditorDragOver}></textarea>
              <div class="kg-editor-drop-hint">拖拽文件到此处上传</div>
            </div>
            ${this.editingDoc ? this.renderPreviewPanel() : nothing}
          </div>
          <div class="modal-footer" style="justify-content:space-between;flex-wrap:wrap;gap:8px;">
            <div class="kg-editor-tags">
              ${this.editorTags.map(tag => html`
                <span class="kg-editor-tag">
                  ${tag}<button @click=${() => this.removeTag(tag)}>&times;</button>
                </span>
              `)}
              <input type="text" class="kg-editor-tag-input"
                placeholder="输入标签, 回车添加..." @keydown=${this.onTagKeydown} />
            </div>
            <div style="display:flex;gap:8px;align-items:center;">
              <label class="btn btn-secondary" style="cursor:pointer;font-size:13px;">
                <input type="file" hidden @change=${this.onFileUpload}
                  accept="image/*,.glb,.gltf,.mp4,.webm,.pdf,.zip" />
                ${this.uploading ? '上传中...' : '附件'}
              </label>
              <button class="btn btn-secondary" @click=${this.closeEditor}>取消</button>
              <button class="btn btn-primary" ?disabled=${this.saving || !this.editorTitle.trim()}
                @click=${this.handleSave}>${this.saving ? '保存中...' : '保存'}</button>
            </div>
          </div>
        </div>
      </div>
    `;
  }

  private renderPreviewPanel() {
    if (this.previewLoading) {
      return html`<div class="kg-preview-panel"><div class="kg-preview-empty">解析中...</div></div>`;
    }
    if (!this.previewData) {
      return html`<div class="kg-preview-panel"><div class="kg-preview-empty">编辑后将在此预览解析结果</div></div>`;
    }
    const { entities, relations } = this.previewData;
    return html`
      <div class="kg-preview-panel">
        ${entities.length > 0 ? html`
          <div class="kg-preview-section">
            <div class="kg-preview-label">实体 (${entities.length})</div>
            ${entities.map(e => html`
              <div class="kg-preview-item">
                <span class="kg-preview-item-type ${e.entityType}">${e.entityType}</span>
                <span class="kg-preview-item-name">${e.name}</span>
              </div>
            `)}
          </div>
        ` : html`<div class="kg-preview-empty">未检测到实体</div>`}
        ${relations.length > 0 ? html`
          <div class="kg-preview-section">
            <div class="kg-preview-label">关系 (${relations.length})</div>
            ${relations.map(r => html`
              <div class="kg-preview-rel">
                <span>${r.sourceEntityId.slice(0, 6)}</span>
                <span class="kg-preview-rel-type">${r.relationType}</span>
                <span>${r.targetEntityId.slice(0, 6)}</span>
              </div>
            `)}
          </div>
        ` : nothing}
      </div>
    `;
  }
}
