/**
 * 场景交互树组件（X3a 前端，Task 8）。
 *
 * 数据形态（实现者抉择，详见 task-8-report.md）：
 * - 树本体 = dry-run 返回的 treePreview 文本解析（缩进树，与后端展开结果严格一致）
 * - 场景节点 key / 设备判定 / 属性数 / 参数化标记 = 客户端对场景包结构
 *   （模板 device_info JSON）做同序深度优先走查后按位置对齐叠加
 * - 待建节点（extra_nodes）不提交 dry-run，仅本地叠加渲染；确认时由
 *   buildExtraNodesPayload 组装进实例化请求
 *
 * Task 9 的树拖拽复用本组件的渲染与数据形态。
 */
import { html, nothing, type TemplateResult } from "lit";

// ──────────────────────────────────────────────
// 场景包结构（模板 device_info JSON 的最小投影，键保持 snake_case 原样）
// ──────────────────────────────────────────────

export interface SceneStructNode {
  key?: string;
  category?: string;
  count?: number;
  count_param?: string;
  scene_ref?: string;
  template_ref?: string;
  param_mapping?: Record<string, string>;
  properties?: unknown[];
  children?: SceneStructNode[];
}

export interface SceneStructRoot {
  parameters?: { name: string; default: number }[];
  children?: SceneStructNode[];
}

/** 后端 scene_ref 嵌套深度上限（与 crates/db/src/scene_template.rs 一致） */
const MAX_SCENE_REF_DEPTH = 5;
/** attach_simulated_driver 填充的模拟驱动名（与后端 SIMULATOR_DRIVER 一致） */
export const SIMULATOR_DRIVER = "simulator";

// ──────────────────────────────────────────────
// 树节点与待建节点
// ──────────────────────────────────────────────

/** 驱动标记三态（D11：必须配文字标签，非纯色编码） */
export type TreeDriver = "sim" | "physical" | "none";

export interface PendingNode {
  id: string;
  templateId: string;
  category: string;
  propertyCount: number | null;
  name: string;
  count: number;
  /** 显式驱动名；"simulator"=模拟，null=无驱动，其余=物理驱动 */
  driverName: string | null;
  /** 场景节点 key 路径（[] = 挂根） */
  parentPath: string[];
}

export interface SceneTreeNode {
  id: string;
  name: string;
  category: string;
  children: SceneTreeNode[];
  /** 场景节点 key 路径；null = 不可作为挂载点（结构未解析/无 key/待建节点） */
  keyPath: string[] | null;
  propertyCount: number | null;
  /** template_ref 展开的叶设备（驱动标记与挂载按钮的依据） */
  isDevice: boolean;
  /** 可树上改数的场景参数名（仅当该参数在详情 parameters 中可编辑） */
  countParam: string | null;
  /** 驱动标记；null = 不显示（容器节点） */
  driver: TreeDriver | null;
  driverName?: string | null;
  pending?: PendingNode;
}

export interface ExtraNodePayload {
  templateId: string;
  parentPath: string[];
  name: string;
  count: number;
  driverName: string | null;
}

export interface SceneTreeDriverOption {
  value: string;
  label: string;
}

export interface SceneTreeHost {
  onTreeParamStep(param: string, delta: number): void;
  onTreeAddDevice(keyPath: string[]): void;
  onTreeRemovePending(id: string): void;
  onTreePendingDriver(id: string, driver: string | null): void;
  /** 在该节点下渲染行内「添加设备」表单（未打开时返回 nothing/null） */
  renderAddDeviceForm?(keyPath: string[]): unknown;
}

// ──────────────────────────────────────────────
// treePreview 解析
// ──────────────────────────────────────────────

