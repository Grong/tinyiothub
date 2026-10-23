/**
 * 派生属性编辑器 drawer（园区物派生属性 Task 11，spec §四 交互状态表）。
 *
 * - 物选择器 renderThingPicker：树形+搜索，选物后列出数值属性（data_type
 *   float/int/number/double，大小写不敏感），选中回调 onPick。
 *   reuse: scene-instantiator picker (TODOS P2) —— 场景实例化 picker 可直接复用本函数。
 * - 表达式区：只读文本域 + 插入按钮（添加引用/运算符/descendant_count()），
 *   插入即追加到末尾（不做光标级编辑，YAGNI）；文本域本身即实时预览。
 * - 保存走 Task 8 的 POST/PUT properties API；400 明细文本原样行内红字
 *   展示在表达式区下方（不做前端二次解析）。
 */
import { html, nothing } from "lit";
import { ref } from "lit/directives/ref.js";
import { thingApi } from "../../api/things.js";
import { success } from "../components/toast.js";
import type { ThingProperty } from "../../types/index.js";

// ---------------------------------------------------------------------------
// 物选择器（可复用）
// reuse: scene-instantiator picker (TODOS P2)
// ---------------------------------------------------------------------------

export interface PickerThing {
  id: string;
  name: string;
  displayName?: string;
  parentId?: string | null;
}

export interface PickerProperty {
  name: string;
  displayName?: string;
  dataType?: string;
}

export interface PickedRef {
  thingId: string;
  thingName: string;
  propertyName: string;
}

export interface ThingPickerOptions {
  onPick: (ref: PickedRef) => void;
  /** 只显示数值属性（默认 true） */
  filterNumericProps?: boolean;
  /** 测试/复用方注入；默认走 thingApi（workspace 内全量列表，前端组树） */
  fetchThings?: () => Promise<PickerThing[]>;
  fetchProperties?: (thingId: string) => Promise<PickerProperty[]>;
}

const NUMERIC_DATA_TYPES = new Set(["float", "int", "number", "double"]);

export function isNumericDataType(dataType?: string): boolean {
  return NUMERIC_DATA_TYPES.has((dataType || "").trim().toLowerCase());
}

async function defaultFetchThings(): Promise<PickerThing[]> {
  // workspace 内全量（不带分页参数，后端 limit 缺省即全量）——与既有物列表
  // 视图同源（thingApi.list），不新造分页协议；树由前端按 parentId 组装。
  const res = await thingApi.list();
  return ((res.result as any)?.items || []) as PickerThing[];
}

async function defaultFetchProperties(thingId: string): Promise<PickerProperty[]> {
  const res = await thingApi.getProperties(thingId);
  return ((res.result as any) || []) as PickerProperty[];
}

/**
 * 渲染物选择器到 container（命令式 DOM，不依赖宿主框架）。
 * 树形（按 parentId 分组缩进）+ 搜索过滤；选物 → 数值属性列表 → onPick。
 */
