import { LitElement, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { getWorkspaceId } from '../../api/client.js';
import {
  instantiateHarnessWorkflowTemplate,
  listHarnessWorkflowTemplates,
} from '../../api/harness.js';
import type {
  HarnessWorkflowTemplate,
  WorkflowRevision,
  WorkflowTrigger,
} from '../../types/harness.js';
import {
  currentTimeZone,
  formatHarnessTime,
  nextAnnouncement,
  renderHarnessStatus,
} from './harness-status.js';
import './harness-templates.css';

const EMPTY_DEVICE_SELECTOR = '{\n  "device_ids": []\n}';

type TemplateField = 'deviceSelector' | 'sourceSelector' | 'approvalPolicy';

const FIELD_ERROR_TEXT: Record<TemplateField, string> = {
  deviceSelector: '设备选择器不是有效 JSON',
  sourceSelector: 'Source 选择器不是有效 JSON',
  approvalPolicy: '审批策略不是有效 JSON',
};

function errorMessage(error: unknown): string {
  return error instanceof Error && error.message ? error.message : '请求失败';
}

function triggerLabel(trigger: WorkflowTrigger): string {
  if (trigger.type === 'cron') return `定时 ${trigger.expression}`;
  if (trigger.type === 'watch') return '设备观察';
  return '事件驱动';
}

function jsonText(value: unknown): string {
  return JSON.stringify(value ?? {}, null, 2);
}

function parseObject(
  value: string,
  field: TemplateField,
  errors: Partial<Record<TemplateField, string>>,
): Record<string, unknown> | null {
  let parsed: unknown;
  try {
    parsed = value.trim() ? JSON.parse(value) : {};
  } catch {
    errors[field] = FIELD_ERROR_TEXT[field];
    return null;
  }
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    errors[field] = FIELD_ERROR_TEXT[field];
    return null;
  }
  return parsed as Record<string, unknown>;
}

function deviceIds(selector: Record<string, unknown>): string[] {
  const raw = selector.device_ids ?? selector.deviceIds;
  return Array.isArray(raw)
    ? raw.filter((value): value is string => typeof value === 'string' && value.trim() !== '')
    : [];
}

@customElement('view-harness-templates')
export class HarnessTemplatesView extends LitElement {
  @property() workspaceId = getWorkspaceId() ?? '';

  @state() private templates: HarnessWorkflowTemplate[] = [];
  @state() private selectedId = '';
  @state() private deviceSelector = EMPTY_DEVICE_SELECTOR;
  @state() private sourceSelector = '{}';
  @state() private approvalPolicy = '{}';
  @state() private fieldErrors: Partial<Record<TemplateField, string>> = {};
  @state() private loading = false;
  @state() private loadError = '';
  @state() private submitting = false;
  @state() private submitError = '';
  @state() private result: WorkflowRevision | null = null;
  @state() private announcement = '';