/** 解析后端 tree_preview（两级空格缩进 + `名称 (category)`）为嵌套树。 */
export function parseTreePreview(text: string): SceneTreeNode[] {
  const roots: SceneTreeNode[] = [];
  const stack: { depth: number; node: SceneTreeNode }[] = [];
  let seq = 0;
  for (const raw of text.split("\n")) {
    if (!raw.trim()) continue;
    const indent = raw.length - raw.trimStart().length;
    const depth = Math.floor(indent / 2);
    const label = raw.trim();
    const m = /^(.*) \(([^()]*)\)$/.exec(label);
    const node: SceneTreeNode = {
      id: `n${seq++}`,
      name: m ? m[1] : label,
      category: m ? m[2] : "",
      children: [],
      keyPath: null,
      propertyCount: null,
      isDevice: false,
      countParam: null,
      driver: null,
    };
    while (stack.length && stack[stack.length - 1].depth >= depth) stack.pop();
    if (stack.length === 0) roots.push(node);
    else stack[stack.length - 1].node.children.push(node);
    stack.push({ depth, node });
  }
  return roots;
}

// ──────────────────────────────────────────────
// 结构走查（key/设备/属性数/参数化 叠加）
// ──────────────────────────────────────────────

interface Skeleton {
  key: string | null;
  isDevice: boolean;
  countParam: string | null;
  propertyCount: number | null;
  children: Skeleton[];
}

interface ExpandCtx {
  refScenes: Map<string, SceneStructRoot>;
  deviceTemplates: Map<string, { propertyCount?: number; category?: string }>;
}

function resolveCount(n: SceneStructNode, params: Record<string, number>): number {
  if (typeof n.count === "number") return Math.max(0, Math.trunc(n.count));
  if (n.count_param) {
    const v = params[n.count_param];
    if (typeof v === "number" && !Number.isNaN(v)) return Math.max(0, Math.trunc(v));
    return 1;
  }
  return 1;
}

/** 与后端展开器同序（深度优先、count 展开）生成骨架树；名称以 treePreview 为准，这里只取结构信息。 */
function expandStructNodes(
  nodes: SceneStructNode[],
  params: Record<string, number>,
  ctx: ExpandCtx,
  refStack: string[],
): Skeleton[] {
  const out: Skeleton[] = [];
  for (const n of nodes ?? []) {
    if (n.scene_ref) {
      // 环/超深/缺引用：跳过该子树（对应预览节点拿不到 key，不可挂接，降级安全）
      if (refStack.includes(n.scene_ref) || refStack.length >= MAX_SCENE_REF_DEPTH) continue;
      const target = ctx.refScenes.get(n.scene_ref);
      if (!target) continue;
      const copies = resolveCount(n, params);
      // 目标参数空间：目标默认 ← param_mapping {目标参数: 本模板参数}
      const mapped: Record<string, number> = {};
      for (const p of target.parameters ?? []) mapped[p.name] = p.default;
      for (const [tp, sp] of Object.entries(n.param_mapping ?? {})) {
        if (typeof params[sp] === "number") mapped[tp] = params[sp];
      }
      for (let i = 0; i < copies; i++) {
        out.push({
          key: n.key ?? null,
          isDevice: false,
          countParam: n.count_param ?? null,
          propertyCount: Array.isArray(n.properties) && n.properties.length > 0 ? n.properties.length : null,
          children: expandStructNodes(target.children ?? [], mapped, ctx, [...refStack, n.scene_ref]),
        });
      }
      continue;
    }
    const copies = resolveCount(n, params);
    const isDevice = !!n.template_ref;
    const tpl = n.template_ref ? ctx.deviceTemplates.get(n.template_ref) : undefined;
    const ownProps = Array.isArray(n.properties) && n.properties.length > 0 ? n.properties.length : null;
    for (let i = 0; i < copies; i++) {
      out.push({
        key: n.key ?? null,
        isDevice,
        countParam: n.count_param ?? null,
        propertyCount: ownProps ?? tpl?.propertyCount ?? null,
        children: expandStructNodes(n.children ?? [], params, ctx, refStack),
      });
    }
  }
  return out;
}