export function renderThingPicker(container: HTMLElement, opts: ThingPickerOptions): void {
  const filterNumeric = opts.filterNumericProps !== false;
  const fetchThings = opts.fetchThings || defaultFetchThings;
  const fetchProperties = opts.fetchProperties || defaultFetchProperties;

  container.classList.add("thing-picker");
  container.innerHTML = `
    <input type="search" class="thing-picker__search" placeholder="搜索物名称…" aria-label="搜索物" />
    <div class="thing-picker__body" role="listbox" aria-label="候选物"></div>
  `;
  const searchInput = container.querySelector<HTMLInputElement>(".thing-picker__search")!;
  const body = container.querySelector<HTMLElement>(".thing-picker__body")!;

  let things: PickerThing[] = [];
  let search = "";
  let selectedThingId: string | null = null;
  let propsLoading = false;
  let propsError = "";
  let properties: PickerProperty[] = [];

  function el(tag: string, cls: string, text: string): HTMLElement {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    e.textContent = text;
    return e;
  }

  function renderPropsPanel(parent: HTMLElement): void {
    if (!selectedThingId) return;
    if (propsLoading) {
      parent.appendChild(el("div", "thing-picker__hint", "加载中…"));
      return;
    }
    if (propsError) {
      const bar = el("div", "thing-picker__error", propsError);
      const retry = el("button", "btn btn--ghost btn--xs", "重试");
      retry.addEventListener("click", () => { void selectThing(selectedThingId!); });
      bar.appendChild(retry);
      parent.appendChild(bar);
      return;
    }
    const shown = filterNumeric ? properties.filter(p => isNumericDataType(p.dataType)) : properties;
    if (shown.length === 0) {
      parent.appendChild(el("div", "thing-picker__hint", "该物没有数值属性"));
      return;
    }
    for (const p of shown) {
      const btn = el("button", "thing-picker__prop", p.displayName ? `${p.displayName}（${p.name}）` : p.name);
      btn.setAttribute("type", "button");
      btn.setAttribute("data-prop", p.name);
      btn.addEventListener("click", () => {
        const thing = things.find(t => t.id === selectedThingId);
        if (thing) opts.onPick({ thingId: thing.id, thingName: thing.name, propertyName: p.name });
      });
      parent.appendChild(btn);
    }
  }

  function thingButton(t: PickerThing, depth: number): HTMLElement {
    const btn = el(
      "button",
      "thing-picker__thing" + (t.id === selectedThingId ? " thing-picker__thing--active" : ""),
      t.displayName ? `${t.displayName}（${t.name}）` : t.name,
    );
    btn.setAttribute("type", "button");
    btn.setAttribute("data-thing", t.id);
    btn.style.paddingLeft = `${8 + depth * 16}px`;
    btn.addEventListener("click", () => { void selectThing(t.id); });
    return btn;
  }

  function renderBody(): void {
    body.innerHTML = "";
    if (things.length === 0) {
      body.appendChild(el("div", "thing-picker__empty", "本工作区还没有带数值属性的物"));
      return;
    }
    if (search) {
      const q = search.toLowerCase();
      const matched = things.filter(t =>
        t.name.toLowerCase().includes(q) || (t.displayName || "").toLowerCase().includes(q));
      if (matched.length === 0) {
        const empty = el("div", "thing-picker__empty", "无匹配的物，");
        const clear = el("button", "thing-picker__clear", "清除搜索");
        clear.setAttribute("type", "button");
        clear.addEventListener("click", () => { searchInput.value = ""; search = ""; renderBody(); });
        empty.appendChild(clear);
        body.appendChild(empty);
        return;
      }
      for (const t of matched) body.appendChild(thingButton(t, 0));
    } else {
      const byParent = new Map<string, PickerThing[]>();
      const ids = new Set(things.map(t => t.id));
      const roots: PickerThing[] = [];
      for (const t of things) {
        if (t.parentId && ids.has(t.parentId)) {
          const arr = byParent.get(t.parentId) || [];
          arr.push(t);
          byParent.set(t.parentId, arr);
        } else {
          roots.push(t);
        }
      }
      const walk = (t: PickerThing, depth: number): void => {
        body.appendChild(thingButton(t, depth));
        for (const c of byParent.get(t.id) || []) walk(c, depth + 1);
      };
      for (const r of roots) walk(r, 0);
    }
    renderPropsPanel(body);
  }

  async function selectThing(id: string): Promise<void> {
    selectedThingId = id;
    propsLoading = true;
    propsError = "";
    properties = [];
    renderBody();
    try {
      const list = await fetchProperties(id);
      if (selectedThingId !== id) return; // 竞态：已改选其他物，丢弃过期结果
      properties = list;
      propsLoading = false;
    } catch (e: any) {
      if (selectedThingId !== id) return;
      propsLoading = false;
      propsError = e?.message || "属性加载失败";
    }
    renderBody();
  }

  searchInput.addEventListener("input", () => { search = searchInput.value.trim(); renderBody(); });

  body.appendChild(el("div", "thing-picker__hint", "加载中…"));
  void fetchThings().then(list => {
    things = list;
    renderBody();
  }).catch((e: any) => {
    body.innerHTML = "";
    const bar = el("div", "thing-picker__error", e?.message || "物列表加载失败");
    const retry = el("button", "btn btn--ghost btn--xs", "重试");
    retry.addEventListener("click", () => renderThingPicker(container, opts));
    bar.appendChild(retry);
    body.appendChild(bar);
  });
}

// ---------------------------------------------------------------------------
// 编辑器状态 + 行为
// ---------------------------------------------------------------------------

export interface DerivedEditorState {
  /** create=新增派生（POST）；edit=编辑派生（PUT 全量）；bind=普通数值行就地绑定（PUT 仅 expression） */
  mode: "create" | "edit" | "bind";
  propertyId?: string;
  name: string;
  displayName: string;
  unit: string;
  expression: string;
  pickerOpen: boolean;
  saving: boolean;
  saveError: string;
}

