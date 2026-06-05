import { LitElement, html, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import {
  knowledgeApi,
  type KnowledgeDocument,
  type KnowledgeEntity,
  type KnowledgeRelation,
  type KnowledgeParseJob,
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

interface Attachment {
  type: 'image' | 'model3d' | 'video' | 'file';
  path: string;
  alt: string;
}

function extractAttachments(content: string): Attachment[] {
  const attachments: Attachment[] = [];
  // Image: ![alt](path)
  const imgRe = /!\[([^\]]*)\]\(([^)]+)\)/g;
  let m: RegExpExecArray | null;
  while ((m = imgRe.exec(content)) !== null) {
    const path = m[2];
    const ext = path.split('.').pop()?.toLowerCase() || '';
    if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp'].includes(ext)) {
      attachments.push({ type: 'image', path, alt: m[1] });
    }
  }
  // GLB/GLTF: ```3d\npath\n```
  const glbRe = /```3d\n([^\n]+)\n```/g;
  while ((m = glbRe.exec(content)) !== null) {
    attachments.push({ type: 'model3d', path: m[1], alt: '' });
  }
  // Video: <video src="path">
  const vidRe = /<video[^>]+src="([^"]+)"/g;
  while ((m = vidRe.exec(content)) !== null) {
    attachments.push({ type: 'video', path: m[1], alt: '' });
  }
  return attachments;
}

@customElement('view-knowledge')
export class KnowledgeView extends LitElement {
  createRenderRoot() { return this; }

  @state() loading = true;
  @state() error = '';
  @state() documents: KnowledgeDocument[] = [];
  @state() searchQuery = '';
  @state() statusFilter = '';
  @state() page = 1;
  @state() pageSize = 24;
  @state() total = 0;

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

  // Parse results & polling
  @state() activePanel: 'preview' | 'results' = 'preview';
  @state() parseResults: { entities: KnowledgeEntity[]; relations: KnowledgeRelation[] } | null = null;
  @state() resultsLoading = false;
  @state() pollingJobId: string | null = null;
  @state() editingEntityId: string | null = null;
  @state() entityEditName = '';
  @state() parseDiff: { added: number; removed: number; modified: number } | null = null;
  @state() confirmingAll = false;

  private _searchTimer: ReturnType<typeof setTimeout> | null = null;
  private _previewTimer: ReturnType<typeof setTimeout> | null = null;
  private _pollTimer: ReturnType<typeof setInterval> | null = null;
  private _pollAttempts = 0;
  private _lastFocused: Element | null = null;

  connectedCallback() {
    super.connectedCallback();
    this.loadDocuments();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    if (this._searchTimer) clearTimeout(this._searchTimer);
    if (this._previewTimer) clearTimeout(this._previewTimer);
    this.stopPolling();
  }

  // ── Data ──

  private async loadDocuments(resetPage = true) {
    this.loading = true;
    this.error = '';
    if (resetPage) this.page = 1;
    try {
      const params: Record<string, string | number> = {};
      if (this.searchQuery.trim()) params.q = this.searchQuery.trim();
      if (this.statusFilter) params.status = this.statusFilter;
      params.page = this.page;
      params.pageSize = this.pageSize;
      const res = await knowledgeApi.listDocuments(params as Record<string, any>);
      this.documents = (res.result as any)?.data ?? [];
      this.total = (res.result as any)?.pagination?.totalCount ?? 0;
    } catch (e: any) {
      this.error = e.message || '加载文档失败';
    } finally {
      this.loading = false;
    }
  }

