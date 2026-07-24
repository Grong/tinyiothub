/**
 * 模板三段编辑器 (T20)
 *
 * D5 design: 属性 | 事件 | 动作 三标签页表单编辑器
 * - 全宽表格，行内编辑
 * - 验证错误时标签页显示红点
 * - 编辑事件字段时显示可引用属性摘要栏
 */

import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { SignalWatcher } from "@lit-labs/signals";
import { templateApi } from "../../api/templates.js";
import { success, error as toastError } from "../components/toast.js";

// ──────────────────────────────────────────────
// Local editor types
// ──────────────────────────────────────────────

interface PropertyRow {
  key: string; // local editing key
  name: string;
  dataType: "string" | "number" | "boolean" | "object";
  unit: string;
  readOnly: boolean;
  description: string;
}

interface EventField {
  key: string;
  name: string;
  dataType: string;
  description: string;
}

interface EventRow {
  key: string;
  name: string;
  level: "info" | "warning" | "error" | "critical";
  fields: EventField[];
  description: string;
}

interface ActionParam {
  key: string;
  name: string;
  dataType: string;
  required: boolean;
  description: string;
}

interface ActionRow {
  key: string;
  name: string;
  parameters: ActionParam[];
  description: string;
}

type TabKey = "properties" | "events" | "actions";

const DATA_TYPE_OPTIONS = ["string", "number", "boolean", "object"] as const;
const EVENT_LEVEL_OPTIONS = ["info", "warning", "error", "critical"] as const;

let _nextKey = 1;
function nextKey(): string {
  return `k_${_nextKey++}`;
}

// ──────────────────────────────────────────────
// Helpers to parse JSON string fields from API
// ──────────────────────────────────────────────

function parseJson<T>(v: unknown, fallback: T): T {
  if (!v) return fallback;
  if (typeof v === "string") {
    try {
      return JSON.parse(v) as T;
    } catch {
      return fallback;
    }
  }
  return v as unknown as T;
}

function mapPropertyRow(raw: any): PropertyRow {
  return {
    key: nextKey(),
    name: raw.name || "",
    dataType: raw.dataType || raw.data_type || "string",
    unit: raw.unit || "",
    readOnly: !!(raw.readOnly ?? raw.read_only ?? raw.isReadOnly ?? raw.is_read_only),
    description:
      typeof raw.description === "object" && raw.description !== null
        ? raw.description.zh || raw.description.en || ""
        : raw.description || "",
  };
}

function mapEventRow(raw: any): EventRow {
  return {
    key: nextKey(),
    name: raw.name || "",
    level: raw.level || "info",
    fields: Array.isArray(raw.fields) ? raw.fields.map((f: any) => ({
      key: nextKey(),
      name: f.name || "",
      dataType: f.dataType || f.data_type || "string",
      description:
        typeof f.description === "object" && f.description !== null
          ? f.description.zh || f.description.en || ""
          : f.description || "",
    })) : [],
    description:
      typeof raw.description === "object" && raw.description !== null
        ? raw.description.zh || raw.description.en || ""
        : raw.description || "",
  };
}

function mapActionRow(raw: any): ActionRow {
  return {
    key: nextKey(),
    name: raw.name || "",
    parameters: Array.isArray(raw.parameters) ? raw.parameters.map((p: any) => ({
      key: nextKey(),
      name: p.name || "",
      dataType: p.dataType || p.data_type || "string",
      required: !!(p.required ?? p.isRequired ?? p.is_required),
      description:
        typeof p.description === "object" && p.description !== null
          ? p.description.zh || p.description.en || ""
          : p.description || "",
    })) : [],
    description:
      typeof raw.description === "object" && raw.description !== null
        ? raw.description.zh || raw.description.en || ""
        : raw.description || "",
  };
}

// ──────────────────────────────────────────────
// Serialize for save
// ──────────────────────────────────────────────

function serializeProperty(p: PropertyRow) {
  return {
    name: p.name,
    data_type: p.dataType,
    unit: p.unit || null,
    is_read_only: p.readOnly,
    description: p.description ? { zh: p.description } : null,
  };
}