interface AssignCtx {
  editableParams: Set<string>;
  attachSim: boolean;
}

/** 按位置（兄弟序）把骨架信息叠加到预览树上；数量不一致时只对齐前缀，余者保持不可挂接。 */
function assignMeta(previewNodes: SceneTreeNode[], skels: Skeleton[], parentPath: string[] | null, ctx: AssignCtx) {
  const n = Math.min(previewNodes.length, skels.length);
  for (let i = 0; i < n; i++) {
    const p = previewNodes[i];
    const s = skels[i];
    if (parentPath !== null) {
      p.keyPath = s.key != null ? [...parentPath, s.key] : parentPath.length === 0 ? [] : null;
    }
    p.isDevice = s.isDevice;
    p.countParam = s.countParam && ctx.editableParams.has(s.countParam) ? s.countParam : null;
    p.propertyCount = s.propertyCount;
    if (s.isDevice) p.driver = ctx.attachSim ? "sim" : "none";
    assignMeta(p.children, s.children, p.keyPath, ctx);
  }
}

function findByKeyPath(nodes: SceneTreeNode[], path: string[]): SceneTreeNode | null {
  for (const n of nodes) {
    if (n.keyPath && n.keyPath.length === path.length && n.keyPath.every((k, i) => k === path[i])) return n;
    const hit = findByKeyPath(n.children, path);
    if (hit) return hit;
  }
  return null;
}

export interface BuildSceneTreeArgs {
  previewText: string;
  /** 场景包结构（模板 device_info JSON 解析结果）；缺省时树仅渲染，不可挂接/改数 */
  rootScene: SceneStructRoot | null;
  rootName?: string;
  refScenes?: Map<string, SceneStructRoot>;
  deviceTemplates?: Map<string, { propertyCount?: number; category?: string }>;
  params: Record<string, number>;
  editableParams?: Set<string>;
  rootPropertyCount?: number | null;
  attachSim: boolean;
  pending?: PendingNode[];
}

export function buildSceneTree(args: BuildSceneTreeArgs): SceneTreeNode[] {
  const roots = parseTreePreview(args.previewText);
  if (roots.length === 0) return roots;
  if (args.rootScene) {
    const expandCtx: ExpandCtx = {
      refScenes: args.refScenes ?? new Map(),
      deviceTemplates: args.deviceTemplates ?? new Map(),
    };
    const rootSkel: Skeleton = {
      key: null,
      isDevice: false,
      countParam: null,
      propertyCount: args.rootPropertyCount ?? null,
      children: expandStructNodes(args.rootScene.children ?? [], args.params, expandCtx, [args.rootName ?? ""]),
    };
    assignMeta(roots, [rootSkel], [], {
      editableParams: args.editableParams ?? new Set(),
      attachSim: args.attachSim,
    });
  }
  for (const p of args.pending ?? []) {
    const mount = findByKeyPath(roots, p.parentPath);
    if (!mount) continue;
    mount.children.push({
      id: `pending:${p.id}`,
      name: p.count > 1 ? `${p.name} ×${p.count}` : p.name,
      category: p.category,
      children: [],
      keyPath: null,
      propertyCount: p.propertyCount,
      isDevice: true,
      countParam: null,
      driver: p.driverName == null ? "none" : p.driverName === SIMULATOR_DRIVER ? "sim" : "physical",
      driverName: p.driverName,
      pending: p,
    });
  }
  return roots;
}

// ──────────────────────────────────────────────
// 提交 payload 组装
// ──────────────────────────────────────────────

export function buildExtraNodesPayload(pending: PendingNode[]): ExtraNodePayload[] {
  return pending.map((p) => ({
    templateId: p.templateId,
    parentPath: [...p.parentPath],
    name: p.name,
    count: p.count,
    driverName: p.driverName,
  }));
}

export function pendingNodeTotal(pending: PendingNode[]): number {
  return pending.reduce((s, p) => s + p.count, 0);
}

