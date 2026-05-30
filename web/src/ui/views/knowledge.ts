import { LitElement, html, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import {
  knowledgeApi,
  type KnowledgeDocument,
  type KnowledgeEntity,
  type KnowledgeRelation,
  type PreviewParseResponse,
} from '../../api/knowledge.js';
import { success, error as toastError } from '../components/toast.js';
import '../../styles/views/knowledge.css';

// ── Config ──

const STATUS_CONFIG: Record<string, { label: string; cssClass: string }> = {
  parsed: { label: '已解析', cssClass: 'kg-status--parsed' },
  pending: { label: '待解析', cssClass: 'kg-status--pending' },
  failed: { label: '解析失败', cssClass: 'kg-status--failed' },
  parsing: { label: '解析中', cssClass: 'kg-status--parsing' },
};

const ENTITY_TYPE_CONFIG: Record<string, { label: string; icon: string }> = {
  space: { label: '空间', icon: '◇' },
  device: { label: '设备', icon: '◆' },
  functional: { label: '功能', icon: '◈' },
};

const ENTITY_TYPE_OPTIONS = [
  { value: '', label: '全部' },
  { value: 'space', label: '空间' },
  { value: 'device', label: '设备' },
  { value: 'functional', label: '功能' },
];

const TABS = [
  { key: 'documents', label: '文档' },
  { key: 'entities', label: '实体' },
  { key: 'relations', label: '关系' },
] as const;

type TabKey = (typeof TABS)[number]['key'];

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString('zh-CN', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  } catch { return iso; }
}

function entityTypeConfig(entityType: string) {
  return ENTITY_TYPE_CONFIG[entityType] || { label: entityType, icon: '○' };
}

// ── Component ──

@customElement('view-knowledge')
export class KnowledgeView extends LitElement {
  createRenderRoot() { return this; }

  // Documents
  @state() documents: KnowledgeDocument[] = [];
  @state() loading = true;
  @state() error = '';

  // Entities & Relations
  @state() entities: KnowledgeEntity[] = [];
  @state() relations: KnowledgeRelation[] = [];
  @state() entityTypeFilter = '';

  // UI
  @state() activeTab: TabKey = 'documents';
  @state() searchQuery = '';
  @state() statusFilter = '';

  // Editor (inline, not modal)
  @state() editingDoc: KnowledgeDocument | null = null;
  @state() isNewDoc = false;
  @state() editorTitle = '';
  @state() editorContent = '';
  @state() editorTags: string[] = [];
  @state() saving = false;
  @state() previewData: PreviewParseResponse | null = null;
  @state() previewLoading = false;
  @state() uploading = false;

  private _searchTimer: ReturnType<typeof setTimeout> | null = null;
  private _previewTimer: ReturnType<typeof setTimeout> | null = null;

  connectedCallback() {
    super.connectedCallback();
    this.loadDocuments();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    if (this._searchTimer) clearTimeout(this._searchTimer);
    if (this._previewTimer) clearTimeout(this._previewTimer);
  }

  // ── Data loading ──

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

  private async loadEntities() {
    try {
      const params: Record<string, string> = {};
      if (this.entityTypeFilter) params.entityType = this.entityTypeFilter;
      const res = await knowledgeApi.listEntities(params);
      this.entities = res.result ?? [];
    } catch { /* silent */ }
  }

  private async loadRelations() {
    try {
      const res = await knowledgeApi.listRelations();
      this.relations = res.result ?? [];
    } catch { /* silent */ }
  }

  private onSearchInput(e: Event) {
    this.searchQuery = (e.target as HTMLInputElement).value;
    if (this._searchTimer) clearTimeout(this._searchTimer);
    this._searchTimer = setTimeout(() => this.loadDocuments(), 200);
  }

  // ── Tab switching ──

  private switchTab(tab: TabKey) {
    this.activeTab = tab;
    this.editingDoc = null;
    this.isNewDoc = false;
    if (tab === 'entities') this.loadEntities();
    if (tab === 'relations') this.loadRelations();
  }

  // ── Editor ──