function serializeEvent(e: EventRow) {
  return {
    name: e.name,
    level: e.level,
    fields: e.fields
      .filter((f) => f.name.trim() !== "")
      .map((f) => ({
        name: f.name,
        data_type: f.dataType,
        description: f.description ? { zh: f.description } : null,
      })),
    description: e.description ? { zh: e.description } : null,
  };
}

function serializeAction(a: ActionRow) {
  return {
    name: a.name,
    parameters: a.parameters
      .filter((p) => p.name.trim() !== "")
      .map((p) => ({
        name: p.name,
        data_type: p.dataType,
        required: p.required,
        description: p.description ? { zh: p.description } : null,
      })),
    description: a.description ? { zh: a.description } : null,
  };
}

// ──────────────────────────────────────────────
// Validation helpers
// ──────────────────────────────────────────────

interface ValidationIssue {
  segment: TabKey;
  rowKey: string;
  field: string;
  message: string;
}

function validate(
  properties: PropertyRow[],
  events: EventRow[],
  actions: ActionRow[],
): ValidationIssue[] {
  const issues: ValidationIssue[] = [];

  for (const p of properties) {
    if (!p.name.trim()) {
      issues.push({ segment: "properties", rowKey: p.key, field: "name", message: "属性名不能为空" });
    }
  }

  for (const e of events) {
    if (!e.name.trim()) {
      issues.push({ segment: "events", rowKey: e.key, field: "name", message: "事件名不能为空" });
    }
    for (const f of e.fields) {
      if (!f.name.trim()) {
        issues.push({ segment: "events", rowKey: e.key, field: "fields", message: "字段名不能为空" });
      }
    }
  }

  for (const a of actions) {
    if (!a.name.trim()) {
      issues.push({ segment: "actions", rowKey: a.key, field: "name", message: "动作名不能为空" });
    }
    for (const p of a.parameters) {
      if (!p.name.trim()) {
        issues.push({ segment: "actions", rowKey: a.key, field: "parameters", message: "参数名不能为空" });
      }
    }
  }

  return issues;
}

function hasSegmentErrors(issues: ValidationIssue[], segment: TabKey): boolean {
  return issues.some((i) => i.segment === segment);
}

// ──────────────────────────────────────────────
// Component
// ──────────────────────────────────────────────

@customElement("view-template-editor")
export class TemplateEditorView extends SignalWatcher(LitElement) {
  @state() loading = true;
  @state() error = "";
  @state() saving = false;

  // Template meta
  @state() templateId = "";
  @state() templateName = "";

  // Active tab
  @state() activeTab: TabKey = "properties";

