import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError, getWorkspaceId } from '../../api/client.js';
import {
  createHarnessWorkflow,
  getHarnessWorkflow,
  setHarnessWorkflowEnabled,
  updateHarnessWorkflow,
} from '../../api/harness.js';
import type {
  HarnessApiError,
  WorkflowDefinition,
  WorkflowRevision,
  WorkflowTrigger,
} from '../../types/harness.js';
import { createDialogFocusTrap, type DialogFocusTrap } from './runtime-dialog-focus.js';
import {
  currentTimeZone,
  formatHarnessTime,
  nextAnnouncement,
  renderHarnessStatus,
} from './harness-status.js';
import './harness-workflow-edit.css';

/** Fixed conflict copy (DR3=A). No automatic overwrite and no diff. */
export const WORKFLOW_CONFLICT_MESSAGE = '配置已更新；重新加载后再试';

const GROUP_ORDER = ['objective', 'trigger', 'tools', 'policy', 'summary'] as const;
type GroupId = (typeof GROUP_ORDER)[number];

const GROUP_LABELS: Record<GroupId, string> = {
  objective: '目标与设备',
  trigger: '触发',
  tools: '工具与 Skills',
  policy: '策略与证据',
  summary: '配置摘要',
};

const TRIGGER_TYPES = ['cron', 'watch', 'event'] as const;
type TriggerType = (typeof TRIGGER_TYPES)[number];

const TRIGGER_LABELS: Record<TriggerType, string> = {
  cron: 'Cron',
  watch: 'Watch',
  event: 'Event',
};

const FIELD_LABELS: Record<string, string> = {
  objective: '目标',
  'trigger.triggerId': '触发 ID',
  'trigger.expression': 'Cron 表达式',
  deviceSelector: '设备选择器',
  sourceSelector: 'Source 选择器',
  toolIds: '工具 ID',
  skillRefs: 'Skills',
  requiredCapabilities: '所需能力',
  approvalPolicy: '审批策略',
  evidencePolicy: '证据策略',
  _form: '配置',
};

const FIELD_INPUT_IDS: Record<string, string> = {
  objective: 'workflow-objective',
  'trigger.triggerId': 'workflow-trigger-id',
  'trigger.expression': 'workflow-cron-expression',
  deviceSelector: 'workflow-device-selector',
  sourceSelector: 'workflow-source-selector',
  toolIds: 'workflow-tool-ids',
  skillRefs: 'workflow-skill-refs',
  requiredCapabilities: 'workflow-required-capabilities',
  approvalPolicy: 'workflow-approval-policy',
  evidencePolicy: 'workflow-evidence-policy',
};

interface EditorForm {
  objective: string;
  deviceSelector: string;
  sourceSelector: string;
  triggerType: TriggerType;
  triggerId: string;
  cronExpression: string;
  toolIds: string;
  skillRefs: string;
  requiredCapabilities: string;
  approvalPolicy: string;
  evidencePolicy: string;
}

interface EditorFeedback {
  kind: 'pending' | 'success' | 'conflict' | 'error';
  title: string;
  detail: string;
  next: string;
}

type EditorDialog = { kind: 'leave' } | { kind: 'toggle' };

const TRIGGER_FIELD_MAP: Record<string, string> = {
  triggerId: 'trigger.triggerId',
  trigger_id: 'trigger.triggerId',
  trigger: 'trigger.triggerId',
  expression: 'trigger.expression',
  cron: 'trigger.expression',
  'trigger.expression': 'trigger.expression',
  'trigger.trigger_id': 'trigger.triggerId',
  deviceSelector: 'deviceSelector',
  device_selector: 'deviceSelector',
  sourceSelector: 'sourceSelector',
  source_selector: 'sourceSelector',
  toolIds: 'toolIds',
  tool_ids: 'toolIds',
  skillRefs: 'skillRefs',
  skill_refs: 'skillRefs',
  requiredCapabilities: 'requiredCapabilities',
  required_capabilities: 'requiredCapabilities',
  approvalPolicy: 'approvalPolicy',
  approval_policy: 'approvalPolicy',
  evidencePolicy: 'evidencePolicy',
  evidence_policy: 'evidencePolicy',
  objective: 'objective',
};

const DEFAULT_FIELD_MESSAGES: Record<string, string> = {
  objective: '目标不能为空',
  'trigger.triggerId': '触发 ID 不能为空',
  'trigger.expression': 'Cron 表达式无效',
  deviceSelector: '设备选择器不是有效 JSON',
  sourceSelector: 'Source 选择器不是有效 JSON',
  toolIds: '工具 ID 无效',
  skillRefs: 'Skills 无效',
  requiredCapabilities: '所需能力无效',
  approvalPolicy: '审批策略不是有效 JSON',
  evidencePolicy: '证据策略不是有效 JSON',
  _form: '保存失败',
};

