import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { SignalWatcher } from "@lit-labs/signals";
import { thingApi, type Thing } from "../../api/things.js";
import { success, error as toastError } from "../components/toast.js";

type ViewMode = "list" | "tree";

const UPGRADE_NOTICE_KEY = "thing-ontology-upgrade-notice-dismissed";

interface TreeNode {
  thing: Thing;
  children: TreeNode[];
  depth: number;
}

@customElement("view-things")
export class ThingsView extends SignalWatcher(LitElement) {
  @state() loading = true;
  @state() error = "";
  @state() things: Thing[] = [];
  @state() total = 0;
  @state() unassignedResourceCount = 0;

  // View mode
  @state() viewMode: ViewMode = "list";

  // Filters
  @state() searchName = "";
  @state() filterType = "";

  // Tree state
  @state() expandedIds = new Set<string>();

  // Upgrade notice
  @state() showUpgradeNotice = false;

  // Create modal
  @state() showCreateModal = false;
  @state() createSaving = false;
  @state() createName = "";
  @state() createThingType = "";
  @state() createParentId = "";
  @state() createTemplateId = "";

  // Drag state
  @state() dragOverId: string | null = null;
  @state() cycleErrorId: string | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.showUpgradeNotice = !localStorage.getItem(UPGRADE_NOTICE_KEY);
    this.loadThings();
  }

  // === Data Loading ===

  async loadThings() {
    this.loading = true;
    this.error = "";
    try {
      const params: Record<string, string> = {};
      if (this.searchName) params.name = this.searchName;
      if (this.filterType) params.thingType = this.filterType;

      // Fetch all things (no pagination for tree view)
      if (this.viewMode === "tree") {
        params.limit = "1000";
      }

      const res = await thingApi.list(params);
      const data = res.result;
      if (data) {
        this.things = data.items || [];
        this.total = data.total || this.things.length;
        this.unassignedResourceCount = data.unassignedResourceCount ?? 0;
      }
    } catch (err: any) {
      this.error = err.message || "加载物列表失败";
    } finally {
      this.loading = false;
    }
  }

  dismissUpgradeNotice() {
    localStorage.setItem(UPGRADE_NOTICE_KEY, "1");
    this.showUpgradeNotice = false;
  }

  switchView(mode: ViewMode) {
    this.viewMode = mode;
    if (mode === "tree") {
      // Re-fetch with higher limit for tree
      this.loadThings();
    }
  }

  // === Navigation ===

  navigateToThing(id: string) {
    window.history.pushState({}, "", `/things/${id}`);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }

  // === Tree helpers ===

  buildTree(): TreeNode[] {
    const idMap = new Map<string, TreeNode>();
    const roots: TreeNode[] = [];

    // First pass: create nodes
    for (const thing of this.things) {
      idMap.set(thing.id, { thing, children: [], depth: 0 });
    }

    // Second pass: build hierarchy
    for (const thing of this.things) {
      const node = idMap.get(thing.id)!;
      if (thing.parentId && idMap.has(thing.parentId)) {
        const parent = idMap.get(thing.parentId)!;
        parent.children.push(node);
      } else {
        roots.push(node);
      }
    }

    // Compute depths
    const assignDepth = (nodes: TreeNode[], depth: number) => {
      for (const node of nodes) {
        node.depth = depth;
        assignDepth(node.children, depth + 1);
      }
    };
    assignDepth(roots, 0);

    return roots;
  }

  toggleExpand(id: string) {
    const next = new Set(this.expandedIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    this.expandedIds = next;
  }

  // Auto-expand first 2 levels
  autoExpandLevels(nodes: TreeNode[], maxLevel: number) {
    for (const node of nodes) {
      if (node.depth < maxLevel) {
        this.expandedIds.add(node.thing.id);
      }
      this.autoExpandLevels(node.children, maxLevel);
    }
  }

  get isFiltered(): boolean {
    return !!(this.searchName || this.filterType);
  }

  // === Drag & Drop (Tree) ===

  onDragStart(e: DragEvent, id: string) {
    if (!e.dataTransfer) return;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", id);
    e.stopPropagation();
  }

  onDragOver(e: DragEvent, id: string) {
    e.preventDefault();
    if (!e.dataTransfer) return;
    e.dataTransfer.dropEffect = "move";
    this.dragOverId = id;
  }

  onDragLeave(_e: DragEvent, _id: string) {
    this.dragOverId = null;
  }

  async onDrop(e: DragEvent, newParentId: string) {
    e.preventDefault();
    this.dragOverId = null;
    this.cycleErrorId = null;
    const draggedId = e.dataTransfer?.getData("text/plain");
    if (!draggedId || draggedId === newParentId) return;

    try {
      await thingApi.update(draggedId, { parentId: newParentId });
      success("物已移动");
      await this.loadThings();
    } catch (err: any) {
      if (err.code === 409 || (err.message && err.message.includes("CYCLE"))) {
        this.cycleErrorId = newParentId;
        toastError("不能移动到该节点——这会造成循环引用");
      } else {
        toastError(err.message || "移动失败");
      }
    }
  }

  // === Create ===

  openCreateModal() {
    this.createName = "";
    this.createThingType = "";
    this.createParentId = "";
    this.createTemplateId = "";
    this.showCreateModal = true;
  }

  closeCreateModal() {
    this.showCreateModal = false;
  }

  async submitCreate() {
    if (!this.createName.trim()) return;
    this.createSaving = true;
    try {
      const payload: Record<string, unknown> = {
        name: this.createName.trim(),
        thingType: this.createThingType || undefined,
        parentId: this.createParentId || undefined,
        templateId: this.createTemplateId || undefined,
      };
      const res = await thingApi.create(payload);
      const newThing = res.result;
      success("物已创建");
      this.closeCreateModal();
      if (newThing?.id) {
        this.navigateToThing(newThing.id);
      } else {
        await this.loadThings();
      }
    } catch (err: any) {
      toastError(err.message || "创建失败");
    } finally {
      this.createSaving = false;
    }
  }

  // === Helpers ===

  statusLabel(state?: string): string {
    switch (state) {
      case "online": return "在线";
      case "offline": return "离线";
      case "error": return "故障";
      case "maintenance": return "维护";
      default: return state || "未知";
    }
  }

  statusColor(state?: string): string {
    switch (state) {
      case "online": return "var(--success)";
      case "offline": return "var(--muted)";
      case "error": return "var(--danger)";
      case "maintenance": return "var(--warning)";
      default: return "var(--muted)";
    }
  }

  thingTypeLabel(t?: string): string {
    return t || "-";
  }

  hasKnowledge(thing: Thing): boolean {
    return thing.summaryStatus === "completed" || !!thing.ontologySummary;
  }

  formatTime(iso?: string): string {
    if (!iso) return "-";
    return iso.slice(0, 16).replace("T", " ");
  }

  // === Render ===

  render() {
    if (this.loading) {
      return this.renderSkeleton();
    }

    if (this.error && this.things.length === 0) {
      return this.renderError();
    }

    return html`
      <div class="things-view">
        ${this.showUpgradeNotice ? this.renderUpgradeNotice() : nothing}
        ${this.renderToolbar()}
        <div class="things-view__content">
          ${this.viewMode === "list" ? this.renderListView() : this.renderTreeView()}
        </div>
        ${this.showCreateModal ? this.renderCreateModal() : nothing}
      </div>
    `;
  }

  renderUpgradeNotice() {
    return html`
      <div class="upgrade-notice" role="status">
        <span class="upgrade-notice__icon">&#9432;</span>
        <span class="upgrade-notice__text">设备已升级为物，全部数据已迁移</span>
        <button
          class="upgrade-notice__dismiss"
          @click=${this.dismissUpgradeNotice}
          aria-label="关闭提示"
        >&times;</button>
      </div>
    `;
  }

  renderToolbar() {
    return html`
      <div class="toolbar">
        <div class="field filter-bar__search">
          <input
            type="text"
            placeholder="搜索物名称..."
            .value=${this.searchName}
            @input=${(e: Event) => { this.searchName = (e.target as HTMLInputElement).value; }}
            @keydown=${(e: KeyboardEvent) => { if (e.key === "Enter") this.loadThings(); }}
          />
        </div>
        <select class="select filter-bar__select" .value=${this.filterType} @change=${(e: Event) => { this.filterType = (e.target as HTMLSelectElement).value; this.loadThings(); }}>
          <option value="">全部类型</option>
          <option value="device">设备</option>
          <option value="space">空间</option>
          <option value="group">分组</option>
        </select>
        <div class="toolbar__spacer"></div>
        <div class="view-toggle">
          <button
            class="btn btn--ghost btn--sm view-toggle__btn ${this.viewMode === "list" ? "view-toggle__btn--active" : ""}"
            @click=${() => this.switchView("list")}
            title="列表视图"
            aria-label="列表视图"
            aria-pressed=${this.viewMode === "list"}
          >&#9776; 列表</button>
          <button
            class="btn btn--ghost btn--sm view-toggle__btn ${this.viewMode === "tree" ? "view-toggle__btn--active" : ""}"
            @click=${() => {
              this.switchView("tree");
              // Auto-expand first 2 levels after tree is loaded
              requestAnimationFrame(() => {
                const tree = this.buildTree();
                this.autoExpandLevels(tree, 2);
              });
            }}
            title="树形视图"
            aria-label="树形视图"
            aria-pressed=${this.viewMode === "tree"}
          >&#9776; 树</button>
        </div>
        <button class="btn btn--primary" @click=${this.openCreateModal}>创建物</button>
      </div>
    `;
  }

  // === List View (D3) ===

  renderListView() {
    if (this.things.length === 0) {
      return this.isFiltered ? this.renderFilterNoResults() : this.renderEmpty();
    }

    return html`
      <div class="card table-container">
        <table class="data-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>类型</th>
              <th>知识</th>
              <th>状态</th>
              <th>更新时间</th>
            </tr>
          </thead>
          <tbody>
            ${this.things.map(t => html`
              <tr
                class="thing-row"
                tabindex="0"
                role="link"
                @click=${() => this.navigateToThing(t.id)}
                @keydown=${(e: KeyboardEvent) => { if (e.key === "Enter") this.navigateToThing(t.id); }}
              >
                <td>
                  <div class="data-table__primary">${t.name}</div>
                  ${t.breadcrumb?.length ? html`
                    <div class="data-table__secondary">${t.breadcrumb.map(b => b.name).join(" / ")}</div>
                  ` : nothing}
                </td>
                <td class="data-table__cell-sm">${this.thingTypeLabel(t.thingType || t.deviceType)}</td>
                <td class="data-table__cell-sm">${this.renderKnowledgeBadge(t)}</td>
                <td>
                  <span class="status-badge">
                    <span class="status-dot" style="background: ${this.statusColor(t.state)};" role="img" aria-label="${this.statusLabel(t.state)}"></span>
                    <span class="status-badge__label">${this.statusLabel(t.state)}</span>
                  </span>
                </td>
                <td class="data-table__cell-sm">${this.formatTime(t.updatedAt)}</td>
              </tr>
            `)}
          </tbody>
        </table>
      </div>
    `;
  }

  // === Knowledge Badge (D6) ===

  renderKnowledgeBadge(thing: Thing) {
    const hasDoc = this.hasKnowledge(thing);
    return html`
      <span class="knowledge-badge">
        <span
          class="knowledge-dot ${hasDoc ? "knowledge-dot--has" : "knowledge-dot--none"}"
          role="img"
          aria-label=${hasDoc ? "已挂载" : "未挂载"}
        ></span>
        <span class="knowledge-badge__text">${hasDoc ? "已挂载文档" : "未挂载"}</span>
      </span>
    `;
  }

  // === Tree View (D3, D12) ===

  renderTreeView() {
    const tree = this.buildTree();

    if (tree.length === 0) {
      return this.isFiltered ? this.renderFilterNoResults() : this.renderEmpty();
    }

    return html`
      <div class="card tree-container">
        <div class="tree-header">
          <span class="tree-header__count">共 ${this.total} 个物</span>
          ${this.unassignedResourceCount > 0 ? html`
            <span class="tree-header__unassigned" style="margin-left: var(--space-2); font-size: 12px; color: var(--muted);">
              ${this.unassignedResourceCount} 个资源未指派
            </span>
          ` : nothing}
        </div>
        <div class="tree-list" role="tree">
          ${tree.map(node => this.renderTreeNode(node))}
        </div>
      </div>
    `;
  }

  renderTreeNode(node: TreeNode): unknown {
    const hasChildren = node.children.length > 0;
    const isExpanded = this.expandedIds.has(node.thing.id);
    const isDragOver = this.dragOverId === node.thing.id;
    const isCycleError = this.cycleErrorId === node.thing.id;
    const indent = node.depth * 24;

    return html`
      <div class="tree-node-wrapper" role="none">
        <div
          class="tree-node ${isDragOver ? "tree-node--drag-over" : ""} ${isCycleError ? "tree-node--cycle-error" : ""}"
          role="treeitem"
          aria-expanded=${hasChildren ? isExpanded : nothing}
          aria-selected="false"
          draggable="true"
          @dragstart=${(e: DragEvent) => this.onDragStart(e, node.thing.id)}
          @dragover=${(e: DragEvent) => this.onDragOver(e, node.thing.id)}
          @dragleave=${(e: DragEvent) => this.onDragLeave(e, node.thing.id)}
          @drop=${(e: DragEvent) => this.onDrop(e, node.thing.id)}
          style="padding-left: ${indent + 12}px;"
        >
          <!-- Expand/collapse arrow -->
          <span class="tree-node__toggle" @click=${(e: Event) => { e.stopPropagation(); this.toggleExpand(node.thing.id); }}>
            ${hasChildren
              ? html`<span class="tree-node__arrow ${isExpanded ? "tree-node__arrow--expanded" : ""}" aria-hidden="true">&#9654;</span>`
              : html`<span class="tree-node__arrow tree-node__arrow--hidden" aria-hidden="true"></span>`
            }
          </span>

          <!-- Node content (click to navigate) -->
          <span
            class="tree-node__content"
            @click=${() => this.navigateToThing(node.thing.id)}
            @keydown=${(e: KeyboardEvent) => { if (e.key === "Enter") this.navigateToThing(node.thing.id); }}
            tabindex="0"
            role="link"
          >
            <span class="tree-node__name">${node.thing.name}</span>
            <span class="tree-node__type">${this.thingTypeLabel(node.thing.thingType || node.thing.deviceType)}</span>
            ${this.renderKnowledgeBadge(node.thing)}
            <span class="status-badge">
              <span class="status-dot" style="background: ${this.statusColor(node.thing.state)};" role="img" aria-label="${this.statusLabel(node.thing.state)}"></span>
              <span class="status-badge__label">${this.statusLabel(node.thing.state)}</span>
            </span>
          </span>
        </div>
        ${hasChildren && isExpanded ? html`
          <div class="tree-children" role="group">
            ${node.children.map((child): ReturnType<typeof this.renderTreeNode> => this.renderTreeNode(child))}
          </div>
        ` : nothing}
      </div>
    `;
  }

  // === Skeleton / Loading (D7) ===

  renderSkeleton() {
    return html`
      <div class="things-view">
        ${this.showUpgradeNotice ? this.renderUpgradeNotice() : nothing}
        ${this.renderToolbar()}
        <div class="card table-container">
          <table class="data-table">
            <thead>
              <tr>
                <th>名称</th>
                <th>类型</th>
                <th>知识</th>
                <th>状态</th>
                <th>更新时间</th>
              </tr>
            </thead>
            <tbody>
              ${Array.from({ length: 5 }).map(() => html`
                <tr class="skeleton-row">
                  <td><div class="skeleton-line skeleton-line--lg"></div></td>
                  <td><div class="skeleton-line skeleton-line--sm"></div></td>
                  <td><div class="skeleton-line skeleton-line--sm"></div></td>
                  <td><div class="skeleton-line skeleton-line--sm"></div></td>
                  <td><div class="skeleton-line skeleton-line--md"></div></td>
                </tr>
              `)}
            </tbody>
          </table>
        </div>
      </div>
    `;
  }

  // === Empty State (D7) ===

  renderEmpty() {
    return html`
      <div class="card empty-center" style="padding: var(--space-8) var(--space-4);">
        <div style="font-size: 48px; margin-bottom: var(--space-3); opacity: 0.3;">&#128736;</div>
        <div style="font-size: 16px; font-weight: 600; margin-bottom: var(--space-2);">还没有物</div>
        <div style="font-size: 13px; color: var(--muted); margin-bottom: var(--space-4);">
          创建第一个物来开始管理您的 IoT 设备
        </div>
        <button class="btn btn--primary" @click=${this.openCreateModal}>创建第一个物</button>
      </div>
    `;
  }

  // === Error State (D7) ===

  renderError() {
    return html`
      <div class="things-view">
        ${this.showUpgradeNotice ? this.renderUpgradeNotice() : nothing}
        ${this.renderToolbar()}
        <div class="card" style="padding: var(--space-8) var(--space-4); text-align: center;">
          <div style="font-size: 14px; color: var(--danger); margin-bottom: var(--space-3);">${this.error}</div>
          <button class="btn btn--primary" @click=${this.loadThings}>重试</button>
        </div>
      </div>
    `;
  }

  // === Filter No Results (D7) ===

  renderFilterNoResults() {
    return html`
      <div class="card empty-center" style="padding: var(--space-8) var(--space-4);">
        <div style="font-size: 48px; margin-bottom: var(--space-3); opacity: 0.3;">&#128269;</div>
        <div style="font-size: 14px; color: var(--muted); margin-bottom: var(--space-3);">无匹配的物</div>
        <button
          class="btn btn--ghost"
          @click=${() => {
            this.searchName = "";
            this.filterType = "";
            this.loadThings();
          }}
        >清除过滤</button>
      </div>
    `;
  }

  // === Create Modal (D8) ===

  renderCreateModal() {
    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label="创建物" @click=${this.closeCreateModal} @keydown=${(e: KeyboardEvent) => { if (e.key === "Escape") this.closeCreateModal(); }}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <span>创建物</span>
            <button class="btn btn--icon" aria-label="关闭" @click=${this.closeCreateModal}>&times;</button>
          </div>
          <div class="modal-body">
            <div class="field">
              <label class="label">名称 <span style="color: var(--danger);">*</span></label>
              <input
                type="text"
                class="input"
                placeholder="输入物名称"
                .value=${this.createName}
                @input=${(e: Event) => { this.createName = (e.target as HTMLInputElement).value; }}
              />
            </div>
            <div class="field">
              <label class="label">类型</label>
              <select class="select" .value=${this.createThingType} @change=${(e: Event) => { this.createThingType = (e.target as HTMLSelectElement).value; }}>
                <option value="">选择类型</option>
                <option value="device">设备</option>
                <option value="space">空间</option>
                <option value="group">分组</option>
              </select>
            </div>
            <div class="field">
              <label class="label">父级（可选）</label>
              <input
                type="text"
                class="input"
                placeholder="父级物 ID"
                .value=${this.createParentId}
                @input=${(e: Event) => { this.createParentId = (e.target as HTMLInputElement).value; }}
              />
            </div>
            <div class="field">
              <label class="label">模板（可选）</label>
              <input
                type="text"
                class="input"
                placeholder="模板 ID"
                .value=${this.createTemplateId}
                @input=${(e: Event) => { this.createTemplateId = (e.target as HTMLInputElement).value; }}
              />
            </div>
          </div>
          <div class="modal-footer" style="display: flex; justify-content: flex-end; gap: var(--space-2); padding: var(--space-3) var(--space-4); border-top: 1px solid var(--border);">
            <button class="btn btn--ghost" @click=${this.closeCreateModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.createSaving || !this.createName.trim()} @click=${this.submitCreate}>
              ${this.createSaving ? "创建中..." : "创建"}
            </button>
          </div>
        </div>
      </div>
    `;
  }
}