  private goToPage(p: number) {
    this.page = p;
    this.loadDocuments(false);
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
    this.editorTitle = doc?.name ?? '';
    this.editorContent = doc?.content ?? '';
    this.editorTags = doc?.tags ? [...doc.tags] : [];
    this.previewData = null;
    this.parseResults = null;
    this.parseDiff = null;
    this.confirmingAll = false;
    this.activePanel = 'preview';
    this.showEditor = true;
    this.requestUpdate();
    // Load existing parse results for parsed docs
    if (doc && doc.parseStatus === 'parsed') {
      this.loadParseResults(doc.id);
    }
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
    this.parseResults = null;
    this.activePanel = 'preview';
    this.editingEntityId = null;
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
    // Flush any pending tag from the input
    const tagInput = this.querySelector('.kg-editor-tag-input') as HTMLInputElement | null;
    if (tagInput?.value.trim()) {
      this.addTag(tagInput.value.trim());
      tagInput.value = '';
    }
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
    if (!confirm(`确定要删除文档「${doc.name}」吗？此操作不可撤销。`)) return;
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
      const res = await knowledgeApi.triggerParse(doc.id);
      const jobId = (res.result as any)?.job_id || (res.result as any)?.parseId;
      if (jobId) {
        this.startPolling(jobId, doc.id);
      }
      success('解析任务已启动');
      this.loadDocuments();
    } catch (err: any) {
      toastError(err.message || '启动解析失败');
    }
  }

  // ── Parse Polling ──

  private startPolling(jobId: string, docId: string) {
    this.stopPolling();
    this.pollingJobId = jobId;
    this._pollAttempts = 0;

    // Immediate first poll
    this.pollParseJob(jobId, docId);

    this._pollTimer = setInterval(() => {
      this.pollParseJob(jobId, docId);
    }, 2000);
  }

  private stopPolling() {
    if (this._pollTimer) {
      clearInterval(this._pollTimer);
      this._pollTimer = null;
    }
    this.pollingJobId = null;
    this._pollAttempts = 0;
  }

  private async pollParseJob(jobId: string, docId: string) {
    this._pollAttempts++;
    if (this._pollAttempts > 30) {
      this.stopPolling();
      toastError('解析超时，请稍后重试');
      this.loadDocuments();
      return;
    }

    try {
      const res = await knowledgeApi.getParseJob(jobId);
      const job: KnowledgeParseJob | undefined = res.result as any;

      if (!job) return;

      if (job.status === 'completed') {
        this.stopPolling();
        const summary = job.resultSummary;
        const diff = summary?.diff;
        const msg = diff
          ? `解析完成：${summary!.entityCount} 个实体, ${summary!.relationCount} 个关系 (新增 ${diff.added} / 修改 ${diff.modified} / 删除 ${diff.removed})`
          : `解析完成：${summary?.entityCount ?? 0} 个实体, ${summary?.relationCount ?? 0} 个关系`;
        success(msg);
        if (diff) this.parseDiff = diff;
        this.loadDocuments();
        if (this.showEditor && this.editingDoc?.id === docId) {
          this.loadParseResults(docId);
        }
      } else if (job.status === 'failed') {
        this.stopPolling();
        toastError(job.errorMessage || '解析失败');
        this.loadDocuments();
      }
      // "pending" or "running" — keep polling
    } catch {
      // Network error — keep polling until max attempts
    }
  }

  // ── Parse Results ──

  private async loadParseResults(docId: string) {
    this.resultsLoading = true;
    try {
      const [entitiesRes, relationsRes] = await Promise.all([
        knowledgeApi.listEntities({ documentId: docId }),
        knowledgeApi.listRelations(),
      ]);
      const entities: KnowledgeEntity[] = (entitiesRes.result as any) || [];
      const allRelations: KnowledgeRelation[] = (relationsRes.result as any) || [];

      // Filter relations to only those involving the document's entities
      const entityIds = new Set(entities.map((e: KnowledgeEntity) => e.id));
      const relations = allRelations.filter(
        (r: KnowledgeRelation) => entityIds.has(r.sourceEntityId) || entityIds.has(r.targetEntityId)
      );

      // Sort entities by confidence descending
      entities.sort((a: KnowledgeEntity, b: KnowledgeEntity) => b.confidence - a.confidence);
      this.parseResults = { entities, relations };
      if (entities.length > 0 || relations.length > 0) {
        this.activePanel = 'results';
      }
    } catch {
      this.parseResults = null;
    } finally {
      this.resultsLoading = false;
    }
  }

  // ── Entity Operations ──

  private async handleConfirmEntity(entity: KnowledgeEntity) {
    try {
      await knowledgeApi.updateEntity(entity.id, { tags: [...entity.tags, 'confirmed'] });
      success('实体已确认');
      if (this.editingDoc) this.loadParseResults(this.editingDoc.id);
    } catch (err: any) {
      toastError(err.message || '确认失败');
    }
  }

  private startEditEntity(entity: KnowledgeEntity) {
    this.editingEntityId = entity.id;
    this.entityEditName = entity.name;
  }

  private cancelEditEntity() {
    this.editingEntityId = null;
    this.entityEditName = '';
  }

  private async saveEntityEdit(entity: KnowledgeEntity) {
    if (!this.entityEditName.trim()) return;
    try {
      await knowledgeApi.updateEntity(entity.id, { name: this.entityEditName.trim() });
      this.cancelEditEntity();
      if (this.editingDoc) this.loadParseResults(this.editingDoc.id);
    } catch (err: any) {
      toastError(err.message || '保存失败');
    }
  }

  private async handleDeleteEntity(entity: KnowledgeEntity) {
    if (!confirm(`确定要删除实体「${entity.name}」吗？`)) return;
    try {
      // Delete by clearing — the entity will be removed on next parse
      await knowledgeApi.updateEntity(entity.id, { name: `__deleted__${entity.name}` });
      success('实体已标记删除');
      if (this.editingDoc) this.loadParseResults(this.editingDoc.id);
    } catch (err: any) {
      toastError(err.message || '删除失败');
    }
  }

  // ── Batch Operations ──

  private async handleConfirmAll() {
    if (!this.parseResults?.entities.length) return;
    const unconfirmed = this.parseResults.entities.filter(
      e => !e.name.startsWith('__deleted__') && !e.tags.includes('confirmed')
    );
    if (unconfirmed.length === 0) { success('所有实体已确认'); return; }

    this.confirmingAll = true;
    let confirmed = 0;
    let failed = 0;
    for (const entity of unconfirmed) {
      try {
        await knowledgeApi.updateEntity(entity.id, { tags: [...entity.tags, 'confirmed'] });
        confirmed++;
      } catch { failed++; }
    }
    this.confirmingAll = false;

    if (failed > 0) {
      toastError(`已确认 ${confirmed} 个, ${failed} 个失败`);
    } else {
      success(`已全部确认 ${confirmed} 个实体`);
    }
    if (this.editingDoc) this.loadParseResults(this.editingDoc.id);
  }

  private async handleReparse() {
    if (!this.editingDoc) return;
    try {
      const res = await knowledgeApi.triggerParse(this.editingDoc.id);
      const jobId = (res.result as any)?.job_id || (res.result as any)?.parseId;
      if (jobId) {
        this.parseDiff = null;
        this.startPolling(jobId, this.editingDoc.id);
        success('重新解析已启动');
      }
    } catch (err: any) {
      toastError(err.message || '启动解析失败');
    }
  }

  private getDiffStatus(entity: KnowledgeEntity): 'added' | 'modified' | 'removed' | 'unchanged' {
    if (entity.name.startsWith('__deleted__')) return 'removed';
    if (!this.parseDiff) return 'unchanged';
    if (!entity.tags.includes('confirmed')) return 'added';
    return 'modified';
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
      } else {
        // Editor not open: auto-open with file pre-populated
        const nameWithoutExt = file.name.replace(/\.[^.]+$/, '');
        this.editorTitle = nameWithoutExt;
        this.editorContent = `# ${nameWithoutExt}\n\n${md}`;
        this.editorTags = [];
        this.editingDoc = null;
        this.previewData = null;
        this.showEditor = true;
        this.requestUpdate();
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
        ${this.renderToolbar()}
        ${this.loading
          ? this.renderSkeleton()
          : this.error
            ? this.renderError()
            : this.documents.length === 0
              ? this.renderEmpty()
              : this.renderGrid()}
        ${!this.loading && !this.error && this.documents.length > 0 ? this.renderPagination() : nothing}
        ${this.showEditor ? this.renderEditorModal() : nothing}
      </div>
    `;
  }

  // ── Toolbar ──

  private renderToolbar() {
    return html`
      <div class="knowledge-toolbar">
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
    const status = STATUS_CONFIG[doc.parseStatus ?? ''] || { label: doc.parseStatus ?? '未知', cssClass: '' };
    const attachments = extractAttachments(doc.content ?? '');
    const hasImage = attachments.some(a => a.type === 'image');
    const hasGLB = attachments.some(a => a.type === 'model3d');

    return html`
      <div class="knowledge-card card ${doc.parseStatus === 'parsing' || this.pollingJobId ? 'knowledge-card--polling' : ''}" @click=${() => this.openEditor(doc)}>
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
          ${hasImage
            ? html`<div class="kg-card-thumb">
                <img src=${attachments.find(a => a.type === 'image')!.path} alt="preview" loading="lazy" />
                ${hasGLB
                  ? html`<span class="kg-card-badge-3d" title="含 3D 模型">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="11" height="11">
                        <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>
                      </svg>
                      3D
                    </span>`
                  : nothing}
              </div>`
            : hasGLB
              ? html`<div class="kg-card-thumb kg-card-thumb--placeholder">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="32" height="32">
                    <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>
                  </svg>
                  <span class="kg-card-badge-3d kg-card-badge-3d--overlay">3D 模型</span>
                </div>`
              : nothing}
          <h3 class="knowledge-card-title">${doc.name}</h3>
          <p class="knowledge-card-desc">${(doc.content ?? '').replace(/^#+\s/gm, '').replace(/!\[.*?\]\(.*?\)/g, '').replace(/```3d\n.*?\n```/g, '').trim().slice(0, 120) || '暂无内容'}</p>
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

  // ── Pagination ──

  private get totalPages() { return Math.max(1, Math.ceil(this.total / this.pageSize)); }

  private renderPagination() {
    const totalPages = this.totalPages;
    if (totalPages <= 1) return nothing;

    return html`
      <div class="knowledge-pagination">
        <span class="knowledge-pagination-info">共 ${this.total} 条</span>
        <button class="btn btn-secondary"
          ?disabled=${this.page <= 1}
          @click=${() => this.goToPage(this.page - 1)}>上一页</button>
        ${Array.from({ length: totalPages }, (_, i) => i + 1).map(p => html`
          <button class="btn btn-secondary ${p === this.page ? 'knowledge-pagination-btn--active' : ''}"
            @click=${() => this.goToPage(p)}>${p}</button>
        `)}
        <button class="btn btn-secondary"
          ?disabled=${this.page >= totalPages}
          @click=${() => this.goToPage(this.page + 1)}>下一页</button>
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
          <div class="modal-header" style="padding-bottom:0">
            <button class="modal-close" @click=${this.closeEditor} style="margin-left:auto">&times;</button>
          </div>
          <div class="modal-body knowledge-editor-body">
            <input type="text" class="kg-editor-title"
              placeholder="文档标题" .value=${this.editorTitle}
              @input=${(e: Event) => { this.editorTitle = (e.target as HTMLInputElement).value; }} />
            <div class="kg-editor-workspace">
              <div class="kg-editor-main">
                <textarea class="kg-editor-textarea"
                  placeholder="在此编写 Markdown 知识文档..."
                  .value=${this.editorContent}
                  @input=${this.onContentChange}
                  @drop=${this.onEditorDrop}
                  @dragover=${this.onEditorDragOver}></textarea>
                <div class="kg-editor-drop-hint">拖拽文件到此处上传</div>
              </div>
              ${this.editingDoc ? this.renderSidePanel() : nothing}
            </div>
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

  // ── Side Panel (tabs: Preview | Results) ──

  private renderSidePanel() {
    const hasResults = this.parseResults && (this.parseResults.entities.length > 0 || this.parseResults.relations.length > 0);
    const isParsing = this.editingDoc?.parseStatus === 'parsing' || this.pollingJobId !== null;

    return html`
      <div class="kg-side-panel">
        <div class="kg-panel-tabs">
          <button class="kg-panel-tab ${this.activePanel === 'preview' ? 'active' : ''}"
            @click=${() => { this.activePanel = 'preview'; }}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13">
              <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
              <circle cx="12" cy="12" r="3"/>
            </svg>
            预览
          </button>
          <button class="kg-panel-tab ${this.activePanel === 'results' ? 'active' : ''}"
            @click=${() => { this.activePanel = 'results'; }}
            ?disabled=${!hasResults && !isParsing}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13">
              <polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/>
            </svg>
            解析结果
            ${hasResults ? html`<span class="kg-panel-tab-badge">${this.parseResults!.entities.length}</span>` : nothing}
          </button>
        </div>
        <div class="kg-panel-content">
          ${this.activePanel === 'results'
            ? this.renderResultsPanel()
            : this.renderPreviewPanel()}
        </div>
      </div>
    `;
  }

  private renderResultsPanel() {
    if (this.resultsLoading) {
      return html`<div class="kg-results-loading">
        <span class="kg-spinner-lg"></span>
        <span>加载解析结果...</span>
      </div>`;
    }

    if (!this.parseResults || (this.parseResults.entities.length === 0 && this.parseResults.relations.length === 0)) {
      return html`<div class="kg-results-empty">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="32" height="32">
          <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z"/>
        </svg>
        <p>暂无解析结果</p>
        <span>点击「解析」按钮由 AI 自动提取</span>
      </div>`;
    }

    const { entities, relations } = this.parseResults;
    const hasUnconfirmed = entities.some(e => !e.name.startsWith('__deleted__') && !e.tags.includes('confirmed'));

    return html`
      <div class="kg-results-scroll">
        ${this.parseDiff ? html`
          <div class="kg-diff-summary">
            <span class="kg-diff-summary-title">变更摘要</span>
            <span class="kg-diff-badge kg-diff-badge--added">+${this.parseDiff.added}</span>
            <span class="kg-diff-badge kg-diff-badge--modified">~${this.parseDiff.modified}</span>
            <span class="kg-diff-badge kg-diff-badge--removed">-${this.parseDiff.removed}</span>
          </div>
        ` : nothing}
        <div class="kg-results-actions">
          ${hasUnconfirmed ? html`
            <button class="kg-batch-btn kg-batch-btn--confirm"
              ?disabled=${this.confirmingAll}
              @click=${this.handleConfirmAll}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="12" height="12">
                <polyline points="20 6 9 17 4 12"/>
              </svg>
              ${this.confirmingAll ? '确认中...' : '全部确认'}
            </button>
          ` : nothing}
          <button class="kg-batch-btn kg-batch-btn--reparse"
            @click=${this.handleReparse}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="12" height="12">
              <polyline points="23 4 23 10 17 10"/>
              <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
            </svg>
            重新解析
          </button>
        </div>
        ${entities.length > 0 ? html`
          <div class="kg-results-section">
            <div class="kg-results-section-header">
              <span class="kg-results-section-title">实体</span>
              <span class="kg-results-section-count">${entities.length}</span>
            </div>
            ${entities.map((e: KnowledgeEntity) => this.renderEntityCard(e))}
          </div>
        ` : nothing}
        ${relations.length > 0 ? html`
          <div class="kg-results-section">
            <div class="kg-results-section-header">
              <span class="kg-results-section-title">关系</span>
              <span class="kg-results-section-count">${relations.length}</span>
            </div>
            ${relations.map((r: KnowledgeRelation) => this.renderRelationRow(r, entities))}
          </div>
        ` : nothing}
      </div>
    `;
  }

  private renderEntityCard(entity: KnowledgeEntity) {
    const isEditing = this.editingEntityId === entity.id;
    const diffStatus = this.getDiffStatus(entity);
    const typeConfig: Record<string, { icon: string; label: string }> = {
      space: { icon: '◻', label: '空间' },
      device: { icon: '⬡', label: '设备' },
      functional: { icon: '⚙', label: '功能' },
    };
    const tc = typeConfig[entity.entityType] || { icon: '●', label: entity.entityType };

    const diffClass = diffStatus !== 'unchanged' ? `kg-entity-card--diff-${diffStatus}` : '';

    return html`
      <div class="kg-entity-card ${entity.confidence < 0.5 ? 'kg-entity-card--low' : ''} ${diffClass}">
        <div class="kg-entity-card-header">
          <span class="kg-entity-type-badge ${entity.entityType}">${tc.icon} ${tc.label}</span>
          <div class="kg-entity-confidences">
            <span class="kg-entity-confidence" style="--conf:${entity.confidence}">
              ${Math.round(entity.confidence * 100)}%
            </span>
          </div>
        </div>
        ${isEditing ? html`
          <input class="kg-entity-edit-input" .value=${this.entityEditName}
            @input=${(e: Event) => { this.entityEditName = (e.target as HTMLInputElement).value; }}
            @keydown=${(e: KeyboardEvent) => {
              if (e.key === 'Enter') this.saveEntityEdit(entity);
              if (e.key === 'Escape') this.cancelEditEntity();
            }} />
          <div class="kg-entity-edit-actions">
            <button class="kg-entity-action kg-entity-action--save" @click=${() => this.saveEntityEdit(entity)}>保存</button>
            <button class="kg-entity-action" @click=${() => this.cancelEditEntity()}>取消</button>
          </div>
        ` : html`
          <div class="kg-entity-card-body">
            <span class="kg-entity-name">${entity.name}</span>
            ${entity.description ? html`<span class="kg-entity-desc">${entity.description}</span>` : nothing}
          </div>
          ${entity.tags.length > 0 ? html`
            <div class="kg-entity-tags">
              ${entity.tags.map((t: string) => html`<span class="kg-entity-tag">${t}</span>`)}
            </div>
          ` : nothing}
          <div class="kg-entity-actions">
            <button class="kg-entity-action kg-entity-action--confirm" title="确认"
              @click=${() => this.handleConfirmEntity(entity)}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13">
                <polyline points="20 6 9 17 4 12"/>
              </svg>
            </button>
            <button class="kg-entity-action" title="编辑"
              @click=${() => this.startEditEntity(entity)}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13">
                <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
              </svg>
            </button>
            <button class="kg-entity-action kg-entity-action--delete" title="删除"
              @click=${() => this.handleDeleteEntity(entity)}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13">
                <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>
        `}
        <div class="kg-confidence-bar">
          <div class="kg-confidence-fill" style="width:${entity.confidence * 100}%"></div>
        </div>
      </div>
    `;
  }

  private renderRelationRow(relation: KnowledgeRelation, entities: KnowledgeEntity[]) {
    const sourceName = entities.find((e: KnowledgeEntity) => e.id === relation.sourceEntityId)?.name || relation.sourceEntityId.slice(0, 8);
    const targetName = entities.find((e: KnowledgeEntity) => e.id === relation.targetEntityId)?.name || relation.targetEntityId.slice(0, 8);
    return html`
      <div class="kg-relation-row">
        <span class="kg-relation-entity">${sourceName}</span>
        <span class="kg-relation-type">${relation.relationType}</span>
        <span class="kg-relation-entity">${targetName}</span>
        <span class="kg-relation-confidence">${Math.round(relation.confidence * 100)}%</span>
      </div>
    `;
  }

  private renderPreviewPanel() {
    const attachments = extractAttachments(this.editorContent);
    const hasFiles = attachments.length > 0;

    if (this.previewLoading) {
      return html`<div class="kg-preview-panel"><div class="kg-preview-empty">解析中...</div></div>`;
    }
    if (!this.previewData && !hasFiles) {
      return html`<div class="kg-preview-panel"><div class="kg-preview-empty">编辑后将在此预览解析结果</div></div>`;
    }

    const { entities, relations } = this.previewData || { entities: [], relations: [] };

    return html`
      <div class="kg-preview-panel">
        ${hasFiles ? html`
          <div class="kg-preview-section">
            <div class="kg-preview-label">附件 (${attachments.length})</div>
            <div class="kg-preview-files">
              ${attachments.map(a => a.type === 'image'
                ? html`<a class="kg-preview-file" href=${a.path} target="_blank" title=${a.alt}>
                    <img src=${a.path} alt=${a.alt} loading="lazy" />
                    <span class="kg-preview-file-label">图片</span>
                  </a>`
                : a.type === 'model3d'
                  ? html`<div class="kg-preview-file kg-preview-file--3d">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="28" height="28">
                        <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>
                      </svg>
                      <span class="kg-preview-file-label">3D 模型</span>
                      <code class="kg-preview-file-path">${a.path.split('/').pop()}</code>
                    </div>`
                : nothing
              )}
            </div>
          </div>
        ` : nothing}
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