  // Segments (editable state)
  @state() properties: PropertyRow[] = [];
  @state() events: EventRow[] = [];
  @state() actions: ActionRow[] = [];


  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    // Read template ID from URL hash or query
    const params = new URLSearchParams(window.location.search);
    const id = params.get("id") || "";
    if (id) {
      this.templateId = id;
      this.loadTemplate(id);
    } else {
      this.loading = false;
      this.error = "未指定模板 ID";
    }
  }

  // ──────────────────────────────────────────────
  // Data loading
  // ──────────────────────────────────────────────

  async loadTemplate(id: string) {
    this.loading = true;
    this.error = "";
    try {
      const res = await templateApi.getTemplate(id);
      const tpl = res.result;
      if (!tpl) {
        this.error = "模板不存在";
        this.loading = false;
        return;
      }

      this.templateName = tpl.name || "";

      // Parse JSON fields — handle both old (commands) and new (actions) field names
      const rawProperties = parseJson<any[]>(tpl.properties, []);
      const tplAny = tpl as Record<string, any>;
      const rawActions = parseJson<any[]>(tplAny.actions || tplAny.commands, []);
      const rawEvents = parseJson<any[]>(tplAny.events, []);

      this.properties = rawProperties.map(mapPropertyRow);
      this.actions = rawActions.map(mapActionRow);
      this.events = rawEvents.map(mapEventRow);

      this.loading = false;
    } catch (err: any) {
      this.error = err?.message || "加载模板失败";
      this.loading = false;
    }
  }

  // ──────────────────────────────────────────────
  // Save
  // ──────────────────────────────────────────────

  async handleSave() {
    if (this.saving) return;

    const issues = validate(this.properties, this.events, this.actions);
    if (issues.length > 0) {
      // Show the first tab with errors
      const firstIssue = issues[0];
      this.activeTab = firstIssue.segment;
      toastError(`请修复校验错误后再保存（${issues.length} 个问题）`);
      return;
    }

    this.saving = true;
    try {
      const body: Record<string, any> = {
        properties: this.properties.map(serializeProperty),
        commands: this.actions.map(serializeAction),
        events: this.events.map(serializeEvent),
      };

      await templateApi.updateTemplate(this.templateId, body);
      success("模板已保存");
    } catch (err: any) {
      toastError(err?.message || "保存失败");
    } finally {
      this.saving = false;
    }
  }

  // ──────────────────────────────────────────────
  // Helpers
  // ──────────────────────────────────────────────

  private propertyNames(): string[] {
    return this.properties.filter((p) => p.name.trim()).map((p) => p.name);
  }

  // ──────────────────────────────────────────────
  // Row mutations
  // ──────────────────────────────────────────────

  private addProperty() {
    this.properties = [
      ...this.properties,
      { key: nextKey(), name: "", dataType: "string", unit: "", readOnly: false, description: "" },
    ];
  }

  private removeProperty(key: string) {
    this.properties = this.properties.filter((p) => p.key !== key);
  }

  private updateProperty(key: string, patch: Partial<PropertyRow>) {
    this.properties = this.properties.map((p) => (p.key === key ? { ...p, ...patch } : p));
  }

  private addEvent() {
    this.events = [
      ...this.events,
      { key: nextKey(), name: "", level: "info", fields: [], description: "" },
    ];
  }

  private removeEvent(key: string) {
    this.events = this.events.filter((e) => e.key !== key);
  }

  private updateEvent(key: string, patch: Partial<EventRow>) {
    this.events = this.events.map((e) => (e.key === key ? { ...e, ...patch } : e));
  }

  private addEventField(eventKey: string) {
    this.events = this.events.map((e) =>
      e.key === eventKey
        ? {
            ...e,
            fields: [
              ...e.fields,
              { key: nextKey(), name: "", dataType: "string", description: "" },
            ],
          }
        : e,
    );
  }

  private removeEventField(eventKey: string, fieldKey: string) {
    this.events = this.events.map((e) =>
      e.key === eventKey
        ? { ...e, fields: e.fields.filter((f) => f.key !== fieldKey) }
        : e,
    );
  }

  private updateEventField(eventKey: string, fieldKey: string, patch: Partial<EventField>) {
    this.events = this.events.map((e) =>
      e.key === eventKey
        ? {
            ...e,
            fields: e.fields.map((f) => (f.key === fieldKey ? { ...f, ...patch } : f)),
          }
        : e,
    );
  }

  private addAction() {
    this.actions = [
      ...this.actions,
      { key: nextKey(), name: "", parameters: [], description: "" },
    ];
  }

  private removeAction(key: string) {
    this.actions = this.actions.filter((a) => a.key !== key);
  }

  private updateAction(key: string, patch: Partial<ActionRow>) {
    this.actions = this.actions.map((a) => (a.key === key ? { ...a, ...patch } : a));
  }

  private addActionParam(actionKey: string) {
    this.actions = this.actions.map((a) =>
      a.key === actionKey
        ? {
            ...a,
            parameters: [
              ...a.parameters,
              { key: nextKey(), name: "", dataType: "string", required: false, description: "" },
            ],
          }
        : a,
    );
  }

  private removeActionParam(actionKey: string, paramKey: string) {
    this.actions = this.actions.map((a) =>
      a.key === actionKey
        ? { ...a, parameters: a.parameters.filter((p) => p.key !== paramKey) }
        : a,
    );
  }

  private updateActionParam(actionKey: string, paramKey: string, patch: Partial<ActionParam>) {
    this.actions = this.actions.map((a) =>
      a.key === actionKey
        ? {
            ...a,
            parameters: a.parameters.map((p) => (p.key === paramKey ? { ...p, ...patch } : p)),
          }
        : a,
    );
  }

  // ──────────────────────────────────────────────
  // Render
  // ──────────────────────────────────────────────

  render() {
    if (this.loading) return this._renderSkeleton();
    if (this.error) return this._renderError();

    const issues = validate(this.properties, this.events, this.actions);

    return html`
      <div class="template-editor">
        <!-- Header -->
        <div class="te-header">
          <h2 class="te-title">${this.templateName || "模板编辑器"}</h2>
          <button
            class="btn btn--primary btn--sm"
            ?disabled=${this.saving}
            @click=${() => this.handleSave()}
          >
            ${this.saving ? "保存中…" : "保存"}
          </button>
        </div>

        <!-- Tabs -->
        <div class="te-tabs">
          ${this._renderTab("properties", "属性", issues)}
          ${this._renderTab("events", "事件", issues)}
          ${this._renderTab("actions", "动作", issues)}
        </div>

        <!-- Tab content -->
        <div class="te-content">
          ${this.activeTab === "properties" ? this._renderPropertiesTab() : nothing}
          ${this.activeTab === "events" ? this._renderEventsTab() : nothing}
          ${this.activeTab === "actions" ? this._renderActionsTab() : nothing}
        </div>
      </div>
    `;
  }

  private _renderTab(key: TabKey, label: string, issues: ValidationIssue[]) {
    const hasErrs = hasSegmentErrors(issues, key);
    return html`
      <button
        class="te-tab ${this.activeTab === key ? "active" : ""}"
        @click=${() => { this.activeTab = key; }}
      >
        ${label}
        ${hasErrs ? html`<span class="te-tab-dot"></span>` : nothing}
      </button>
    `;
  }

  // ── Properties Tab ────────────────────────────

  private _renderPropertiesTab() {
    const hasItems = this.properties.length > 0;
    return html`
      ${!hasItems ? this._renderEmpty("属性") : nothing}
      <table class="te-table">
        <thead>
          <tr>
            <th>属性名</th>
            <th>类型</th>
            <th>单位</th>
            <th>只读</th>
            <th>描述</th>
            <th style="width:40px"></th>
          </tr>
        </thead>
        <tbody>
          ${this.properties.map((p) => this._renderPropertyRow(p))}
        </tbody>
      </table>
      <button class="te-add-btn" @click=${() => this.addProperty()}>
        + 添加属性
      </button>
    `;
  }

  private _renderPropertyRow(p: PropertyRow) {
    return html`
      <tr>
        <td>
          <input
            class="te-input"
            type="text"
            .value=${p.name}
            placeholder="属性名"
            @input=${(e: Event) => this.updateProperty(p.key, { name: (e.target as HTMLInputElement).value })}
          />
        </td>
        <td>
          <select
            class="te-select"
            @change=${(e: Event) => this.updateProperty(p.key, { dataType: (e.target as HTMLSelectElement).value as PropertyRow["dataType"] })}
          >
            ${DATA_TYPE_OPTIONS.map(
              (dt) => html`<option value=${dt} ?selected=${p.dataType === dt}>${dt}</option>`,
            )}
          </select>
        </td>
        <td>
          <input
            class="te-input te-input--sm"
            type="text"
            .value=${p.unit}
            placeholder="如 ℃"
            @input=${(e: Event) => this.updateProperty(p.key, { unit: (e.target as HTMLInputElement).value })}
          />
        </td>
        <td style="text-align:center">
          <input
            type="checkbox"
            .checked=${p.readOnly}
            @change=${(e: Event) => this.updateProperty(p.key, { readOnly: (e.target as HTMLInputElement).checked })}
          />
        </td>
        <td>
          <input
            class="te-input"
            type="text"
            .value=${p.description}
            placeholder="描述"
            @input=${(e: Event) => this.updateProperty(p.key, { description: (e.target as HTMLInputElement).value })}
          />
        </td>
        <td>
          <button class="te-remove-btn" @click=${() => this.removeProperty(p.key)} title="删除">×</button>
        </td>
      </tr>
    `;
  }

  // ── Events Tab ────────────────────────────────

  private _renderEventsTab() {
    const hasItems = this.events.length > 0;
    const propNames = this.propertyNames();
    return html`
      ${!hasItems ? this._renderEmpty("事件") : nothing}
      ${propNames.length > 0
        ? html`
            <div class="te-refbar">
              <span class="te-refbar-icon">&#9432;</span>
              可引用的属性: <strong>${propNames.join(", ")}</strong>
            </div>
          `
        : nothing}
      ${this.events.map((e) => this._renderEventCard(e))}
      <button class="te-add-btn" @click=${() => this.addEvent()}>
        + 添加事件
      </button>
    `;
  }

  private _renderEventCard(e: EventRow) {
    return html`
      <div class="te-card">
        <div class="te-card-header">
          <input
            class="te-input te-card-name"
            type="text"
            .value=${e.name}
            placeholder="事件名"
            @input=${(ev: Event) => this.updateEvent(e.key, { name: (ev.target as HTMLInputElement).value })}
          />
          <select
            class="te-select te-select--sm"
            @change=${(ev: Event) =>
              this.updateEvent(e.key, { level: (ev.target as HTMLSelectElement).value as EventRow["level"] })}
          >
            ${EVENT_LEVEL_OPTIONS.map(
              (l) => html`<option value=${l} ?selected=${e.level === l}>${l}</option>`,
            )}
          </select>
          <button class="te-remove-btn" @click=${() => this.removeEvent(e.key)} title="删除事件">×</button>
        </div>

        <!-- Fields sub-table -->
        <div class="te-subsection">
          <div class="te-subsection-label">字段</div>
          <table class="te-table te-table--sub">
            <thead>
              <tr>
                <th>字段名</th>
                <th>类型</th>
                <th>描述</th>
                <th style="width:40px"></th>
              </tr>
            </thead>
            <tbody>
              ${e.fields.map((f) => this._renderEventFieldRow(e.key, f))}
            </tbody>
          </table>
          <button class="te-add-btn te-add-btn--sm" @click=${() => this.addEventField(e.key)}>
            + 添加字段
          </button>
        </div>
      </div>
    `;
  }

  private _renderEventFieldRow(eventKey: string, f: EventField) {
    return html`
      <tr>
        <td>
          <input
            class="te-input te-input--sm"
            type="text"
            .value=${f.name}
            placeholder="字段名"
            @input=${(ev: Event) => this.updateEventField(eventKey, f.key, { name: (ev.target as HTMLInputElement).value })}
          />
        </td>
        <td>
          <select
            class="te-select te-select--xs"
            @change=${(ev: Event) =>
              this.updateEventField(eventKey, f.key, { dataType: (ev.target as HTMLSelectElement).value })}
          >
            ${DATA_TYPE_OPTIONS.map(
              (dt) => html`<option value=${dt} ?selected=${f.dataType === dt}>${dt}</option>`,
            )}
          </select>
        </td>
        <td>
          <input
            class="te-input te-input--sm"
            type="text"
            .value=${f.description}
            placeholder="描述"
            @input=${(ev: Event) => this.updateEventField(eventKey, f.key, { description: (ev.target as HTMLInputElement).value })}
          />
        </td>
        <td>
          <button class="te-remove-btn" @click=${() => this.removeEventField(eventKey, f.key)} title="删除字段">×</button>
        </td>
      </tr>
    `;
  }

  // ── Actions Tab ───────────────────────────────

  private _renderActionsTab() {
    const hasItems = this.actions.length > 0;
    return html`
      ${!hasItems ? this._renderEmpty("动作") : nothing}
      ${this.actions.map((a) => this._renderActionCard(a))}
      <button class="te-add-btn" @click=${() => this.addAction()}>
        + 添加动作
      </button>
    `;
  }

  private _renderActionCard(a: ActionRow) {
    return html`
      <div class="te-card">
        <div class="te-card-header">
          <input
            class="te-input te-card-name"
            type="text"
            .value=${a.name}
            placeholder="动作名"
            @input=${(ev: Event) => this.updateAction(a.key, { name: (ev.target as HTMLInputElement).value })}
          />
          <button class="te-remove-btn" @click=${() => this.removeAction(a.key)} title="删除动作">×</button>
        </div>

        <!-- Parameters sub-table -->
        <div class="te-subsection">
          <div class="te-subsection-label">参数</div>
          <table class="te-table te-table--sub">
            <thead>
              <tr>
                <th>参数名</th>
                <th>类型</th>
                <th>必填</th>
                <th>描述</th>
                <th style="width:40px"></th>
              </tr>
            </thead>
            <tbody>
              ${a.parameters.map((p) => this._renderActionParamRow(a.key, p))}
            </tbody>
          </table>
          <button class="te-add-btn te-add-btn--sm" @click=${() => this.addActionParam(a.key)}>
            + 添加参数
          </button>
        </div>
      </div>
    `;
  }

  private _renderActionParamRow(actionKey: string, p: ActionParam) {
    return html`
      <tr>
        <td>
          <input
            class="te-input te-input--sm"
            type="text"
            .value=${p.name}
            placeholder="参数名"
            @input=${(ev: Event) => this.updateActionParam(actionKey, p.key, { name: (ev.target as HTMLInputElement).value })}
          />
        </td>
        <td>
          <select
            class="te-select te-select--xs"
            @change=${(ev: Event) =>
              this.updateActionParam(actionKey, p.key, { dataType: (ev.target as HTMLSelectElement).value })}
          >
            ${DATA_TYPE_OPTIONS.map(
              (dt) => html`<option value=${dt} ?selected=${p.dataType === dt}>${dt}</option>`,
            )}
          </select>
        </td>
        <td style="text-align:center">
          <input
            type="checkbox"
            .checked=${p.required}
            @change=${(ev: Event) => this.updateActionParam(actionKey, p.key, { required: (ev.target as HTMLInputElement).checked })}
          />
        </td>
        <td>
          <input
            class="te-input te-input--sm"
            type="text"
            .value=${p.description}
            placeholder="描述"
            @input=${(ev: Event) => this.updateActionParam(actionKey, p.key, { description: (ev.target as HTMLInputElement).value })}
          />
        </td>
        <td>
          <button class="te-remove-btn" @click=${() => this.removeActionParam(actionKey, p.key)} title="删除参数">×</button>
        </td>
      </tr>
    `;
  }

  // ── Shared sub-views ──────────────────────────

  private _renderEmpty(label: string) {
    return html`
      <div class="te-empty">
        <div class="te-empty-text">暂无${label}定义</div>
        <button class="btn btn--primary btn--sm" @click=${() => {
          if (label === "属性") this.addProperty();
          else if (label === "事件") this.addEvent();
          else if (label === "动作") this.addAction();
        }}>
          添加第一个${label}
        </button>
      </div>
    `;
  }

  private _renderSkeleton() {
    return html`
      <div class="te-skeleton">
        <div class="te-sk-header"></div>
        <div class="te-sk-tabs">
          <div class="te-sk-tab"></div>
          <div class="te-sk-tab"></div>
          <div class="te-sk-tab"></div>
        </div>
        <div class="te-sk-rows">
          <div class="te-sk-row"></div>
          <div class="te-sk-row"></div>
          <div class="te-sk-row"></div>
        </div>
      </div>
    `;
  }

  private _renderError() {
    return html`
      <div class="te-error">
        <p>${this.error}</p>
        <button class="btn btn--primary btn--sm" @click=${() => this.loadTemplate(this.templateId)}>
          重试
        </button>
      </div>
    `;
  }
}
