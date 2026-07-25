import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { SignalWatcher } from "@lit-labs/signals";
import { thingApi, type Thing } from "../../api/things.js";
import { templateApi } from "../../api/templates.js";
import { driverApi } from "../../api/drivers.js";
import { i18n } from "../../i18n/index.js";
import { success, error as toastError } from "../components/toast.js";
import { icons } from "../icons.js";

type ViewMode = "list" | "tree";

const UPGRADE_NOTICE_KEY = "thing-ontology-upgrade-notice-dismissed";

interface TreeNode {
  thing: Thing;
  children: TreeNode[];
  depth: number;
}

// === Template wizard helpers ===

interface DeviceInfo {
  defaultNamePattern: string;
  defaultDisplayNamePattern?: string;
  defaultDescription?: Record<string, string>;
  defaultPosition?: string;
  requiredFields: string[];
}

interface ProcessedTemplate {
  id: string;
  name: string;
  displayName: Record<string, string>;
  description: Record<string, string> | null;
  category: string;
  version: string;
  manufacturer?: string;
  deviceType: string;
  protocolType?: string;
  driverName?: string;
  tags: string[];
  deviceInfo: DeviceInfo;
  properties: any[];
  actions: any[];
  isBuiltin: boolean;
}

function parseJsonField(raw: any, fallback: any): any {
  if (raw === null || raw === undefined) return fallback;
  if (typeof raw === "object") return raw;
  if (typeof raw === "string") {
    try { return JSON.parse(raw); } catch { return fallback; }
  }
  return fallback;
}

function transformTemplate(raw: any): ProcessedTemplate {
  return {
    id: raw.id,
    name: raw.name,
    displayName: parseJsonField(raw.displayName, {}),
    description: parseJsonField(raw.description, null),
    category: raw.category || "others",
    version: raw.version || "",
    manufacturer: raw.manufacturer,
    deviceType: raw.deviceType || "",
    protocolType: raw.protocolType,
    driverName: raw.driverName,
    tags: parseJsonField(raw.tags, []),
    deviceInfo: parseJsonField(raw.deviceInfo, { defaultNamePattern: raw.name, requiredFields: [] } as DeviceInfo),
    properties: parseJsonField(raw.properties, []),
    actions: parseJsonField(raw.actions || raw.commands, []), // back-compat: accept old "commands" key
    isBuiltin: raw.isBuiltin === 1 || raw.isBuiltin === true,
  };
}

function isFieldRequired(deviceInfo: DeviceInfo | undefined, fieldName: string): boolean {
  return deviceInfo?.requiredFields?.includes(fieldName) || false;
}

function getLocalizedText(obj: Record<string, string> | undefined, fallback: string): string {
  if (!obj || typeof obj !== "object") return fallback;
  const locale = i18n.getLocale();
  if (locale.startsWith("zh")) {
    return obj["zh"] || obj["en"] || Object.values(obj)[0] || fallback;
  }
  return obj["en"] || obj["zh"] || Object.values(obj)[0] || fallback;
}

const CATEGORY_LABELS: Record<string, string> = {
  sensors: "传感器",
  controllers: "控制器",
  cameras: "摄像头",
  gateways: "网关",
  others: "其他",
};