/** 最小宿主接口（DevicesView 满足；测试用 stub） */
export interface DerivedEditorHost {
  selectedDevice: { thing: { id: string } } | null;
  derivedEditor: DerivedEditorState | null;
  requestUpdate(): void;
  loadDeviceDetail(id: string): Promise<void> | void;
  handleModalKeydown?(e: KeyboardEvent, closeFn: () => void): void;
}

function setState(host: DerivedEditorHost, patch: Partial<DerivedEditorState>): void {
  if (!host.derivedEditor) return;
  host.derivedEditor = { ...host.derivedEditor, ...patch };
  host.requestUpdate();
}

export function openDerivedEditorCreate(host: DerivedEditorHost): void {
  host.derivedEditor = {
    mode: "create",
    name: "",
    displayName: "",
    unit: "",
    expression: "",
    pickerOpen: false,
    saving: false,
    saveError: "",
  };
  host.requestUpdate();
}

export function openDerivedEditorEdit(host: DerivedEditorHost, prop: ThingProperty): void {
  host.derivedEditor = {
    mode: "edit",
    propertyId: prop.id,
    name: prop.name || "",
    displayName: prop.displayName || "",
    unit: prop.unit || "",
    expression: prop.expression || "",
    pickerOpen: false,
    saving: false,
    saveError: "",
  };
  host.requestUpdate();
}

/** 绑定模式（园区物 Task 5）：普通数值行就地升级——属性名/显示名/单位预填只读，只编辑表达式 */
export function openDerivedEditorBind(host: DerivedEditorHost, prop: ThingProperty): void {
  host.derivedEditor = {
    mode: "bind",
    propertyId: prop.id,
    name: prop.name || "",
    displayName: prop.displayName || "",
    unit: prop.unit || "",
    expression: "",
    pickerOpen: false,
    saving: false,
    saveError: "",
  };
  host.requestUpdate();
}

export function closeDerivedEditor(host: DerivedEditorHost): void {
  if (host.derivedEditor?.saving) return;
  host.derivedEditor = null;
  host.requestUpdate();
}

/** 插入即追加到末尾（YAGNI：不做光标级编辑） */
export function appendDerivedExpression(host: DerivedEditorHost, text: string): void {
  if (!host.derivedEditor) return;
  setState(host, { expression: host.derivedEditor.expression + text, saveError: "" });
}

export async function saveDerivedEditor(host: DerivedEditorHost): Promise<void> {
  const st = host.derivedEditor;
  const thingId = host.selectedDevice?.thing?.id;
  if (!st || !thingId || st.saving) return;
  if (!st.name.trim()) {
    setState(host, { saveError: "请填写属性名" });
    return;
  }
  if (!st.expression.trim()) {
    setState(host, { saveError: "请构建表达式" });
    return;
  }
  setState(host, { saving: true, saveError: "" });
  try {
    if (st.mode === "bind" && st.propertyId) {
      // 绑定模式：只提交表达式（PUT 三态：缺省不动，Task 4）；属性名/单位保持原样
      await thingApi.updatePropertyDef(thingId, st.propertyId, { expression: st.expression });
    } else if (st.mode === "edit" && st.propertyId) {
      await thingApi.updatePropertyDef(thingId, st.propertyId, {
        name: st.name.trim(),
        displayName: st.displayName.trim() || undefined,
        unit: st.unit.trim() || undefined,
        expression: st.expression,
      });
    } else {
      await thingApi.createProperty(thingId, {
        name: st.name.trim(),
        displayName: st.displayName.trim() || undefined,
        unit: st.unit.trim() || undefined,
        expression: st.expression,
      });
    }
  } catch (e: any) {
    // 400 明细文本（语法定位/无效引用/环）原样行内展示，不二次解析
    setState(host, { saving: false, saveError: e?.message || "保存失败" });
    return;
  }
  const bindMode = st.mode === "bind";
  host.derivedEditor = null;
  host.requestUpdate();
  success(bindMode ? "表达式已绑定，属性已升级为派生" : "派生属性已保存");
  await host.loadDeviceDetail(thingId);
}

// ---------------------------------------------------------------------------
// drawer 渲染
// ---------------------------------------------------------------------------

const OPERATORS: { label: string; insert: string }[] = [
  { label: "+", insert: " + " },
  { label: "−", insert: " - " },
  { label: "×", insert: " * " },
  { label: "÷", insert: " / " },
  { label: "(", insert: "(" },
  { label: ")", insert: ")" },
];