  private openNewDoc() {
    this.isNewDoc = true;
    this.editingDoc = null;
    this.editorTitle = '';
    this.editorContent = '';
    this.editorTags = [];
    this.previewData = null;
    this.requestUpdate();
  }

  private openDoc(doc: KnowledgeDocument) {
    this.isNewDoc = false;
    this.editingDoc = doc;
    this.editorTitle = doc.title;
    this.editorContent = doc.content;
    this.editorTags = [...doc.tags];
    this.previewData = null;
    this.requestUpdate();
    this.debouncedPreview();
  }

  private closeEditor() {
    this.editingDoc = null;
    this.isNewDoc = false;
    this.editorTitle = '';
    this.editorContent = '';
    this.editorTags = [];
    this.previewData = null;
  }

  private onEditorKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') this.closeEditor();
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
      const res = await knowledgeApi.previewParse(this.editingDoc.id, { content: this.editorContent });
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
      if (this.editingDoc?.id === doc.id) this.closeEditor();
      this.loadDocuments();
    } catch (e: any) {
      toastError(e.message || '删除失败');
    }
  }

  private async handleParse(doc: KnowledgeDocument) {
    try {
      await knowledgeApi.triggerParse(doc.id);
      success('解析任务已启动');
      this.loadDocuments();
    } catch (e: any) {
      toastError(e.message || '启动解析失败');
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

  // ── File upload ──

  private async handleFileUpload(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    this.uploading = true;
    try {
      const result = await knowledgeApi.uploadFile(file);
      const filePath = (result.result as any).filePath || '';
      const ext = file.name.split('.').pop()?.toLowerCase() || '';
      const isImage = ['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp'].includes(ext);
      const isVideo = ['mp4', 'webm', 'mov'].includes(ext);
      const isModel = ['glb', 'gltf'].includes(ext);

      let md = '';
      if (isImage) md = `![${file.name}](${filePath})`;
      else if (isVideo) md = `<video src="${filePath}" controls></video>`;
      else if (isModel) md = `\`\`\`3d\n${filePath}\n\`\`\``;
      else md = `[${file.name}](${filePath})`;

      // If editor is open, insert at cursor; otherwise open a new doc
      if (this.isNewDoc || this.editingDoc) {
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
      } else {
        this.editorContent = md;
        this.openNewDoc();
      }
      success(`已上传 ${file.name}`);
    } catch (e: any) {
      toastError(e.message || '上传失败');
    } finally {
      this.uploading = false;
      input.value = '';
    }
  }

  private onDrop(e: DragEvent) {
    e.preventDefault();
    const file = e.dataTransfer?.files?.[0];
    if (!file) return;
    const dt = new DataTransfer();
    dt.items.add(file);
    const input = document.createElement('input');
    input.type = 'file';
    input.files = dt.files;
    input.onchange = (ev) => this.handleFileUpload(ev);
    input.dispatchEvent(new Event('change'));
  }

  private onDragOver(e: DragEvent) {
    e.preventDefault();
    (e.currentTarget as HTMLElement).classList.add('kg-drop-active');
  }

  private onDragLeave(e: DragEvent) {
    (e.currentTarget as HTMLElement).classList.remove('kg-drop-active');
  }

  // ── Entity helpers ──

  private getEntityById(id: string): KnowledgeEntity | undefined {
    return this.entities.find(e => e.id === id);
  }

  // ═══════════════════════════════════════════════════
  //  RENDER
  // ═══════════════════════════════════════════════════

  render() {
    const inEditor = this.isNewDoc || this.editingDoc;
    return html`
      <div class="kg-view" @keydown=${this.onEditorKeydown}>
        ${this.renderHeader()}
        ${!inEditor ? this.renderTabs() : nothing}
        ${inEditor
          ? this.renderEditor()
          : this.activeTab === 'documents' ? this.renderDocumentsTab()
          : this.activeTab === 'entities' ? this.renderEntitiesTab()
          : this.renderRelationsTab()}
        ${!inEditor ? this.renderStatusBar() : nothing}
      </div>
    `;
  }

  // ── Header ──

  private renderHeader() {
    const inEditor = this.isNewDoc || this.editingDoc;
    return html`
      <div class="kg-header">
        <div class="kg-header-left">
          ${inEditor
            ? html`
              <button class="kg-back-btn" @click=${this.closeEditor} aria-label="返回">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18">
                  <polyline points="15 18 9 12 15 6"/>
                </svg>
              </button>
              <h2 class="kg-title">${this.isNewDoc ? '新建文档' : '编辑文档'}</h2>
            `
            : html`<h2 class="kg-title">知识图谱</h2>`}
        </div>
        <div class="kg-header-right">
          ${inEditor
            ? html`
              <label class="kg-btn kg-btn-ghost kg-upload-label">
                <input type="file" hidden @change=${this.handleFileUpload}
                  accept="image/*,.glb,.gltf,.mp4,.webm,.pdf,.zip" />
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
                  <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                  <polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>
                </svg>
                ${this.uploading ? '上传中...' : '附件'}
              </label>
              <button class="kg-btn kg-btn-primary"
                ?disabled=${this.saving || !this.editorTitle.trim()}
                @click=${this.handleSave}>
                ${this.saving ? '保存中...' : '保存'}
              </button>
            `
            : html`
              <label class="kg-btn kg-btn-ghost kg-upload-label">
                <input type="file" hidden @change=${this.handleFileUpload}
                  accept="image/*,.glb,.gltf,.mp4,.webm,.pdf,.zip" />
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
                  <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                  <polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>
                </svg>
                ${this.uploading ? '上传中...' : '上传文件'}
              </label>
              <button class="kg-btn kg-btn-primary" @click=${this.openNewDoc}>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
                  <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
                </svg>
                新建文档
              </button>
            `}
        </div>
      </div>
    `;
  }

  // ── Tabs ──

  private renderTabs() {
    const counts: Record<TabKey, number> = {
      documents: this.documents.length,
      entities: this.entities.length,
      relations: this.relations.length,
    };
    return html`
      <div class="kg-tabs">
        ${TABS.map(t => html`
          <button class="kg-tab ${this.activeTab === t.key ? 'active' : ''}"
            @click=${() => this.switchTab(t.key)}>
            ${t.label}
            <span class="kg-tab-count">${counts[t.key]}</span>
          </button>
        `)}
        <div class="kg-tabs-spacer"></div>
        ${this.activeTab === 'documents' ? this.renderDocFilters() : nothing}
        ${this.activeTab === 'entities' ? this.renderEntityFilters() : nothing}
      </div>
    `;
  }

  private renderDocFilters() {
    return html`
      <div class="kg-search">
        <svg class="kg-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
          <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
        </svg>
        <input type="text" placeholder="搜索文档..." .value=${this.searchQuery} @input=${this.onSearchInput} />
      </div>
    `;
  }

  private renderEntityFilters() {
    return html`
      <div class="kg-filter-chips">
        ${ENTITY_TYPE_OPTIONS.map(o => html`
          <button class="kg-chip ${this.entityTypeFilter === o.value ? 'active' : ''}"
            @click=${() => { this.entityTypeFilter = this.entityTypeFilter === o.value ? '' : o.value; this.loadEntities(); }}>
            ${o.label}
          </button>
        `)}
      </div>
    `;
  }

  // ── Status bar ──

  private renderStatusBar() {
    // Load entities/relations on first render
    if (this.entities.length === 0 && this.activeTab === 'documents') {
      setTimeout(() => { this.loadEntities(); this.loadRelations(); }, 100);
    }
    const lastUpdated = this.documents.length > 0
      ? formatDate(this.documents.reduce((a, b) => a.updatedAt > b.updatedAt ? a : b).updatedAt)
      : '暂无';
    return html`
      <div class="kg-statusbar">
        <span>${this.documents.length} 个文档</span>
        <span class="kg-statusbar-sep">·</span>
        <span>${this.entities.length} 个实体</span>
        <span class="kg-statusbar-sep">·</span>
        <span>${this.relations.length} 个关系</span>
        <span class="kg-statusbar-sep">·</span>
        <span>最后更新 ${lastUpdated}</span>
      </div>
    `;
  }

  // ═══════════════════════════════════════════════════
  //  DOCUMENTS TAB
  // ═══════════════════════════════════════════════════

  private renderDocumentsTab() {
    if (this.loading) return this.renderSkeleton();
    if (this.error) return this.renderError();
    if (this.documents.length === 0) return this.renderEmpty();
    return html`
      <div class="kg-doc-grid">
        ${this.documents.map(doc => this.renderDocumentCard(doc))}
      </div>
    `;
  }

  private renderDocumentCard(doc: KnowledgeDocument) {
    const status = STATUS_CONFIG[doc.parseStatus] || { label: doc.parseStatus, cssClass: '' };
    const entityCount = this.entities.filter(e => e.sourceDocumentId === doc.id).length;
    return html`
      <article class="kg-doc-card" @click=${() => this.openDoc(doc)}>
        <div class="kg-doc-card-top">
          <span class="kg-status ${status.cssClass}">
            ${doc.parseStatus === 'parsing' ? html`<span class="kg-spinner"></span>` : nothing}
            ${status.label}
          </span>
          <div class="kg-doc-card-actions">
            ${doc.parseStatus !== 'parsing'
              ? html`
                <button class="kg-icon-btn" title="解析" @click=${(e: Event) => { e.stopPropagation(); this.handleParse(doc); }}>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13">
                    <polygon points="5 3 19 12 5 21 5 3"/>
                  </svg>
                </button>
              `
              : nothing}
            <button class="kg-icon-btn kg-icon-btn--danger" title="删除"
              @click=${(e: Event) => { e.stopPropagation(); this.handleDelete(doc); }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13">
                <polyline points="3 6 5 6 21 6"/>
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
              </svg>
            </button>
          </div>
        </div>
        <h3 class="kg-doc-card-title">${doc.title}</h3>
        <p class="kg-doc-card-excerpt">${doc.content.replace(/^#+\s/gm, '').trim().slice(0, 120) || '暂无内容'}</p>
        <div class="kg-doc-card-footer">
          <div class="kg-doc-card-tags">
            ${doc.tags.length > 0
              ? doc.tags.slice(0, 3).map(t => html`<span class="kg-tag">${t}</span>`)
              : html`<span class="kg-tag kg-tag--empty">无标签</span>`}
            ${doc.tags.length > 3 ? html`<span class="kg-tag kg-tag--more">+${doc.tags.length - 3}</span>` : nothing}
          </div>
          <span class="kg-doc-card-meta">
            ${entityCount > 0 ? html`<span>${entityCount} 实体</span>` : nothing}
            <span>${formatDate(doc.updatedAt)}</span>
          </span>
        </div>
      </article>
    `;
  }

  private renderSkeleton() {
    return html`
      <div class="kg-doc-grid">
        ${Array.from({ length: 6 }).map(() => html`
          <div class="kg-skeleton-card">
            <div class="kg-skeleton-badge"></div>
            <div class="kg-skeleton-line kg-skeleton-line--lg"></div>
            <div class="kg-skeleton-line"></div>
            <div class="kg-skeleton-line kg-skeleton-line--sm"></div>
          </div>
        `)}
      </div>
    `;
  }

  private renderError() {
    return html`
      <div class="kg-empty">
        <p class="kg-empty-text">${this.error}</p>
        <button class="kg-btn kg-btn-secondary" @click=${this.loadDocuments}>重试</button>
      </div>
    `;
  }

  private renderEmpty() {
    return html`
      <div class="kg-empty">
        <div class="kg-empty-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" width="56" height="56">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
            <polyline points="14 2 14 8 20 8"/>
            <line x1="16" y1="13" x2="8" y2="13"/>
            <line x1="16" y1="17" x2="8" y2="17"/>
          </svg>
        </div>
        <h3>开始构建知识图谱</h3>
        <p>编写 Markdown 文档来描述您的物联网场景，AI 将自动抽取空间、设备、关系等结构化知识。</p>
        <div class="kg-empty-actions">
          <button class="kg-btn kg-btn-primary" @click=${this.openNewDoc}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            新建文档
          </button>
          <label class="kg-btn kg-btn-secondary kg-upload-label">
            <input type="file" hidden @change=${this.handleFileUpload}
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

  // ═══════════════════════════════════════════════════
  //  ENTITIES TAB
  // ═══════════════════════════════════════════════════

  private renderEntitiesTab() {
    const filtered = this.entities;
    if (filtered.length === 0) {
      return html`
        <div class="kg-empty">
          <div class="kg-empty-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" width="48" height="48">
              <circle cx="12" cy="12" r="3"/><circle cx="4" cy="6" r="2"/><circle cx="20" cy="6" r="2"/>
              <circle cx="4" cy="18" r="2"/><circle cx="20" cy="18" r="2"/>
              <line x1="6.5" y1="7" x2="10" y2="11"/><line x1="14" y1="11" x2="17.5" y2="7"/>
              <line x1="6.5" y1="17" x2="10" y2="13"/><line x1="14" y1="13" x2="17.5" y2="17"/>
            </svg>
          </div>
          <h3>暂无实体</h3>
          <p>创建并解析文档后，AI 将自动提取实体。</p>
        </div>
      `;
    }
    return html`
      <div class="kg-entity-grid">
        ${filtered.map(e => this.renderEntityCard(e))}
      </div>
    `;
  }

  private renderEntityCard(entity: KnowledgeEntity) {
    const typeCfg = entityTypeConfig(entity.entityType);
    const sourceDoc = this.documents.find(d => d.id === entity.sourceDocumentId);
    return html`
      <div class="kg-entity-card">
        <div class="kg-entity-card-header">
          <span class="kg-entity-type-badge ${entity.entityType}">${typeCfg.icon} ${typeCfg.label}</span>
          ${entity.confidence < 0.7
            ? html`<span class="kg-entity-confidence" title="置信度: ${Math.round(entity.confidence * 100)}%">${Math.round(entity.confidence * 100)}%</span>`
            : nothing}
        </div>
        <h4 class="kg-entity-name">${entity.name}</h4>
        ${entity.description
          ? html`<p class="kg-entity-desc">${entity.description}</p>`
          : nothing}
        <div class="kg-entity-card-footer">
          <div class="kg-entity-tags">
            ${entity.tags.slice(0, 3).map(t => html`<span class="kg-tag">${t}</span>`)}
          </div>
          ${sourceDoc
            ? html`<span class="kg-entity-source" title=${sourceDoc.title}>${sourceDoc.title}</span>`
            : nothing}
        </div>
      </div>
    `;
  }

  // ═══════════════════════════════════════════════════
  //  RELATIONS TAB
  // ═══════════════════════════════════════════════════

  private renderRelationsTab() {
    if (this.relations.length === 0) {
      return html`
        <div class="kg-empty">
          <div class="kg-empty-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" width="48" height="48">
              <line x1="4" y1="12" x2="20" y2="12"/>
              <polyline points="14 6 20 12 14 18"/>
            </svg>
          </div>
          <h3>暂无关系</h3>
          <p>解析文档后，AI 将自动提取实体间的关系。</p>
        </div>
      `;
    }
    return html`
      <div class="kg-relation-list">
        ${this.relations.map(r => this.renderRelationRow(r))}
      </div>
    `;
  }

  private renderRelationRow(rel: KnowledgeRelation) {
    const source = this.getEntityById(rel.sourceEntityId);
    const target = this.getEntityById(rel.targetEntityId);
    const sourceType = source ? entityTypeConfig(source.entityType) : null;
    const targetType = target ? entityTypeConfig(target.entityType) : null;
    return html`
      <div class="kg-relation-row">
        <span class="kg-relation-entity">
          ${sourceType ? html`<span class="kg-relation-entity-icon ${source?.entityType || ''}">${sourceType.icon}</span>` : nothing}
          <span class="kg-relation-entity-name">${source?.name || rel.sourceEntityId.slice(0, 8)}</span>
        </span>
        <span class="kg-relation-arrow">
          <span class="kg-relation-type">${rel.relationType}</span>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="12">
            <line x1="0" y1="6" x2="18" y2="6"/><polyline points="12 2 18 6 12 10"/>
          </svg>
        </span>
        <span class="kg-relation-entity">
          ${targetType ? html`<span class="kg-relation-entity-icon ${target?.entityType || ''}">${targetType.icon}</span>` : nothing}
          <span class="kg-relation-entity-name">${target?.name || rel.targetEntityId.slice(0, 8)}</span>
        </span>
      </div>
    `;
  }

  // ═══════════════════════════════════════════════════
  //  INLINE EDITOR (replaces tab content when active)
  // ═══════════════════════════════════════════════════

  private renderEditor() {
    return html`
      <div class="kg-editor">
        <input
          type="text"
          class="kg-editor-title"
          placeholder="文档标题"
          .value=${this.editorTitle}
          @input=${(e: Event) => { this.editorTitle = (e.target as HTMLInputElement).value; }} />
        <div class="kg-editor-body">
          <div class="kg-editor-main">
            <textarea
              class="kg-editor-textarea"
              placeholder="在此编写 Markdown 知识文档..."
              .value=${this.editorContent}
              @input=${this.onContentChange}
              @drop=${this.onDrop}
              @dragover=${this.onDragOver}
              @dragleave=${this.onDragLeave}></textarea>
            <div class="kg-editor-drop-hint">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="20" height="20">
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                <polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>
              </svg>
              拖拽图片、模型或视频到此处
            </div>
          </div>
          <div class="kg-editor-preview">
            <div class="kg-editor-preview-header">解析预览</div>
            ${this.renderPreviewContent()}
          </div>
        </div>
        <div class="kg-editor-footer">
          <div class="kg-editor-tags">
            ${this.editorTags.map(tag => html`
              <span class="kg-editor-tag">
                ${tag}
                <button @click=${() => this.removeTag(tag)} aria-label="移除">&times;</button>
              </span>
            `)}
            <input type="text" class="kg-editor-tag-input"
              placeholder="输入标签后按回车..." @keydown=${this.onTagInputKeydown} />
          </div>
          <div class="kg-editor-actions">
            <span class="kg-editor-hint">Esc 返回 · Ctrl+Enter 保存</span>
            <button class="kg-btn kg-btn-secondary" @click=${this.closeEditor}>取消</button>
          </div>
        </div>
      </div>
    `;
  }

  private renderPreviewContent() {
    if (this.previewLoading) {
      return html`<div class="kg-preview-empty"><span class="kg-spinner"></span>解析中...</div>`;
    }
    if (!this.previewData) {
      return html`<div class="kg-preview-empty">${this.editingDoc ? '输入内容后将在此处预览解析结果' : '保存文档后可预览解析结果'}</div>`;
    }
    const { entities, relations } = this.previewData;
    return html`
      ${entities.length > 0 ? html`
        <div class="kg-preview-section">
          <div class="kg-preview-section-title">实体 (${entities.length})</div>
          ${entities.map(e => html`
            <div class="kg-preview-entity">
              <span class="kg-preview-entity-type ${e.entityType}">${entityTypeConfig(e.entityType).icon}</span>
              <div>
                <div class="kg-preview-entity-name">${e.name}</div>
                <div class="kg-preview-entity-type-label">${entityTypeConfig(e.entityType).label}</div>
              </div>
            </div>
          `)}
        </div>
      ` : html`<div class="kg-preview-empty">未检测到实体</div>`}
      ${relations.length > 0 ? html`
        <div class="kg-preview-section">
          <div class="kg-preview-section-title">关系 (${relations.length})</div>
          ${relations.map(r => html`
            <div class="kg-preview-relation">
              <span>${r.sourceEntityId.slice(0, 6)}</span>
              <span class="kg-preview-relation-type">${r.relationType}</span>
              <span>${r.targetEntityId.slice(0, 6)}</span>
            </div>
          `)}
        </div>
      ` : nothing}
    `;
  }
}