const CATEGORY_ICONS: Record<string, ReturnType<typeof html>> = {
  sensors: html`
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="24" height="24">
      <path d="M14 4v10.54a4 4 0 1 1-4 0V4a2 2 0 0 1 4 0Z" />
    </svg>
  `,
  controllers: html`
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="24" height="24">
      <rect x="2" y="6" width="7" height="12" rx="2" />  <rect x="15" y="6" width="7" height="12" rx="2" />  <circle cx="5.5" cy="12" r="1.5" /><circle cx="18.5" cy="12" r="1.5" />
    </svg>
  `,
  cameras: html`
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="24" height="24">
      <path d="M23 7l-7 5 7 5V7z" />  <rect x="1" y="5" width="15" height="14" rx="2" />
    </svg>
  `,
  gateways: html`
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="24" height="24">
      <circle cx="12" cy="12" r="2" />  <path d="M12 2v4" />  <path d="M12 18v4" />  <path d="m4.93 4.93 2.83 2.83" />  <path d="m16.24 16.24 2.83 2.83" />  <path d="M2 12h4" />  <path d="M18 12h4" />
    </svg>
  `,
  others: html`
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="24" height="24">
      <rect x="3" y="3" width="18" height="18" rx="2" />  <circle cx="12" cy="12" r="3" />
    </svg>
  `,
};

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

  // Wizard (2-step template-based)
  @state() showWizard = false;
  @state() wizardStep: "template" | "device" = "template";
  @state() wizardLastFocus: HTMLElement | undefined;
  @state() wizTemplates: ProcessedTemplate[] = [];
  @state() wizTemplateLoading = false;
  @state() wizTemplateSearch = "";
  @state() wizSelectedTemplate: ProcessedTemplate | null = null;
  @state() wizName = "";
  @state() wizDescription = "";
  @state() wizAddress = "";
  @state() wizPosition = "";
  @state() wizDriver = "";
  @state() wizDriverConfig: Record<string, string> = {};
  @state() wizConfigOptions: any[] = [];
  @state() wizValidationErrors: Record<string, string> = {};
  @state() wizardSaving = false;
  @state() wizConfigLoading = false;
  @state() driverNames: string[] = [];

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
    this.loadDrivers();
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

  // === Wizard (2-step template-based) ===

  openWizard() {
    this.showWizard = true;
    this.wizardStep = "template";
    this.wizSelectedTemplate = null;
    this.wizTemplateSearch = "";
    this.wizName = "";
    this.wizDescription = "";
    this.wizAddress = "";
    this.wizPosition = "";
    this.wizDriver = "";
    this.wizDriverConfig = {};
    this.wizConfigOptions = [];
    this.wizValidationErrors = {};
    this.wizardSaving = false;
    this.loadTemplates();
    this.loadDrivers();
  }

  closeWizard() {
    this.showWizard = false;
  }

  // === Driver ===

  async loadDrivers() {
    try {
      const res = await driverApi.getDrivers({ page: 1, pageSize: 200 });
      const data = res.result;
      const rawList = data?.data || data || [];
      this.driverNames = (Array.isArray(rawList) ? rawList : []).map((d: any) => d.name || d.id || "").filter(Boolean);
    } catch {
      this.driverNames = [];
    }
  }

  async loadDriverConfig(driverName: string) {
    this.wizConfigLoading = true;
    try {
      const res = await driverApi.getDriverConfig(driverName);
      const data = res.result;
      if (data?.configOptions && Array.isArray(data.configOptions)) {
        this.wizConfigOptions = data.configOptions;
      } else if (Array.isArray(data)) {
        this.wizConfigOptions = data;
      } else {
        this.wizConfigOptions = [];
      }
    } catch {
      this.wizConfigOptions = [];
    } finally {
      this.wizConfigLoading = false;
    }
  }

  async onWizardDriverSelect(driverName: string) {
    this.wizDriver = driverName;
    this.wizDriverConfig = {};
    this.wizConfigOptions = [];
    if (driverName) {
      await this.loadDriverConfig(driverName);
    }
  }

  // === Templates ===

  async loadTemplates() {
    this.wizTemplateLoading = true;
    try {
      const res = await templateApi.getTemplates({ page: 1, pageSize: 200 });
      const data = res.result;
      const rawList = data?.data || data || [];
      this.wizTemplates = (Array.isArray(rawList) ? rawList : []).map(transformTemplate);
    } catch {
      this.wizTemplates = [];
    } finally {
      this.wizTemplateLoading = false;
    }
  }

  selectTemplate(template: ProcessedTemplate) {
    this.wizSelectedTemplate = template;
    const di = template.deviceInfo;
    this.wizName = di.defaultNamePattern
      ? di.defaultNamePattern.replace("{name}", template.name)
      : template.name;
    this.wizDescription = di.defaultDescription
      ? getLocalizedText(di.defaultDescription, "")
      : getLocalizedText(template.description || {}, "");
    this.wizAddress = "";
    this.wizPosition = di.defaultPosition || "";
    this.wizDriver = template.driverName || "";
    this.wizDriverConfig = {};
    this.wizConfigOptions = [];
    this.wizValidationErrors = {};
    if (this.wizDriver) {
      this.loadDriverConfig(this.wizDriver);
    }
    this.wizardStep = "device";
  }

  wizardBack() {
    this.wizardStep = "template";
    this.wizValidationErrors = {};
  }

  get filteredWizardTemplates(): ProcessedTemplate[] {
    const q = this.wizTemplateSearch.trim().toLowerCase();
    if (!q) return this.wizTemplates;
    return this.wizTemplates.filter(t => {
      const name = t.name?.toLowerCase() || "";
      const displayName = getLocalizedText(t.displayName, "").toLowerCase();
      const desc = t.description ? Object.values(t.description).join(" ").toLowerCase() : "";
      return name.includes(q) || displayName.includes(q) || desc.includes(q);
    });
  }

  get wizardTemplatesByCategory(): Record<string, ProcessedTemplate[]> {
    const groups: Record<string, ProcessedTemplate[]> = {};
    for (const t of this.filteredWizardTemplates) {
      const cat = t.category || "others";
      if (!groups[cat]) groups[cat] = [];
      groups[cat].push(t);
    }
    return groups;
  }

  validateWizardForm(): boolean {
    const errors: Record<string, string> = {};
    if (!this.wizName.trim()) {
      errors.name = "物名称不能为空";
    } else if (this.wizName.trim().length < 2) {
      errors.name = "物名称至少需要2个字符";
    } else if (this.wizName.trim().length > 50) {
      errors.name = "物名称不能超过50个字符";
    }
    if (this.wizSelectedTemplate && isFieldRequired(this.wizSelectedTemplate.deviceInfo, "address") && !this.wizAddress.trim()) {
      errors.address = "物地址是必填字段";
    }
    if (this.wizDriver && this.wizConfigOptions.length > 0) {
      for (const opt of this.wizConfigOptions) {
        if (opt.required) {
          const userValue = this.wizDriverConfig[opt.name];
          const hasUserValue = userValue !== undefined && userValue.trim() !== "";
          const hasDefaultValue = opt.defaultValue && opt.defaultValue.trim() !== "";
          if (!hasUserValue && !hasDefaultValue) {
            errors[`driverConfig.${opt.name}`] = `${opt.label}是必填字段`;
          }
        }
      }
    }
    this.wizValidationErrors = errors;
    return Object.keys(errors).length === 0;
  }

  async submitWizard() {
    if (!this.wizSelectedTemplate) {
      toastError("请先选择物模板");
      return;
    }
    if (!this.validateWizardForm()) {
      toastError("请检查并修正表单中的错误");
      return;
    }
    if (this.wizardSaving) return;
    this.wizardSaving = true;
    try {
      // Build driver config merging user values with defaults
      const finalDriverConfig: Record<string, string> = {};
      if (this.wizDriver && this.wizConfigOptions.length > 0) {
        for (const opt of this.wizConfigOptions) {
          const userValue = this.wizDriverConfig[opt.name];
          if (userValue !== undefined && userValue !== "") {
            finalDriverConfig[opt.name] = userValue;
          } else if (opt.defaultValue) {
            finalDriverConfig[opt.name] = opt.defaultValue;
          }
        }
      }
      const payload: Record<string, unknown> = {
        name: this.wizName.trim(),
        thingType: this.wizSelectedTemplate.deviceType || undefined,
        templateId: this.wizSelectedTemplate.id,
        deviceType: this.wizSelectedTemplate.deviceType || undefined,
        protocolType: this.wizSelectedTemplate.protocolType || undefined,
        driverName: this.wizDriver || undefined,
        driverOptions: Object.keys(finalDriverConfig).length > 0 ? JSON.stringify(finalDriverConfig) : undefined,
      };
      await thingApi.create(payload);
      success("物已创建");
      this.closeWizard();
      await this.loadThings();
    } catch (err: any) {
      toastError(err.message || "物创建失败");
    } finally {
      this.wizardSaving = false;
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
        ${this.showWizard ? this.renderWizard() : nothing}
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
        <button class="btn btn--primary" @click=${this.openWizard}>创建物</button>
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
        <button class="btn btn--primary" @click=${this.openWizard}>创建第一个物</button>
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

  // === Wizard (2-step template-based) ===

  renderWizard() {
    const isStep1 = this.wizardStep === "template";
    return html`
      <div class="wizard-overlay" role="dialog" aria-modal="true" aria-label="物创建向导" @click=${(e: Event) => { if ((e.target as HTMLElement).classList.contains("wizard-overlay")) this.closeWizard(); }} @keydown=${(e: KeyboardEvent) => { if (e.key === "Escape") this.closeWizard(); }}>
        <div class="wizard-dialog">
          <div class="wizard-dialog__header">
            <button class="wizard-dialog__back" aria-label="返回" @click=${isStep1 ? this.closeWizard : this.wizardBack}>
              <span class="rotate-90">${icons.arrowDown}</span>
              <span>${isStep1 ? "返回物列表" : "返回模板选择"}</span>
            </button>
            <span class="wizard-dialog__title">${isStep1 ? "选择物模板" : "填写物信息"}</span>
            <button class="modal-close wizard-dialog__close" aria-label="关闭" @click=${this.closeWizard}>&times;</button>
          </div>
          <div class="wizard-dialog__body">
            ${isStep1 ? this.renderWizardTemplateSelection() : this.renderWizardDeviceInfo()}
          </div>
          ${!isStep1 ? html`
            <div class="wizard-form-footer">
              <button class="btn btn--ghost" @click=${this.wizardBack}>上一步</button>
              <button class="btn btn--primary" ?disabled=${this.wizardSaving || !this.wizName.trim()} @click=${this.submitWizard}>
                ${this.wizardSaving ? "创建中..." : "创建物"}
              </button>
            </div>
          ` : nothing}
        </div>
      </div>
    `;
  }

  renderWizardTemplateSelection() {
    const groups = this.wizardTemplatesByCategory;
    const categories = Object.keys(groups);
    return html`
      <div class="wizard-search">
        <span class="wizard-search__icon">${icons.search}</span>
        <input type="text" class="wizard-search__input" placeholder="搜索物模板..."
          .value=${this.wizTemplateSearch}
          @input=${(e: Event) => { this.wizTemplateSearch = (e.target as HTMLInputElement).value; }}
        />
      </div>
      ${this.wizTemplateLoading ? html`
        <div class="wizard-loading">
          <span class="loading-spinner"></span>
          <span class="wizard-loading__text">加载中...</span>
        </div>
      ` : this.filteredWizardTemplates.length === 0 ? html`
        <div class="wizard-empty">
          <div class="wizard-empty__icon">&#128230;</div>
          <div class="wizard-empty__title">没有找到匹配的模板</div>
          <div class="wizard-empty__hint">尝试调整搜索条件或浏览其他分类</div>
        </div>
      ` : html`
        ${categories.map(cat => html`
          <div class="wizard-category">
            <div class="wizard-category__header">
              <span class="wizard-category__title">${CATEGORY_LABELS[cat] || cat}</span>
              <span class="wizard-category__count">${groups[cat].length} 个模板</span>
            </div>
            <div class="wizard-template-grid">
              ${groups[cat].map(t => this.renderTemplateCard(t))}
            </div>
          </div>
        `)}
      `}
    `;
  }

  renderTemplateCard(t: ProcessedTemplate) {
    const displayName = getLocalizedText(t.displayName, t.name);
    return html`
      <div class="card template-card" @click=${() => this.selectTemplate(t)}>
        <div class="template-card__header">
          <span class="template-card__icon">${CATEGORY_ICONS[t.category] || CATEGORY_ICONS.others}</span>
          <div class="template-card__title-wrap">
            <div class="template-card__title">${displayName}</div>
            ${t.manufacturer ? html`<div class="inline-muted">${t.manufacturer}</div>` : nothing}
          </div>
          ${t.isBuiltin ? html`<span class="template-card__badge">内置</span>` : nothing}
        </div>
        <div class="template-card__meta">
          ${t.deviceType ? html`<span>${t.deviceType}</span>` : nothing}
          ${t.protocolType ? html`<span>${t.protocolType}</span>` : nothing}
          ${t.version ? html`<span>v${t.version}</span>` : nothing}
        </div>
        <div class="template-card__stats">
          <span>${t.properties.length} 属性</span>
          <span>${t.actions.length} 动作</span>
        </div>
      </div>
    `;
  }

  renderWizardDeviceInfo() {
    const t = this.wizSelectedTemplate;
    if (!t) return nothing;
    const displayName = getLocalizedText(t.displayName, t.name);
    const hasError = (name: string) => Boolean(this.wizValidationErrors[name]);
    const getError = (name: string) => this.wizValidationErrors[name] || "";

    return html`
      <div class="wizard-split">
        <!-- Left panel: form -->
        <div class="wizard-split__form wizard-fields">
          <div class="wizard-form-header">
            <div class="wizard-form-header__title">填写物信息</div>
            <button class="btn btn--ghost btn--sm" @click=${this.wizardBack}>切换模板</button>
          </div>

          <!-- Template summary chip -->
          <div class="template-chip">
            <span class="template-chip__icon">${CATEGORY_ICONS[t.category] || CATEGORY_ICONS.others}</span>
            <div class="template-chip__title-wrap">
              <div class="template-chip__title">${displayName}</div>
              <div class="template-chip__meta">
                ${t.manufacturer ? html`<span>${t.manufacturer} · </span>` : nothing}
                <span>${t.deviceType || t.category}</span>
                ${t.version ? html` · v${t.version}` : nothing}
              </div>
            </div>
            ${t.isBuiltin ? html`<span class="template-chip__badge">内置</span>` : nothing}
          </div>

          <!-- Name -->
          <div class="field ${hasError("name") ? "field--error" : ""}">
            <span>物名称 <span class="form-label-required">*</span></span>
            <input type="text" placeholder="请输入物名称"
              .value=${this.wizName}
              @input=${(e: any) => { this.wizName = e.target.value; }}
            />
            ${hasError("name") ? html`<div class="form-error">${getError("name")}</div>` : nothing}
          </div>

          <!-- Description -->
          <div class="field">
            <span>物描述 <span class="inline-muted">(可选)</span></span>
            <textarea rows="2" placeholder="请输入物描述"
              .value=${this.wizDescription}
              @input=${(e: any) => { this.wizDescription = e.target.value; }}
            ></textarea>
          </div>

          <!-- Address -->
          <div class="field ${hasError("address") ? "field--error" : ""}">
            <span>物地址 ${isFieldRequired(t.deviceInfo, "address")
              ? html`<span class="form-label-required">*</span>`
              : html`<span class="inline-muted">(可选)</span>`}</span>
            <input type="text" placeholder="请输入物IP地址或连接地址"
              .value=${this.wizAddress}
              @input=${(e: any) => { this.wizAddress = e.target.value; }}
            />
            ${hasError("address") ? html`<div class="form-error">${getError("address")}</div>` : nothing}
          </div>

          <!-- Position -->
          <div class="field">
            <span>安装位置 <span class="inline-muted">(可选)</span></span>
            <input type="text" placeholder="请输入物安装位置"
              .value=${this.wizPosition}
              @input=${(e: any) => { this.wizPosition = e.target.value; }}
            />
          </div>

          <!-- Driver select -->
          <div class="field">
            <span>物驱动 <span class="inline-muted">(选择适合的驱动程序来完成数据采集)</span></span>
            <select .value=${this.wizDriver} @change=${(e: Event) => this.onWizardDriverSelect((e.target as HTMLSelectElement).value)}>
              <option value="">请选择驱动</option>
              ${this.driverNames.map(name => html`<option value=${name}>${name}</option>`)}
            </select>
            ${t.driverName && this.wizDriver !== t.driverName ? html`
              <div class="form-hint">模板默认驱动: ${t.driverName}</div>
            ` : nothing}
          </div>

          <!-- Driver config -->
          ${this.wizDriver ? html`
            <div class="wizard-form-section">
              <div class="wizard-form-section__header">
                <span class="wizard-form-section__title">驱动配置</span>
                <span class="wizard-form-section__meta">(${this.wizDriver})</span>
              </div>
              ${this.wizConfigLoading ? html`
                <div class="wizard-loading wizard-loading--compact">
                  <span class="loading-spinner"></span>
                  <span class="wizard-loading__text">加载驱动配置参数...</span>
                </div>
              ` : this.wizConfigOptions.length > 0 ? html`
                ${this.wizConfigOptions.map((opt: any) => this.renderWizardConfigField(opt))}
              ` : html`
                <div class="empty-hint--sm">该驱动无需额外配置参数</div>
              `}
            </div>
          ` : nothing}
        </div>

        <!-- Right panel: template overview -->
        <div class="wizard-split__overview">
          ${this.renderTemplateOverview(t)}
        </div>
      </div>
    `;
  }

  renderWizardConfigField(opt: any) {
    const value = this.wizDriverConfig[opt.name] ?? "";
    const hasError = Boolean(this.wizValidationErrors[`driverConfig.${opt.name}`]);
    const errorMsg = this.wizValidationErrors[`driverConfig.${opt.name}`] || "";
    const placeholder = opt.defaultValue ? `默认: ${opt.defaultValue}` : `请输入${opt.label}`;

    return html`
      <div class="field ${hasError ? "field--error" : ""}">
        <span>
          ${opt.label}
          ${opt.required ? html`<span class="form-label-required">*</span>` : html`<span class="inline-muted">(可选)</span>`}
          ${opt.defaultValue ? html`<span class="inline-muted inline-muted--spaced">· 默认: ${opt.defaultValue}</span>` : nothing}
        </span>
        ${opt.optionType === "boolean" ? html`
          <select .value=${value || (opt.defaultValue === "true" ? "true" : "false")} @change=${(e: Event) => {
            this.wizDriverConfig = { ...this.wizDriverConfig, [opt.name]: (e.target as HTMLSelectElement).value };
          }}>
            <option value="">请选择</option>
            <option value="true">是</option>
            <option value="false">否</option>
          </select>
        ` : opt.optionType === "number" ? html`
          <input type="number" .value=${value} placeholder=${placeholder} @input=${(e: any) => {
            this.wizDriverConfig = { ...this.wizDriverConfig, [opt.name]: e.target.value };
          }} />
        ` : html`
          <input type="text" .value=${value} placeholder=${placeholder} @input=${(e: any) => {
            this.wizDriverConfig = { ...this.wizDriverConfig, [opt.name]: e.target.value };
          }} />
        `}
        ${hasError ? html`<div class="form-error">${errorMsg}</div>` : nothing}
      </div>
    `;
  }

  renderTemplateOverview(t: ProcessedTemplate) {
    const displayName = getLocalizedText(t.displayName, t.name);
    const description = getLocalizedText(t.description ?? undefined, "");
    const totalProps = t.properties.length;
    const totalActs = t.actions.length;
    const readonlyProps = t.properties.filter((p: any) => p.accessMode === "r" || p.accessMode === "R").length;
    const writableProps = totalProps - readonlyProps;

    return html`
      <div class="template-overview__summary">
        <span class="template-overview__icon">${CATEGORY_ICONS[t.category] || CATEGORY_ICONS.others}</span>
        <div class="template-overview__title-wrap">
          <div class="template-overview__title">${displayName}</div>
          <div class="template-overview__meta">
            ${t.manufacturer ? html`${t.manufacturer} · ` : nothing}${t.deviceType || t.category}${t.version ? html` · v${t.version}` : nothing}
          </div>
        </div>
        ${t.isBuiltin ? html`<span class="template-overview__badge">内置</span>` : nothing}
      </div>
      ${description ? html`<div class="template-overview__desc">${description}</div>` : nothing}
      <div class="template-overview__meta-tags">
        ${t.protocolType ? html`<span class="template-overview__meta-tag">协议: ${t.protocolType}</span>` : nothing}
        ${t.driverName ? html`<span class="template-overview__meta-tag">驱动: ${t.driverName}</span>` : nothing}
        ${t.category ? html`<span class="template-overview__meta-tag">${CATEGORY_LABELS[t.category] || t.category}</span>` : nothing}
      </div>
      ${t.tags && t.tags.length > 0 ? html`
        <div class="template-overview__tags">
          ${t.tags.map((tag: string) => html`<span class="template-overview__tag">${tag}</span>`)}
        </div>
      ` : nothing}
      <div class="wizard-overview__stats">
        <div class="wizard-overview__stat">
          <div class="wizard-overview__stat-value">${totalProps}</div>
          <div class="wizard-overview__stat-label">属性数</div>
        </div>
        <div class="wizard-overview__stat">
          <div class="wizard-overview__stat-value">${totalActs}</div>
          <div class="wizard-overview__stat-label">动作数</div>
        </div>
        <div class="wizard-overview__stat">
          <div class="wizard-overview__stat-value">${readonlyProps}</div>
          <div class="wizard-overview__stat-label">只读属性</div>
        </div>
        <div class="wizard-overview__stat">
          <div class="wizard-overview__stat-value">${writableProps}</div>
          <div class="wizard-overview__stat-label">可写属性</div>
        </div>
      </div>
      ${totalProps > 0 ? html`
        <div class="wizard-overview__section-title">属性列表</div>
        <ul class="wizard-overview__list template-overview__list">
          ${t.properties.map((p: any) => html`
            <li class="wizard-overview__list-item">
              <div class="template-overview__list-item-inner">
                <span class="wizard-overview__list-item-name">${p.name || p.displayName || "unnamed"}</span>
                ${p.accessMode === "r" || p.accessMode === "R"
                  ? html`<span class="template-overview__list-badge-ro">R</span>`
                  : html`<span class="template-overview__list-badge-rw">RW</span>`
                }
              </div>
              <span class="wizard-overview__list-item-meta">
                ${p.dataType || ""}${p.unit ? ` ${p.unit}` : ""}
                ${p.minValue != null || p.maxValue != null
                  ? html` <span class="template-overview__range">[${p.minValue ?? '–'}~${p.maxValue ?? '–'}]</span>`
                  : nothing
                }
              </span>
            </li>
          `)}
        </ul>
      ` : nothing}
      ${totalActs > 0 ? html`
        <div class="wizard-overview__section-title">动作列表</div>
        <ul class="wizard-overview__list template-overview__list--commands">
          ${t.actions.map((a: any) => html`
            <li class="wizard-overview__list-item">
              <div class="template-overview__list-item-inner">
                <span class="wizard-overview__list-item-name">${a.name || "unnamed"}</span>
                ${a.parameters && a.parameters.length > 0
                  ? html`<span class="template-overview__param-count">${a.parameters.length} 参数</span>`
                  : nothing
                }
              </div>
              <span class="wizard-overview__list-item-meta">${a.description || ""}</span>
            </li>
          `)}
        </ul>
      ` : nothing}
      ${totalProps === 0 && totalActs === 0 ? html`
        <div class="empty-hint--sm">该模板暂无属性和动作定义</div>
      ` : nothing}
    `;
  }
}