// ──────────────────────────────────────────────
// 渲染
// ──────────────────────────────────────────────

const DRIVER_LABELS: Record<TreeDriver, string> = {
  sim: "模拟",
  physical: "物理",
  none: "无",
};

export interface RenderSceneTreeOpts {
  editable?: boolean;
  /** 待建节点驱动下拉的候选（不含时请宿主自行渲染或隐藏下拉） */
  driverOptions?: SceneTreeDriverOption[];
}

export function renderSceneTree(
  host: SceneTreeHost,
  nodes: SceneTreeNode[],
  opts: RenderSceneTreeOpts = {},
): TemplateResult | typeof nothing {
  if (!nodes.length) return nothing;
  return html`<div class="st-tree" role="tree">${nodes.map((n) => renderNode(host, n, 0, opts))}</div>`;
}

function renderNode(
  host: SceneTreeHost,
  node: SceneTreeNode,
  depth: number,
  opts: RenderSceneTreeOpts,
): TemplateResult {
  const editable = opts.editable !== false;
  return html`
    <div
      class="st-node ${node.pending ? "st-node--pending" : ""}"
      role="treeitem"
      data-node-id=${node.id}
      style=${node.pending
        ? "border-style: dashed; border-width: 1px; border-color: var(--border); border-radius: var(--radius-sm, 6px); margin: 2px 0; padding: 2px 4px;"
        : ""}
    >
      <div class="st-row" style="display: flex; align-items: center; gap: var(--space-2, 8px); flex-wrap: wrap; padding: 2px 0; padding-left: ${depth * 16}px;">
        <span class="st-name" style=${depth === 0 ? "font-weight: 600;" : ""}>${node.name}</span>
        ${node.category ? html`<span class="mp-type-badge">${node.category}</span>` : nothing}
        ${typeof node.propertyCount === "number"
          ? html`<span class="st-props" style="color: var(--muted); font-size: 11px;">${node.propertyCount} 属性</span>`
          : nothing}
        ${node.driver
          ? html`<span class="mp-tag st-driver" data-driver=${node.driver}>${DRIVER_LABELS[node.driver]}</span>`
          : nothing}
        ${node.countParam && editable
          ? html`
            <span class="st-stepper" data-count-param=${node.countParam} style="display: inline-flex; gap: 2px;">
              <button class="btn btn--sm" data-step="-1" title="减少" @click=${() => host.onTreeParamStep(node.countParam!, -1)}>−</button>
              <button class="btn btn--sm" data-step="1" title="增加" @click=${() => host.onTreeParamStep(node.countParam!, 1)}>+</button>
            </span>`
          : nothing}
        ${node.pending && editable
          ? html`
            ${opts.driverOptions
              ? html`<select
                  class="st-pending-driver"
                  .value=${node.pending.driverName ?? "none"}
                  @change=${(e: Event) => {
                    const v = (e.target as HTMLSelectElement).value;
                    host.onTreePendingDriver(node.pending!.id, v === "none" ? null : v);
                  }}
                >
                  ${opts.driverOptions.map((o) => html`<option value=${o.value}>${o.label}</option>`)}
                </select>`
              : nothing}
            <button class="btn btn--sm st-pending-del" @click=${() => host.onTreeRemovePending(node.pending!.id)}>删除</button>`
          : nothing}
        ${!node.pending && !node.isDevice && node.keyPath && editable
          ? html`<button class="btn btn--sm st-add-device" @click=${() => host.onTreeAddDevice(node.keyPath!)}>+ 添加设备</button>`
          : nothing}
      </div>
      ${node.keyPath && host.renderAddDeviceForm ? (host.renderAddDeviceForm(node.keyPath) as TemplateResult | typeof nothing) : nothing}
      ${node.children.map((c) => renderNode(host, c, depth + 1, opts))}
    </div>
  `;
}