function objectValue(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function messageText(value: unknown): string | null {
  if (typeof value === 'string') return value.trim() || null;
  const record = objectValue(value);
  if (!record) return null;
  for (const key of ['message', 'msg', 'detail', 'error']) {
    const candidate = record[key];
    if (typeof candidate === 'string' && candidate.trim()) return candidate;
  }
  return null;
}

function normalizeFieldKey(rawField: string): string {
  const field = rawField.replace(/^definition\./, '');
  return TRIGGER_FIELD_MAP[field] ?? field;
}

function conflictVersion(error: HarnessApiError): number | null {
  const data = objectValue(error.data);
  const result = objectValue(data?.result);
  const details = objectValue(data?.details);
  const candidates = [
    result?.currentVersion,
    result?.current_version,
    details?.currentVersion,
    details?.current_version,
    data?.currentVersion,
    data?.current_version,
  ];
  for (const candidate of candidates) {
    if (typeof candidate === 'number') return candidate;
  }
  return null;
}

function isConflict(error: unknown): error is HarnessApiError {
  if (!(error instanceof ApiError)) return false;
  const data = objectValue(error.data);
  return error.code === 409 || data?.code === 409;
}

function errorMessage(error: unknown): string {
  return error instanceof Error && error.message ? error.message : '请求失败';
}

/**
 * Map a server validation error onto editor field keys. Explicit `fields` or
 * `details` metadata wins; otherwise the backend's message is classified so a
 * top summary can always locate the problem.
 */
export function workflowFieldErrors(
  definition: WorkflowDefinition,
  error: HarnessApiError,
): Record<string, string> {
  const messages: Record<string, string> = {};
  const data = objectValue(error.data);
  const result = objectValue(data?.result);
  const fields = objectValue(data?.fields) ?? objectValue(result?.fields);
  if (fields) {
    for (const [rawField, rawMessage] of Object.entries(fields)) {
      const message = messageText(rawMessage);
      if (message) messages[normalizeFieldKey(rawField)] = message;
    }
  }
  const details = data?.details ?? result?.details;
  if (Array.isArray(details)) {
    for (const item of details) {
      const detail = objectValue(item);
      if (!detail) continue;
      const rawField = detail.field;
      if (typeof rawField !== 'string') continue;
      const message = messageText(detail.message) ?? messageText(detail);
      if (message) messages[normalizeFieldKey(rawField)] = message;
    }
  }
  if (Object.keys(messages).length > 0) return messages;

  const message = error.message ?? '';
  if (/objective|目标/i.test(message)) {
    messages.objective = DEFAULT_FIELD_MESSAGES.objective;
  } else if (/cron/i.test(message) && definition.trigger.type === 'cron') {
    messages['trigger.expression'] = DEFAULT_FIELD_MESSAGES['trigger.expression'];
  } else if (/trigger/i.test(message)) {
    messages['trigger.triggerId'] = DEFAULT_FIELD_MESSAGES['trigger.triggerId'];
  } else if (/device/i.test(message)) {
    messages.deviceSelector = DEFAULT_FIELD_MESSAGES.deviceSelector;
  } else if (/tool/i.test(message)) {
    messages.toolIds = DEFAULT_FIELD_MESSAGES.toolIds;
  } else if (/skill/i.test(message)) {
    messages.skillRefs = DEFAULT_FIELD_MESSAGES.skillRefs;
  } else if (/approval/i.test(message)) {
    messages.approvalPolicy = DEFAULT_FIELD_MESSAGES.approvalPolicy;
  } else if (/evidence/i.test(message)) {
    messages.evidencePolicy = DEFAULT_FIELD_MESSAGES.evidencePolicy;
  } else {
    messages._form = message || DEFAULT_FIELD_MESSAGES._form;
  }
  return messages;
}

function blankForm(): EditorForm {
  return {
    objective: '',
    deviceSelector: '{}',
    sourceSelector: '{}',
    triggerType: 'cron',
    triggerId: '',
    cronExpression: '',
    toolIds: '',
    skillRefs: '',
    requiredCapabilities: '',
    approvalPolicy: '{}',
    evidencePolicy: '{}',
  };
}

function jsonText(value: unknown): string {
  return JSON.stringify(value ?? {}, null, 2);
}

function formFromDefinition(definition: WorkflowDefinition): EditorForm {
  const trigger = definition.trigger;
  return {
    objective: definition.objective,
    deviceSelector: jsonText(definition.deviceSelector),
    sourceSelector: jsonText(definition.sourceSelector),
    triggerType: trigger.type,
    triggerId: trigger.triggerId,
    cronExpression: trigger.type === 'cron' ? trigger.expression : '',
    toolIds: definition.toolIds.join('\n'),
    skillRefs: definition.skillRefs.join('\n'),
    requiredCapabilities: definition.requiredCapabilities.join(', '),
    approvalPolicy: jsonText(definition.approvalPolicy),
    evidencePolicy: jsonText(definition.evidencePolicy),
  };
}

function splitList(value: string): string[] {
  return value
    .split(/[\n,]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function parseJson(value: string, field: string, errors: Record<string, string>): unknown {
  try {
    return value.trim() ? JSON.parse(value) : {};
  } catch {
    errors[field] = DEFAULT_FIELD_MESSAGES[field] ?? '不是有效 JSON';
    return null;
  }
}

@customElement('view-harness-workflow-edit')
export class HarnessWorkflowEditView extends LitElement {
  @property() workspaceId = getWorkspaceId() ?? '';
  @property() workflowId = '';

  @state() private form: EditorForm = blankForm();
  @state() private revision: WorkflowRevision | null = null;
  @state() private version = 0;
  @state() private enabled = false;
  @state() private fieldErrors: Record<string, string> = {};
  @state() private feedback: EditorFeedback | null = null;
  @state() private loading = false;
  @state() private loadError = '';
  @state() private saving = false;
  @state() private dirty = false;
  @state() private announcement = '';
  @state() private dialog: EditorDialog | null = null;
  @state() private togglePending = false;
  @state() private toggleError = '';
  @state() private menuOpen = false;

  private baseline = '';
  private loadGeneration = 0;
  private pendingFocus: string | null = null;
  private pendingReturnFocus: HTMLElement | null = null;
  private returnFocus: HTMLElement | null = null;
  private leaveHref: string | null = null;
  private allowLeave = false;
  private focusTrap: DialogFocusTrap | null = null;
  private lastHash = '';
  private lastPath = '';

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    window.addEventListener('beforeunload', this.onBeforeUnload);
    window.addEventListener('popstate', this.onHistoryNavigation, true);
    window.addEventListener('hashchange', this.onHistoryNavigation, true);
    document.addEventListener('click', this.onDocumentClick, true);
    this.captureLocation();
    if (this.workflowId) {
      void this.load(false);
    } else {
      this.resetCreate();
    }
  }

  disconnectedCallback(): void {
    this.loadGeneration += 1;
    window.removeEventListener('beforeunload', this.onBeforeUnload);
    window.removeEventListener('popstate', this.onHistoryNavigation, true);
    window.removeEventListener('hashchange', this.onHistoryNavigation, true);
    document.removeEventListener('click', this.onDocumentClick, true);
    this.focusTrap?.detach();
    this.focusTrap = null;
    super.disconnectedCallback();
  }

  willUpdate(changed: PropertyValues<this>): void {
    if (!changed.has('workflowId')) return;
    const previous = changed.get('workflowId');
    if (previous === undefined || previous === this.workflowId) return;
    if (this.workflowId && this.workflowId === this.revision?.workflowId) return;
    if (this.workflowId) {
      void this.load(false);
    } else {
      this.resetCreate();
    }
  }

  protected updated(): void {
    this.syncDialogFocusTrap();
    if (this.pendingFocus) {
      const target = this.querySelector<HTMLElement>(`#${this.pendingFocus}`);
      this.pendingFocus = null;
      target?.focus();
    } else if (this.pendingReturnFocus) {
      const target = this.pendingReturnFocus;
      this.pendingReturnFocus = null;
      target?.focus();
    }
  }

  private resetCreate(): void {
    this.loadGeneration += 1;
    this.form = blankForm();
    this.revision = null;
    this.version = 0;
    this.enabled = false;
    this.fieldErrors = {};
    this.feedback = null;
    this.loading = false;
    this.loadError = '';
    this.saving = false;
    this.dirty = false;
    this.baseline = this.snapshot();
    this.menuOpen = false;
    this.pendingFocus = 'harness-editor-title';
  }

  private captureLocation(): void {
    this.lastHash = window.location.hash;
    this.lastPath = `${window.location.pathname}${window.location.search}`;
  }

  /**
   * Back/Forward (popstate) and hash navigation bypass link clicks. While the
   * form is dirty, stop the router, put the editor URL back and ask the user.
   */
  private onHistoryNavigation = (event: Event): void => {
    if (this.allowLeave) return;
    if (!this.dirty) {
      this.captureLocation();
      return;
    }
    event.stopImmediatePropagation();
    const attemptedHash = window.location.hash;
    const attemptedPath = `${window.location.pathname}${window.location.search}`;
    if (attemptedHash === this.lastHash && attemptedPath === this.lastPath) return;
    const shouldRestoreHash = attemptedHash !== this.lastHash;
    const restore = shouldRestoreHash
      ? `${this.lastPath}${this.lastHash}`
      : `${this.lastPath}${attemptedHash}`;
    window.history.replaceState({}, '', restore);
    const attempted = attemptedHash || attemptedPath;
    if (attempted && attempted !== `${this.lastPath}${this.lastHash}`) {
      this.leaveHref = attempted;
    }
    if (!this.dialog) {
      this.returnFocus =
        document.activeElement instanceof HTMLElement ? document.activeElement : null;
      this.dialog = { kind: 'leave' };
    }
  };

  private async load(manual: boolean): Promise<void> {
    if (this.saving) return;
    if (!this.workflowId) {
      this.resetCreate();
      return;
    }
    const generation = ++this.loadGeneration;
    this.loading = true;
    this.loadError = '';
    try {
      const response = await getHarnessWorkflow(this.workspaceId, this.workflowId);
      if (generation !== this.loadGeneration) return;
      const revision = response.result;
      if (!revision) throw new Error('空响应');
      this.applyRevision(revision);
      this.loading = false;
      if (manual) {
        this.feedback = {
          kind: 'success',
          title: `已重新加载版本 v${revision.version}`,
          detail: '本地编辑已替换为服务器上的当前版本。',
          next: '下一步：重新确认配置后再保存。',
        };
        this.announce(`已重新加载版本 v${revision.version}`);
      }
      this.pendingFocus = 'harness-editor-title';
    } catch (error) {
      if (generation !== this.loadGeneration) return;
      this.loading = false;
      this.loadError = errorMessage(error);
      this.pendingFocus = 'harness-editor-title';
    }
  }

  private applyRevision(revision: WorkflowRevision): void {
    this.revision = revision;
    this.version = revision.version;
    this.enabled = revision.enabled;
    this.form = formFromDefinition(revision.definition);
    this.fieldErrors = {};
    this.saving = false;
    this.dirty = false;
    this.baseline = this.snapshot();
  }

  private snapshot(): string {
    return JSON.stringify(this.form);
  }

  private recomputeDirty(): void {
    this.dirty = this.snapshot() !== this.baseline;
  }

  private announce(message: string): void {
    this.announcement = nextAnnouncement(this.announcement, message);
  }

  private onFieldInput(field: string, formKey: keyof EditorForm, event: Event): void {
    const value = (event.currentTarget as HTMLInputElement | HTMLTextAreaElement).value;
    this.form = { ...this.form, [formKey]: value };
    if (this.fieldErrors[field]) {
      const next = { ...this.fieldErrors };
      delete next[field];
      this.fieldErrors = next;
    }
    this.recomputeDirty();
  }

  private setTriggerType(type: TriggerType, focus = true): void {
    if (this.form.triggerType === type) return;
    this.form = { ...this.form, triggerType: type };
    this.recomputeDirty();
    if (focus) this.pendingFocus = `trigger-tab-${type}`;
  }

  private onTriggerTabKeydown(event: KeyboardEvent): void {
    const currentIndex = TRIGGER_TYPES.indexOf(this.form.triggerType);
    let nextIndex: number | null = null;
    if (event.key === 'ArrowRight') nextIndex = (currentIndex + 1) % TRIGGER_TYPES.length;
    else if (event.key === 'ArrowLeft')
      nextIndex = (currentIndex - 1 + TRIGGER_TYPES.length) % TRIGGER_TYPES.length;
    else if (event.key === 'Home') nextIndex = 0;
    else if (event.key === 'End') nextIndex = TRIGGER_TYPES.length - 1;
    if (nextIndex === null) return;
    event.preventDefault();
    this.setTriggerType(TRIGGER_TYPES[nextIndex]);
  }

  private focusGroup(group: GroupId): void {
    const heading = this.querySelector<HTMLElement>(`#harness-group-${group} h2`);
    heading?.scrollIntoView?.({ block: 'start' });
    heading?.focus();
  }

  private focusField(field: string): void {
    const id = FIELD_INPUT_IDS[field];
    const element =
      this.querySelector<HTMLElement>(id ? `#${id}` : '#harness-error-summary') ??
      this.querySelector<HTMLElement>('#harness-error-summary');
    element?.scrollIntoView?.({ block: 'center' });
    element?.focus();
  }

  private collectDefinition(): {
    definition: WorkflowDefinition | null;
    errors: Record<string, string>;
  } {
    const errors: Record<string, string> = {};
    if (!this.form.objective.trim()) errors.objective = '请输入目标';
    if (!this.form.triggerId.trim()) errors['trigger.triggerId'] = '请输入触发 ID';
    if (this.form.triggerType === 'cron' && !this.form.cronExpression.trim()) {
      errors['trigger.expression'] = '请输入 Cron 表达式';
    }

    const deviceSelector = parseJson(this.form.deviceSelector, 'deviceSelector', errors);
    const sourceSelector = parseJson(this.form.sourceSelector, 'sourceSelector', errors);
    const approvalPolicy = parseJson(this.form.approvalPolicy, 'approvalPolicy', errors);
    const evidencePolicy = parseJson(this.form.evidencePolicy, 'evidencePolicy', errors);

    if (Object.keys(errors).length > 0) return { definition: null, errors };

    const trigger: WorkflowTrigger =
      this.form.triggerType === 'cron'
        ? {
            type: 'cron',
            triggerId: this.form.triggerId.trim(),
            expression: this.form.cronExpression.trim(),
          }
        : { type: this.form.triggerType, triggerId: this.form.triggerId.trim() };

    return {
      errors,
      definition: {
        objective: this.form.objective.trim(),
        trigger,
        deviceSelector,
        sourceSelector,
        toolIds: splitList(this.form.toolIds),
        skillRefs: splitList(this.form.skillRefs),
        requiredCapabilities: splitList(this.form.requiredCapabilities),
        approvalPolicy,
        evidencePolicy,
      },
    };
  }

  private onSubmit(event: Event): void {
    event.preventDefault();
    void this.save();
  }

  private async save(): Promise<void> {
    if (this.saving || this.loading) return;
    const { definition, errors } = this.collectDefinition();
    if (!definition) {
      this.applyFieldErrors(errors);
      return;
    }

    this.saving = true;
    this.fieldErrors = {};
    this.feedback = {
      kind: 'pending',
      title: '正在保存配置…',
      detail: '请等待当前请求完成。',
      next: '保存完成后会在此处显示版本与启停状态。',
    };
    this.announce('正在保存配置');
    try {
      const response = this.revision
        ? await updateHarnessWorkflow(this.workspaceId, this.revision.workflowId, {
            expectedVersion: this.version,
            enabled: this.enabled,
            definition,
          })
        : await createHarnessWorkflow(this.workspaceId, definition);
      const revision = response.result;
      if (!revision) throw new Error('空响应');
      const created = this.revision === null;
      this.applyRevision(revision);
      this.saving = false;
      this.feedback = {
        kind: 'success',
        title: `已保存版本 v${revision.version}`,
        detail: revision.enabled
          ? '启停状态保持为已启用；新版本只影响后续计划。'
          : '启停状态保持为未启用；保存不会隐式启用。',
        next: revision.enabled
          ? '下一步：核对配置摘要，或返回工作台。'
          : '下一步：如需运行计划，请使用显式启用操作。',
      };
      this.announce(`已保存版本 v${revision.version}`);
      if (created) {
        window.history.replaceState(
          {},
          '',
          `#/harness/workflows/${encodeURIComponent(revision.workflowId)}`,
        );
        this.captureLocation();
        window.dispatchEvent(new PopStateEvent('popstate'));
      }
    } catch (error) {
      this.saving = false;
      if (isConflict(error)) {
        const latest = conflictVersion(error);
        this.feedback = {
          kind: 'conflict',
          title: WORKFLOW_CONFLICT_MESSAGE,
          detail: `${latest === null ? '服务端已有更新。' : `最新版本：v${latest}。`}你的本地编辑已保留，未自动覆盖。`,
          next: '下一步：手动重新加载后再试。',
        };
        this.pendingFocus = 'harness-feedback';
        return;
      }
      const apiError = error instanceof ApiError ? error : new ApiError(0, errorMessage(error));
      const mapped = workflowFieldErrors(definition, apiError);
      const hasFieldErrors = Object.keys(mapped).some((field) => field !== '_form');
      if (hasFieldErrors) {
        this.applyFieldErrors(mapped);
        return;
      }
      this.feedback = {
        kind: 'error',
        title: '保存失败',
        detail: mapped._form ?? errorMessage(error),
        next: '下一步：检查配置后重试；若重复失败请联系管理员。',
      };
      this.pendingFocus = 'harness-feedback';
    }
  }

  private applyFieldErrors(errors: Record<string, string>): void {
    this.fieldErrors = errors;
    this.feedback = null;
    const first = Object.keys(errors)[0];
    this.pendingFocus = first ? FIELD_INPUT_IDS[first] ?? 'harness-error-summary' : null;
  }

  private requestLeave(href: string): void {
    if (!this.dirty || this.allowLeave) {
      this.navigate(href);
      return;
    }
    this.returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    this.leaveHref = href;
    this.dialog = { kind: 'leave' };
  }

  private confirmLeave(): void {
    const href = this.leaveHref;
    this.allowLeave = true;
    this.dirty = false;
    this.dialog = null;
    this.leaveHref = null;
    if (href) this.navigate(href);
  }

  private navigate(href: string): void {
    if (href.startsWith('#')) {
      window.location.hash = href;
      return;
    }
    window.history.pushState({}, '', href);
    window.dispatchEvent(new PopStateEvent('popstate'));
  }

  private openToggle(event: Event): void {
    if (!this.revision) return;
    this.returnFocus = event.currentTarget instanceof HTMLElement ? event.currentTarget : null;
    this.toggleError = '';
    this.dialog = { kind: 'toggle' };
  }

  private async confirmToggle(): Promise<void> {
    if (!this.revision || this.togglePending) return;
    const targetEnabled = !this.enabled;
    this.togglePending = true;
    this.toggleError = '';
    try {
      const response = await setHarnessWorkflowEnabled(
        this.workspaceId,
        this.revision.workflowId,
        targetEnabled,
      );
      const revision = response.result;
      if (!revision) throw new Error('空响应');
      // Only status metadata changes; unsaved local form edits stay intact.
      this.revision = revision;
      this.version = revision.version;
      this.enabled = revision.enabled;
      this.togglePending = false;
      this.closeDialog();
      this.feedback = {
        kind: 'success',
        title: revision.enabled ? '已启用计划' : '已停用计划',
        detail: revision.enabled
          ? '后续计划会使用当前 revision；在途 Task/Run 不受影响。'
          : '只停止未来计划；在途 Task/Run 保持旧 revision，不会被隐式取消。',
        next: '下一步：查看配置摘要或返回工作台。',
      };
      this.announce(revision.enabled ? '已启用计划' : '已停用计划');
    } catch (error) {
      this.togglePending = false;
      this.toggleError = `${errorMessage(error)} 下一步：确认权限后重试。`;
      this.pendingFocus = 'harness-toggle-error';
    }
  }

  private closeDialog(): void {
    if (this.dialog?.kind === 'toggle' && this.togglePending) return;
    this.dialog = null;
    this.toggleError = '';
    this.pendingReturnFocus = this.returnFocus;
    this.returnFocus = null;
  }

  private openMenu(event: Event): void {
    if (this.menuOpen) {
      this.closeMenu(true);
      return;
    }
    this.returnFocus = event.currentTarget instanceof HTMLElement ? event.currentTarget : null;
    this.menuOpen = true;
    this.pendingFocus = 'harness-menu-reload';
  }

  private closeMenu(focusTrigger: boolean): void {
    if (!this.menuOpen) return;
    this.menuOpen = false;
    if (focusTrigger) this.pendingReturnFocus = this.returnFocus;
    this.returnFocus = null;
  }

  private onMenuKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      this.closeMenu(true);
      return;
    }
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    const items = [...this.querySelectorAll<HTMLElement>('[role="menuitem"]')];
    if (items.length === 0) return;
    event.preventDefault();
    const current = items.indexOf(document.activeElement as HTMLElement);
    const offset = event.key === 'ArrowDown' ? 1 : -1;
    const next = (Math.max(current, 0) + offset + items.length) % items.length;
    items[next].focus();
  }

  private onDocumentClick = (event: MouseEvent): void => {
    const target = event.target;
    if (!(target instanceof Element)) return;

    if (this.menuOpen && !this.querySelector('.harness-editor__more')?.contains(target)) {
      this.closeMenu(false);
    }

    if (!this.dirty || this.allowLeave || this.dialog) return;
    const anchor = target.closest<HTMLAnchorElement>('a[href]');
    if (!anchor || this.contains(anchor)) return;
    const href = anchor.getAttribute('href');
    if (!href || href.startsWith('javascript:')) return;
    event.preventDefault();
    event.stopPropagation();
    this.returnFocus = anchor;
    this.leaveHref = href;
    this.dialog = { kind: 'leave' };
  };

  private onBeforeUnload = (event: BeforeUnloadEvent): void => {
    if (!this.dirty || this.allowLeave) return;
    event.preventDefault();
    event.returnValue = '';
  };

  private syncDialogFocusTrap(): void {
    if (this.dialog && !this.focusTrap) {
      const host = this.querySelector<HTMLElement>('[data-dialog]');
      if (!host) return;
      this.focusTrap = createDialogFocusTrap(host, {
        onEscape: () => this.closeDialog(),
        focusables: () => {
          if (!host.isConnected) return [];
          return [...host.querySelectorAll<HTMLElement>('button:not([disabled]), [href]')];
        },
        primary: () =>
          host.querySelector<HTMLElement>(
            '[data-action="leave-cancel"], [data-action="toggle-cancel"]',
          ),
      });
      this.focusTrap.attach();
      this.pendingFocus =
        this.dialog.kind === 'leave' ? 'harness-leave-cancel' : 'harness-toggle-cancel';
    } else if (!this.dialog && this.focusTrap) {
      this.focusTrap.detach();
      this.focusTrap = null;
    }
  }

  private renderField(config: {
    field: string;
    formKey: keyof EditorForm;
    id: string;
    label: string;
    value: string;
    textarea?: boolean;
    rows?: number;
    hint?: string;
  }) {
    const error = this.fieldErrors[config.field];
    const describedBy = [config.hint ? `${config.id}-hint` : '', error ? `${config.id}-error` : '']
      .filter(Boolean)
      .join(' ');
    return html`
      <div class="harness-editor__field" data-field=${config.field}>
        <label for=${config.id}>${config.label}</label>
        ${
          config.textarea
            ? html`<textarea
                id=${config.id}
                .value=${config.value}
                rows=${config.rows ?? 3}
                aria-invalid=${error ? 'true' : 'false'}
                aria-describedby=${describedBy || nothing}
                @input=${(event: Event) =>
                  this.onFieldInput(config.field, config.formKey, event)}
              ></textarea>`
            : html`<input
                id=${config.id}
                .value=${config.value}
                aria-invalid=${error ? 'true' : 'false'}
                aria-describedby=${describedBy || nothing}
                @input=${(event: Event) =>
                  this.onFieldInput(config.field, config.formKey, event)}
              />`
        }
        ${config.hint ? html`<p class="harness-editor__hint" id=${`${config.id}-hint`}>${config.hint}</p>` : nothing}
        ${error ? html`<p class="harness-editor__field-error" id=${`${config.id}-error`}>${error}</p>` : nothing}
      </div>
    `;
  }

  private renderErrorSummary() {
    const entries = Object.entries(this.fieldErrors);
    if (entries.length === 0) return nothing;
    return html`
      <div
        class="harness-editor__error-summary"
        id="harness-error-summary"
        role="alert"
        tabindex="-1"
      >
        <h2>请修正以下配置</h2>
        <ul>
          ${entries.map(
            ([field, message]) => html`<li>
              <a
                href=${`#harness-field-${field}`}
                data-error-field=${field}
                @click=${(event: Event) => {
                  event.preventDefault();
                  this.focusField(field);
                }}
                >${FIELD_LABELS[field] ?? field}：${message}</a
              >
            </li>`,
          )}
        </ul>
      </div>
    `;
  }

  private renderGroupsNav() {
    return html`
      <nav class="harness-editor__groups" aria-label="配置分组">
        ${GROUP_ORDER.map(
          (group) => html`<a
            href=${`#harness-group-${group}`}
            data-group-link=${group}
            @click=${(event: Event) => {
              event.preventDefault();
              this.focusGroup(group);
            }}
            >${GROUP_LABELS[group]}</a
          >`,
        )}
      </nav>
    `;
  }

  private renderObjectiveGroup() {
    return html`
      <section
        class="harness-editor__group"
        id="harness-group-objective"
        data-group="objective"
        aria-labelledby="harness-group-objective-title"
      >
        <h2 id="harness-group-objective-title" tabindex="-1">${GROUP_LABELS.objective}</h2>
        ${this.renderField({
          field: 'objective',
          formKey: 'objective',
          id: 'workflow-objective',
          label: '目标',
          value: this.form.objective,
          textarea: true,
          rows: 3,
          hint: '用一句话描述持续执行的目标；该文本会进入主 Agent 上下文。',
        })}
        ${this.renderField({
          field: 'deviceSelector',
          formKey: 'deviceSelector',
          id: 'workflow-device-selector',
          label: '设备选择器（JSON）',
          value: this.form.deviceSelector,
          textarea: true,
          hint: '声明式选择器，由后端解释；这里只保证 JSON 原样往返。',
        })}
        ${this.renderField({
          field: 'sourceSelector',
          formKey: 'sourceSelector',
          id: 'workflow-source-selector',
          label: 'Source 选择器（JSON）',
          value: this.form.sourceSelector,
          textarea: true,
        })}
      </section>
    `;
  }

  private renderTriggerGroup() {
    return html`
      <section
        class="harness-editor__group"
        id="harness-group-trigger"
        data-group="trigger"
        aria-labelledby="harness-group-trigger-title"
      >
        <h2 id="harness-group-trigger-title" tabindex="-1">${GROUP_LABELS.trigger}</h2>
        <div class="harness-editor__field">
          <span class="harness-editor__label" id="trigger-type-label">触发类型</span>
          <div
            class="harness-editor__tabs"
            role="tablist"
            aria-labelledby="trigger-type-label"
            @keydown=${(event: KeyboardEvent) => this.onTriggerTabKeydown(event)}
          >
            ${TRIGGER_TYPES.map((type) => {
              const selected = this.form.triggerType === type;
              return html`<button
                type="button"
                role="tab"
                id=${`trigger-tab-${type}`}
                data-trigger-type=${type}
                aria-selected=${selected ? 'true' : 'false'}
                aria-controls=${`trigger-panel-${type}`}
                tabindex=${selected ? '0' : '-1'}
                class="harness-editor__tab"
                @click=${() => this.setTriggerType(type)}
              >
                ${TRIGGER_LABELS[type]}
              </button>`;
            })}
          </div>
        </div>
        ${this.renderField({
          field: 'trigger.triggerId',
          formKey: 'triggerId',
          id: 'workflow-trigger-id',
          label: '触发 ID',
          value: this.form.triggerId,
          hint: '稳定标识；相同 Workflow 内不可重复。',
        })}
        <div
          id="trigger-panel-cron"
          role="tabpanel"
          aria-labelledby="trigger-tab-cron"
          ?hidden=${this.form.triggerType !== 'cron'}
        >
          ${this.renderField({
            field: 'trigger.expression',
            formKey: 'cronExpression',
            id: 'workflow-cron-expression',
            label: 'Cron 表达式',
            value: this.form.cronExpression,
            hint: '时间计划只表示计划时间，不承诺准点执行。时区见配置摘要。',
          })}
        </div>
        <div
          id="trigger-panel-watch"
          role="tabpanel"
          aria-labelledby="trigger-tab-watch"
          ?hidden=${this.form.triggerType !== 'watch'}
        >
          <p class="harness-editor__hint">Watch 触发由设备观察条件唤醒；具体条件由后端解释。</p>
        </div>
        <div
          id="trigger-panel-event"
          role="tabpanel"
          aria-labelledby="trigger-tab-event"
          ?hidden=${this.form.triggerType !== 'event'}
        >
          <p class="harness-editor__hint">Event 触发由工作区事件唤醒；具体来源由后端解释。</p>
        </div>
      </section>
    `;
  }

  private renderToolsGroup() {
    return html`
      <section
        class="harness-editor__group"
        id="harness-group-tools"
        data-group="tools"
        aria-labelledby="harness-group-tools-title"
      >
        <h2 id="harness-group-tools-title" tabindex="-1">${GROUP_LABELS.tools}</h2>
        ${this.renderField({
          field: 'toolIds',
          formKey: 'toolIds',
          id: 'workflow-tool-ids',
          label: '工具 ID（每行一个）',
          value: this.form.toolIds,
          textarea: true,
          rows: 4,
          hint: '只能是注册表声明的工具；未声明工具不会进入 Agent 上下文。',
        })}
        ${this.renderField({
          field: 'skillRefs',
          formKey: 'skillRefs',
          id: 'workflow-skill-refs',
          label: 'Skills（每行一个）',
          value: this.form.skillRefs,
          textarea: true,
          rows: 3,
        })}
        ${this.renderField({
          field: 'requiredCapabilities',
          formKey: 'requiredCapabilities',
          id: 'workflow-required-capabilities',
          label: '所需能力（逗号分隔）',
          value: this.form.requiredCapabilities,
        })}
      </section>
    `;
  }

  private renderPolicyGroup() {
    return html`
      <section
        class="harness-editor__group"
        id="harness-group-policy"
        data-group="policy"
        aria-labelledby="harness-group-policy-title"
      >
        <h2 id="harness-group-policy-title" tabindex="-1">${GROUP_LABELS.policy}</h2>
        ${this.renderField({
          field: 'approvalPolicy',
          formKey: 'approvalPolicy',
          id: 'workflow-approval-policy',
          label: '审批策略（JSON）',
          value: this.form.approvalPolicy,
          textarea: true,
          hint: '只能收紧工具与设备的强制策略，不能降低审批要求。',
        })}
        ${this.renderField({
          field: 'evidencePolicy',
          formKey: 'evidencePolicy',
          id: 'workflow-evidence-policy',
          label: '证据策略（JSON）',
          value: this.form.evidencePolicy,
          textarea: true,
        })}
      </section>
    `;
  }

  private renderSummaryGroup() {
    const nextVersion = this.revision ? this.version + 1 : 1;
    return html`
      <section
        class="harness-editor__group harness-editor__group--summary"
        id="harness-group-summary"
        data-group="summary"
        aria-labelledby="harness-group-summary-title"
      >
        <h2 id="harness-group-summary-title" tabindex="-1">${GROUP_LABELS.summary}</h2>
        <dl class="harness-editor__summary">
          <div><dt>目标</dt><dd>${this.form.objective.trim() || '尚未填写'}</dd></div>
          <div>
            <dt>触发</dt>
            <dd>${TRIGGER_LABELS[this.form.triggerType]} · ${this.form.triggerId.trim() || '尚未填写'}</dd>
          </div>
          <div><dt>工具</dt><dd>${splitList(this.form.toolIds).length} 个</dd></div>
          <div><dt>Skills</dt><dd>${splitList(this.form.skillRefs).length} 个</dd></div>
          <div>
            <dt>启停</dt>
            <dd>
              ${this.revision ? renderHarnessStatus(this.enabled ? 'enabled' : 'disabled') : '保存后为未启用'}
              <span class="harness-editor__summary-note">保存不会改变启停状态。</span>
            </dd>
          </div>
          <div><dt>版本</dt><dd>${this.revision ? `v${this.version} → 保存后 v${nextVersion}` : '首次保存为 v1'}</dd></div>
          <div><dt>时区</dt><dd>${currentTimeZone()}</dd></div>
          <div><dt>更新时间</dt><dd>${formatHarnessTime(this.revision?.createdAt ?? null)}</dd></div>
        </dl>
        <p class="harness-editor__hint">
          新建空白 Workflow 保存为未启用；编辑已有 Workflow 时保留当前 enabled/disabled。禁用只通过显式动作发生。
        </p>
      </section>
    `;
  }

  private renderFeedback() {
    if (!this.feedback) return nothing;
    const alert = this.feedback.kind === 'error' || this.feedback.kind === 'conflict';
    return html`
      <div
        class="harness-editor__feedback"
        id="harness-feedback"
        data-feedback=${this.feedback.kind}
        role=${alert ? 'alert' : 'group'}
        aria-live=${alert ? 'assertive' : 'off'}
        tabindex="-1"
      >
        <strong>${this.feedback.title}</strong>
        <p>${this.feedback.detail}</p>
        <p class="harness-editor__next">${this.feedback.next}</p>
        ${
          this.feedback.kind === 'conflict'
            ? html`<button
                type="button"
                class="btn"
                data-action="reload"
                ?disabled=${this.saving || this.loading}
                @click=${() => this.load(true)}
              >
                重新加载
              </button>`
            : nothing
        }
      </div>
    `;
  }

  private renderActions() {
    return html`
      <div class="harness-editor__actions">
        <button
          type="submit"
          class="btn btn--primary"
          data-action="save"
          ?disabled=${this.saving}
        >
          ${this.saving ? '保存中…' : '保存配置'}
        </button>
        ${
          this.revision
            ? html`<button
                type="button"
                class="btn"
                data-action="toggle-enabled"
                ?disabled=${this.togglePending}
                @click=${(event: Event) => this.openToggle(event)}
              >
                ${this.enabled ? '停用计划' : '启用计划'}
              </button>`
            : html`<button type="button" class="btn" data-action="toggle-enabled" disabled>
                保存后可启用
              </button>`
        }
        <div class="harness-editor__more">
          <button
            type="button"
            class="btn"
            data-action="more"
            aria-haspopup="menu"
            aria-expanded=${this.menuOpen ? 'true' : 'false'}
            aria-controls="harness-editor-menu"
            @click=${(event: Event) => this.openMenu(event)}
          >
            更多操作
          </button>
          ${
            this.menuOpen
              ? html`<div
                  class="harness-editor__menu"
                  id="harness-editor-menu"
                  role="menu"
                  @keydown=${(event: KeyboardEvent) => this.onMenuKeydown(event)}
                >
                  <button
                    type="button"
                    role="menuitem"
                    id="harness-menu-reload"
                    data-action="menu-reload"
                    ?disabled=${this.dirty || this.saving || this.loading}
                    @click=${() => {
                      this.closeMenu(false);
                      void this.load(true);
                    }}
                  >
                    重新加载配置
                  </button>
                  <button
                    type="button"
                    role="menuitem"
                    data-action="menu-back"
                    @click=${() => {
                      this.closeMenu(false);
                      this.requestLeave('#/harness');
                    }}
                  >
                    返回工作台
                  </button>
                </div>`
              : nothing
          }
        </div>
        <a
          class="harness-editor__back-link"
          href="#/harness"
          data-action="back"
          @click=${(event: Event) => {
            event.preventDefault();
            this.requestLeave('#/harness');
          }}
          >返回工作台</a
        >
      </div>
    `;
  }

  private renderDialog() {
    if (!this.dialog) return nothing;
    if (this.dialog.kind === 'leave') {
      return html`
        <div class="harness-dialog-overlay">
          <div
            class="harness-dialog"
            data-dialog="leave"
            role="dialog"
            aria-modal="true"
            aria-labelledby="harness-leave-title"
            aria-describedby="harness-leave-detail"
          >
            <h2 id="harness-leave-title">未保存的更改</h2>
            <p id="harness-leave-detail">离开后本地编辑不会保存，且不会自动提交。</p>
            <div class="harness-dialog__actions">
              <button
                type="button"
                class="btn"
                id="harness-leave-cancel"
                data-action="leave-cancel"
                @click=${() => this.closeDialog()}
              >
                继续编辑
              </button>
              <button
                type="button"
                class="btn btn--danger"
                data-action="leave-confirm"
                @click=${() => this.confirmLeave()}
              >
                放弃更改并离开
              </button>
            </div>
          </div>
        </div>
      `;
    }

    return html`
      <div class="harness-dialog-overlay">
        <div
          class="harness-dialog"
          data-dialog="toggle"
          role="dialog"
          aria-modal="true"
          aria-labelledby="harness-toggle-title"
        >
          <h2 id="harness-toggle-title">${this.enabled ? '停用计划' : '启用计划'}</h2>
          <p>
            ${this.enabled
              ? '停用后只停止未来计划；在途 Task/Run 保持旧 revision，不会被隐式取消。'
              : '启用后后续计划会使用当前 revision；在途 Task/Run 不受影响。'}
          </p>
          ${
            this.togglePending
              ? html`<p role="status">正在更新启停状态…</p>`
              : nothing
          }
          ${
            this.toggleError
              ? html`<p id="harness-toggle-error" role="alert">${this.toggleError}</p>`
              : nothing
          }
          <div class="harness-dialog__actions">
            <button
              type="button"
              class="btn"
              id="harness-toggle-cancel"
              data-action="toggle-cancel"
              ?disabled=${this.togglePending}
              @click=${() => this.closeDialog()}
            >
              取消
            </button>
            <button
              type="button"
              class="btn ${this.enabled ? 'btn--danger' : 'btn--primary'}"
              data-action="toggle-confirm"
              ?disabled=${this.togglePending}
              @click=${() => this.confirmToggle()}
            >
              ${this.togglePending ? '更新中…' : this.enabled ? '确认停用' : '确认启用'}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  render() {
    return html`
      <section class="harness-editor" aria-labelledby="harness-editor-title">
        <a
          class="harness-editor__back"
          href="#/harness"
          data-action="back-top"
          @click=${(event: Event) => {
            event.preventDefault();
            this.requestLeave('#/harness');
          }}
          >← 返回工作台</a
        >
        <header class="harness-editor__header">
          <div>
            <h1 id="harness-editor-title" tabindex="-1">
              ${this.revision ? '编辑工作流' : '新建工作流'}
            </h1>
            <p>
              单页分组配置。保存只写入新 revision；启停是单独动作，不会因保存而隐式改变。
            </p>
          </div>
          <div class="harness-editor__state">
            ${
              this.revision
                ? html`<span class="harness-editor__version">版本 v${this.version}</span>
                    ${renderHarnessStatus(this.enabled ? 'enabled' : 'disabled')}`
                : html`<span class="harness-editor__version">新配置 · 保存后为未启用</span>`
            }
          </div>
        </header>
        ${
          this.loading
            ? html`<p class="harness-editor__status" role="status">正在加载配置…</p>`
            : nothing
        }
        ${
          this.loadError
            ? html`<div class="harness-editor__load-error" role="alert">
                <p>加载 Workflow 失败：${this.loadError}</p>
                <button type="button" class="btn" data-action="retry" @click=${() => this.load(false)}>
                  重试
                </button>
              </div>`
            : nothing
        }
        ${
          !this.loading && !this.loadError
            ? html`${this.renderGroupsNav()}
                <form class="harness-editor__form" novalidate @submit=${this.onSubmit}>
                  ${this.renderErrorSummary()}
                  ${this.renderObjectiveGroup()}
                  ${this.renderTriggerGroup()}
                  ${this.renderToolsGroup()}
                  ${this.renderPolicyGroup()}
                  ${this.renderSummaryGroup()}
                  ${this.renderFeedback()}
                  ${this.renderActions()}
                </form>`
            : nothing
        }
        ${this.renderDialog()}
        <p
          class="harness-editor__sr-only"
          data-announcer
          role="status"
          aria-live="polite"
          aria-atomic="true"
        >
          ${this.announcement}
        </p>
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-harness-workflow-edit': HarnessWorkflowEditView;
  }
}