  private loadGeneration = 0;

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    void this.load();
  }

  disconnectedCallback(): void {
    this.loadGeneration += 1;
    super.disconnectedCallback();
  }

  private async load(): Promise<void> {
    const generation = ++this.loadGeneration;
    this.loading = true;
    this.loadError = '';
    try {
      const response = await listHarnessWorkflowTemplates(this.workspaceId);
      if (generation !== this.loadGeneration) return;
      this.templates = response.result?.templates ?? [];
    } catch (error) {
      if (generation !== this.loadGeneration) return;
      this.loadError = errorMessage(error);
    } finally {
      if (generation === this.loadGeneration) this.loading = false;
    }
  }

  private select(template: HarnessWorkflowTemplate): void {
    this.selectedId = template.id;
    this.deviceSelector = EMPTY_DEVICE_SELECTOR;
    this.sourceSelector = jsonText(template.definition.sourceSelector);
    this.approvalPolicy = '{}';
    this.fieldErrors = {};
    this.submitError = '';
    this.result = null;
    this.announcement = nextAnnouncement(this.announcement, `已选择${template.title}模板`);
  }

  private selected(): HarnessWorkflowTemplate | null {
    return this.templates.find((template) => template.id === this.selectedId) ?? null;
  }

  private async submit(event: Event): Promise<void> {
    event.preventDefault();
    const template = this.selected();
    if (!template || this.submitting) return;

    const errors: Partial<Record<TemplateField, string>> = {};
    const device = parseObject(this.deviceSelector, 'deviceSelector', errors);
    const source = parseObject(this.sourceSelector, 'sourceSelector', errors);
    const approval = parseObject(this.approvalPolicy, 'approvalPolicy', errors);
    if (device && deviceIds(device).length === 0) {
      errors.deviceSelector = '请至少选择一个设备（至少一个设备 ID）';
    }
    this.fieldErrors = errors;
    if (Object.keys(errors).length > 0 || !device || !source || !approval) return;

    this.submitting = true;
    this.submitError = '';
    this.result = null;
    try {
      const response = await instantiateHarnessWorkflowTemplate(this.workspaceId, template.id, {
        workflowId: '',
        deviceSelector: device,
        sourceSelector: source,
        approvalPolicy: approval,
      });
      const revision = response.result;
      if (!revision) throw new Error('创建结果缺失');
      this.result = revision;
      this.announcement = nextAnnouncement(
        this.announcement,
        `已从${template.title}模板创建并启用 Workflow，版本 ${revision.version}`,
      );
    } catch (error) {
      this.submitError = errorMessage(error);
    } finally {
      this.submitting = false;
    }
  }

  private renderTemplate(template: HarnessWorkflowTemplate) {
    const selected = template.id === this.selectedId;
    const definition = template.definition;
    return html`
      <li>
        <button
          type="button"
          class="harness-templates__card ${selected ? 'is-selected' : ''}"
          data-template-id=${template.id}
          aria-pressed=${selected ? 'true' : 'false'}
          @click=${() => this.select(template)}
        >
          <span class="harness-templates__card-head">
            <span class="harness-templates__card-title">${template.title}</span>
            <span class="harness-templates__version">v${template.version}</span>
          </span>
          <span class="harness-templates__card-description">${template.description}</span>
          <span class="harness-templates__meta">触发：${triggerLabel(definition.trigger)}</span>
          <span class="harness-templates__meta">
            工具：${definition.toolIds.join('、') || '无'}
          </span>
          <span class="harness-templates__meta">
            所需能力：${definition.requiredCapabilities.join('、') || '无'}
          </span>
        </button>
      </li>
    `;
  }

  private renderField(
    field: TemplateField,
    id: string,
    label: string,
    value: string,
    onInput: (value: string) => void,
  ) {
    const error = this.fieldErrors[field];
    return html`
      <div class="harness-templates__field">
        <label for=${id}>${label}</label>
        <textarea
          id=${id}
          name=${id}
          rows="4"
          spellcheck="false"
          aria-describedby=${error ? `${id}-error` : nothing}
          aria-invalid=${error ? 'true' : 'false'}
          .value=${value}
          @input=${(event: Event) => onInput((event.target as HTMLTextAreaElement).value)}
        ></textarea>
        ${
          error
            ? html`<p
                class="harness-templates__field-error"
                id=${`${id}-error`}
                data-field-error=${field}
                role="alert"
              >
                ${error}
              </p>`
            : nothing
        }
      </div>
    `;
  }

  private renderForm(template: HarnessWorkflowTemplate) {
    const definition = template.definition;
    return html`
      <form class="harness-templates__form" @submit=${this.submit}>
        <header>
          <h2>${template.title}参数</h2>
          <p>
            模板版本 v${template.version}；Skills：${definition.skillRefs.join('、') || '无'}；
            证据：${definition.evidencePolicy ? '已声明' : '未声明'}。
          </p>
        </header>
        ${this.renderField(
          'deviceSelector',
          'template-device-selector',
          '设备选择器（JSON，device_ids）',
          this.deviceSelector,
          (value) => {
            this.deviceSelector = value;
          },
        )}
        ${this.renderField(
          'sourceSelector',
          'template-source-selector',
          'Source 选择器（JSON）',
          this.sourceSelector,
          (value) => {
            this.sourceSelector = value;
          },
        )}
        ${this.renderField(
          'approvalPolicy',
          'template-approval-policy',
          '审批策略（JSON，只能收紧）',
          this.approvalPolicy,
          (value) => {
            this.approvalPolicy = value;
          },
        )}
        <div class="harness-templates__actions">
          <button type="submit" class="btn btn--primary" data-action="instantiate" ?disabled=${this.submitting}>
            ${this.submitting ? '正在创建…' : '创建并启用'}
          </button>
          <a class="btn" href="#/harness/workflows/new">改为空白配置</a>
        </div>
      </form>
    `;
  }

  private renderResult() {
    if (!this.result) return nothing;
    const revision = this.result;
    return html`
      <section
        class="harness-templates__result"
        data-template-result
        aria-labelledby="harness-template-result-title"
      >
        <h2 id="harness-template-result-title">已创建并启用</h2>
        <p class="harness-templates__result-state">
          状态：${renderHarnessStatus(revision.enabled ? 'enabled' : 'disabled')}
          <span>版本 ${revision.version}</span>
        </p>
        <p>
          下一次计划 wake：${formatHarnessTime(revision.nextWakeAt ?? null)}
          <span class="harness-templates__meta">（${currentTimeZone()}）</span>
        </p>
        <p class="harness-templates__meta">
          未来触发只显示计划时间，不会立即运行；过期 slot 不补跑。
        </p>
        <a href=${`#/harness/workflows/${encodeURIComponent(revision.workflowId)}`}>
          查看 Workflow 详情
        </a>
      </section>
    `;
  }

  render() {
    return html`
      <section class="harness-templates" aria-labelledby="harness-templates-title">
        <a class="harness-templates__back" href="#/harness">← 返回 Harness 工作台</a>
        <header class="harness-templates__header">
          <h1 id="harness-templates-title">从模板新建</h1>
          <p>
            持续 Workflow 会按计划唤醒 Agent 并留下证据。选择一份版本化模板，填写设备、
            数据源和策略后直接创建启用配置。
          </p>
        </header>
        <p class="harness-sr-only" role="status" aria-live="polite">${this.announcement}</p>
        ${
          this.loadError
            ? html`<div class="harness-templates__error" role="alert">
                <p>加载模板失败：${this.loadError}</p>
                <button type="button" class="btn" data-action="retry" @click=${() => this.load()}>
                  重试
                </button>
              </div>`
            : nothing
        }
        ${this.loading ? html`<p class="harness-templates__status" role="status">正在加载模板…</p>` : nothing}
        ${
          this.templates.length > 0
            ? html`
                <ul class="harness-templates__list">
                  ${this.templates.map((template) => this.renderTemplate(template))}
                </ul>
                <p class="harness-templates__hint">
                  模板只包含统一 WorkflowDefinition 数据；失败校验不会创建配置或调度。
                </p>
              `
            : nothing
        }
        <aside class="harness-templates__dry-run" data-dry-run-optional>
          <strong>可选 dry-run：</strong>
          dry-run 是独立旁路，不是启用的前置步骤；创建启用不依赖 dry-run。
        </aside>
        ${this.submitError
          ? html`<p class="harness-templates__error" role="alert">创建失败：${this.submitError}</p>`
          : nothing}
        ${this.selected() ? this.renderForm(this.selected()!) : nothing} ${this.renderResult()}
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-harness-templates': HarnessTemplatesView;
  }
}