function onDrawerKeydown(host: DerivedEditorHost, e: KeyboardEvent): void {
  if (e.key === "Enter") {
    // D11 精神：Enter=确认；但焦点在物选择器内（搜索框等）时不触发保存
    if ((e.target as HTMLElement)?.closest?.(".thing-picker")) return;
    e.preventDefault();
    void saveDerivedEditor(host);
    return;
  }
  if (e.key === "Escape") {
    e.preventDefault();
    closeDerivedEditor(host);
    return;
  }
  // Tab 焦点圈复用宿主 modal 处理
  host.handleModalKeydown?.(e, () => closeDerivedEditor(host));
}

export function renderDerivedEditor(host: DerivedEditorHost) {
  const st = host.derivedEditor;
  if (!st) return nothing;

  return html`
    <div class="modal-overlay derived-drawer-overlay" role="dialog" aria-modal="true"
      aria-label="派生属性编辑器"
      @click=${() => closeDerivedEditor(host)}
      @keydown=${(e: KeyboardEvent) => onDrawerKeydown(host, e)}>
      <div class="derived-drawer" @click=${(e: Event) => e.stopPropagation()}>
        <div class="derived-drawer__header">
          <span>${st.mode === "bind" ? "绑定表达式" : st.mode === "edit" ? "编辑派生属性" : "新增派生属性"}</span>
          <button class="btn btn--icon" aria-label="关闭" @click=${() => closeDerivedEditor(host)}>×</button>
        </div>
        <div class="derived-drawer__body">
          ${st.mode === "bind" ? html`
            <div class="derived-drawer__hint">已有属性就地升级为派生：属性名与单位保持不变，只绑定表达式。</div>
          ` : nothing}
          <label class="derived-drawer__field">
            <span>属性名</span>
            <input type="text" .value=${st.name} placeholder="如 avg_power" ?readonly=${st.mode === "bind"}
              @input=${(e: Event) => setState(host, { name: (e.target as HTMLInputElement).value })} />
          </label>
          <label class="derived-drawer__field">
            <span>显示名</span>
            <input type="text" .value=${st.displayName} placeholder="如 平均功率" ?readonly=${st.mode === "bind"}
              @input=${(e: Event) => setState(host, { displayName: (e.target as HTMLInputElement).value })} />
          </label>
          <label class="derived-drawer__field">
            <span>单位</span>
            <input type="text" .value=${st.unit} placeholder="如 kW" ?readonly=${st.mode === "bind"}
              @input=${(e: Event) => setState(host, { unit: (e.target as HTMLInputElement).value })} />
          </label>

          <div class="derived-drawer__field">
            <span>表达式</span>
            <textarea class="derived-drawer__expression" readonly rows="3"
              aria-label="表达式预览" .value=${st.expression}></textarea>
            <div class="derived-drawer__toolbar">
              <button type="button" class="btn btn--ghost btn--sm"
                @click=${() => setState(host, { pickerOpen: !st.pickerOpen })}>
                ${st.pickerOpen ? "收起引用选择" : "+ 添加引用"}
              </button>
              ${OPERATORS.map(op => html`
                <button type="button" class="btn btn--ghost btn--sm derived-drawer__op"
                  @click=${() => appendDerivedExpression(host, op.insert)}>${op.label}</button>
              `)}
              <button type="button" class="btn btn--ghost btn--sm"
                @click=${() => appendDerivedExpression(host, "descendant_count()")}>descendant_count()</button>
            </div>
            ${st.saveError ? html`<div class="derived-editor__error" role="alert">${st.saveError}</div>` : nothing}
          </div>

          ${st.pickerOpen ? html`
            <div class="derived-drawer__picker" ${ref(el => {
              if (el && !(el as any)._pickerAttached) {
                (el as any)._pickerAttached = true;
                renderThingPicker(el as HTMLElement, {
                  onPick: r => appendDerivedExpression(host, `{${r.thingName}.${r.propertyName}}`),
                  filterNumericProps: true,
                });
              }
            })}></div>
          ` : nothing}
        </div>
        <div class="derived-drawer__footer">
          <button type="button" class="btn btn--ghost" ?disabled=${st.saving}
            @click=${() => closeDerivedEditor(host)}>取消</button>
          <button type="button" class="btn btn--primary" ?disabled=${st.saving}
            @click=${() => void saveDerivedEditor(host)}>${st.saving ? "保存中…" : "保存"}</button>
        </div>
      </div>
    </div>
  `;
}
