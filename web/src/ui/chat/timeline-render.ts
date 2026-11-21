import { html, nothing, type TemplateResult } from 'lit';
import { unsafeHTML } from 'lit/directives/unsafe-html.js';
import { md } from '../shared/markdown.js';
import type { ContentPart } from './timeline-model.js';
import type { TodoItem } from '../../api/assistant.js';
import { presentTool } from '../tool-views/registry.js';

const TOOL_META: Record<string, { icon: string; verb: string }> = {
  get_thing: { icon: '◎', verb: '查看设备' },
  read_device: { icon: '◎', verb: '查看设备' },
  list_devices: { icon: '☷', verb: '列出设备' },
  search_things: { icon: '⌕', verb: '查找设备' },
  get_skill: { icon: '◆', verb: '读取技能' },
  alarm_list: { icon: '⚠', verb: '列出告警' },
  read_properties: { icon: '◫', verb: '读取属性' },
  write_properties: { icon: '✎', verb: '写入属性' },
  send_command: { icon: '▸', verb: '下发命令' },
  get_device_history: { icon: '◷', verb: '读取历史' },
  list_alarms: { icon: '⚠', verb: '列出告警' },
  acknowledge_alarm: { icon: '✓', verb: '确认告警' },
  resolve_alarm: { icon: '✓', verb: '解除告警' },
  query_data: { icon: '⌕', verb: '查询数据' },
  aggregate_data: { icon: 'Σ', verb: '聚合数据' },
  todo_write: { icon: '☑', verb: '更新计划' },
  ask_user: { icon: '?', verb: '请你决定' },
  spawn_subagent: { icon: '⧉', verb: '派出子代理' },
};

const SUBAGENT_STATUS: Record<string, string> = {
  running: '进行中',
  completed: '已返回',
  failed: '失败',
  cancelled: '已取消',
};

const TURN_STATE: Record<string, string> = {
  running: '处理中',
  completed: '完成',
  partial: '部分完成',
  failed: '失败',
  cancelled: '已取消',
};

const TRIGGER_LABEL: Record<string, string> = {
  schedule: '定时巡检触发',
  alarm: '告警触发',
  watch: '值守规则触发',
  thing_event: '设备事件触发',
  directive: '指令触发',
  event: '事件触发',
};

type ToolPart = Extract<ContentPart, { kind: 'tool' }>;
type ReasoningPart = Extract<ContentPart, { kind: 'reasoning' }>;

function formatTokens(value?: number): string {
  if (!value) return '';
  return value >= 1000 ? `${(value / 1000).toFixed(value >= 10_000 ? 0 : 1)}k` : String(value);
}

function formatDuration(start: string, end?: string): string {
  if (!end) return '';
  const ms = new Date(end).getTime() - new Date(start).getTime();
  if (!Number.isFinite(ms) || ms < 0) return '';
  return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms}ms`;
}

function firstValue(args: unknown): string {
  if (!args || typeof args !== 'object') return '';
  const record = args as Record<string, unknown>;
  for (const key of ['keyword', 'thing_id', 'device_id', 'query', 'q', 'summary', 'description']) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  const first = Object.values(record).find((value) => typeof value === 'string' && value.trim());
  return typeof first === 'string' ? first.trim() : '';
}

function formatAt(value: string): string {
  if (!value) return '';
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? ''
    : date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
}

type StructuredToolResult =
  | { kind: 'device'; device: Record<string, unknown> }
  | { kind: 'devices'; devices: Record<string, unknown>[]; total?: number }
  | { kind: 'todos'; todos: TodoItem[] };

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function structuredToolResult(part: ToolPart): StructuredToolResult | null {
  if (!part.result) return null;
  let value: unknown;
  try {
    value = JSON.parse(part.result);
  } catch {
    return null;
  }
  const result = record(value);
  if (!result) return null;

  if (['todo_write', 'update_todo', 'todo'].includes(part.name)) {
    const items = result.todos ?? result.tasks;
    if (!Array.isArray(items)) return null;
    const todos: Array<TodoItem | null> = items.map((item) => {
      const todo = record(item);
      const content = todo?.content ?? todo?.title ?? todo?.name;
      if (typeof content !== 'string') return null;
      const status = todo?.status;
      return {
        content,
        activeForm: typeof todo?.activeForm === 'string' ? todo.activeForm : content,
        status: status === 'completed' || status === 'in_progress' ? status : 'pending',
      };
    });
    return todos.every((todo): todo is TodoItem => todo !== null) ? { kind: 'todos', todos } : null;
  }

  if (['list_devices', 'list_things', 'search_things'].includes(part.name)) {
    if (!Array.isArray(result.items)) return null;
    const devices = result.items.map(record);
    return devices.every((device): device is Record<string, unknown> => device !== null)
      ? {
          kind: 'devices',
          devices,
          total: typeof result.total === 'number' ? result.total : undefined,
        }
      : null;
  }

  if (['read_device', 'get_thing', 'get_thing_profile'].includes(part.name)) {
    return { kind: 'device', device: result };
  }
  return null;
}

function deviceOnline(device: Record<string, unknown>): boolean | undefined {
  if (typeof device.online === 'boolean') return device.online;
  const live = record(device.live);
  return typeof live?.online === 'boolean' ? live.online : undefined;
}

function deviceName(device: Record<string, unknown>): string {
  for (const key of ['displayName', 'name', 'thingId', 'id']) {
    const value = device[key];
    if (typeof value === 'string' && value) return value;
  }
  return '未命名设备';
}

function thingTypeLabel(value: unknown): string {
  if (typeof value !== 'string') return '';
  const labels: Record<string, string> = { device: '设备', space: '空间', line: '产线', building: '建筑' };
  return labels[value] ?? value;
}

function healthLabel(value: unknown): string {
  if (typeof value !== 'string') return '';
  const labels: Record<string, string> = { ok: '健康正常', attention: '需要关注', bad: '健康异常', unknown: '健康未知' };
  return labels[value] ?? value;
}

function displayValue(value: unknown): string {
  if (value === null || value === undefined) return '—';
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return JSON.stringify(value);
}

function structuredSummary(result: StructuredToolResult): string {
  if (result.kind === 'todos') {
    const { done, total } = todoProgress(result.todos);
    return `${done}/${total} 已完成`;
  }
  if (result.kind === 'devices') return `共 ${result.total ?? result.devices.length} 台`;
  const online = deviceOnline(result.device);
  const state = record(result.device.state);
  return [
    online === undefined ? '' : online ? '在线' : '离线',
    state ? `${Object.keys(state).length} 项属性` : '',
  ]
    .filter(Boolean)
    .join('，');
}

export function toolSummary(block: ToolPart): string {
  const arg = firstValue(block.args);
  const verb = TOOL_META[block.name]?.verb ?? block.name;
  const head = arg ? `${verb} ${arg}` : verb;
  const structured = structuredToolResult(block);
  const output =
    block.status === 'failed' && block.error
      ? block.error
      : structured
        ? structuredSummary(structured)
        : block.result?.replace(/\s+/g, ' ').trim();
  if (!output) return head;
  return `${head} → ${output.slice(0, 140)}`;
}

export function todoProgress(todos: TodoItem[]): { done: number; total: number } {
  return {
    done: todos.filter((todo) => todo.status === 'completed').length,
    total: todos.length,
  };
}

export function summarizeTodos(todos: TodoItem[]): string {
  const { done, total } = todoProgress(todos);
  return `计划中 ${done}/${total}`;
}

function activity(content: TemplateResult, className = ''): TemplateResult {
  return html`<div class="activity ${className}">${content}</div>`;
}

function renderTurn(
  part: Extract<ContentPart, { kind: 'turn' }>,
  waitingForUser = false,
): TemplateResult {
  const totalTokens = (part.usage?.inputTokens ?? 0) + (part.usage?.outputTokens ?? 0);
  const elapsed = formatDuration(part.at, part.completedAt ?? new Date().toISOString());
  const state = waitingForUser ? '等待批准' : (TURN_STATE[part.status] ?? part.status);
  return html`
    <header class="turn-header turn-header--${part.status}">
      <span class="turn-header__identity"
        ><span class="turn-header__mark" aria-hidden="true">脑</span>大脑</span
      >
      <span class="turn-header__state ${waitingForUser ? 'turn-header__state--waiting' : ''}"
        >${state}</span
      >
      <span class="turn-header__spacer"></span>
      ${totalTokens > 0 ? html`<span class="turn-header__meta">${formatTokens(totalTokens)} tokens</span>` : nothing}
      ${elapsed ? html`<span class="turn-header__meta">已处理 ${elapsed}</span>` : nothing}
    </header>
  `;
}

function renderUser(part: Extract<ContentPart, { kind: 'user' }>): TemplateResult {
  return html`<div class="log-entry log-entry--user">
    <div class="user-message">
      <span class="log-label">你</span>
      <div class="timeline-user__bubble">${part.text}</div>
    </div>
  </div>`;
}

function renderAssistant(
  part: Extract<ContentPart, { kind: 'assistant' }>,
  liveText: string,
  showHeader: boolean,
  showCursor: boolean,
): TemplateResult | typeof nothing {
  // Interactive chat already renders the token stream below as liveText;
  // avoid showing the same text twice while both update.
  if (part.streaming && liveText) return nothing;
  // A Turn header owns assistant identity; only legacy messages without a
  // turn retain the old per-message header.
  const showIdentity = showHeader && !part.turnId;
  return html`<article class="log-entry log-entry--assistant">
    ${
      showIdentity
        ? html`<div class="assistant-message__head">
            <span class="assistant-message__avatar" aria-hidden="true">脑</span>
            <span class="assistant-message__name">大脑</span>
            <time class="assistant-message__time" datetime=${part.at}>${formatAt(part.at)}</time>
          </div>`
        : nothing
    }
    <div class="timeline-assistant ${showCursor ? 'timeline-assistant--live' : ''}">
      ${unsafeHTML(md(part.text))}
    </div>
  </article>`;
}

function renderReasoning(part: ReasoningPart): TemplateResult {
  return html`<details class="activity activity--reasoning log-entry">
    <summary class="activity__summary">
      <span class="activity__icon" aria-hidden="true">◌</span>
      <span class="activity__label">${part.streaming ? '正在思考中' : '思考过程'}</span>
      <span class="activity__meta">展开</span>
    </summary>
    <div class="activity__detail process-detail">${part.text}</div>
  </details>`;
}

function renderStructuredResult(result: StructuredToolResult): TemplateResult {
  if (result.kind === 'todos') return renderTodoItems(result.todos, 'structured-todos');
  if (result.kind === 'devices') {
    return html`<section class="structured-device-list" aria-label="设备列表">
      <div class="structured-device-list__count">共 ${result.total ?? result.devices.length} 台</div>
      <ul>
        ${result.devices.map((device) => {
          const online = deviceOnline(device);
          const type = thingTypeLabel(device.thingType);
          return html`<li class="structured-device-row">
            <span class="structured-device-row__name">${deviceName(device)}</span>
            ${type ? html`<span class="structured-device-row__type">${type}</span>` : nothing}
            ${online === undefined ? nothing : html`<span class="structured-device__status ${online ? 'is-online' : 'is-offline'}">${online ? '在线' : '离线'}</span>`}
          </li>`;
        })}
      </ul>
    </section>`;
  }

  const { device } = result;
  const online = deviceOnline(device);
  const state = record(device.state);
  const context = [thingTypeLabel(device.thingType), device.zone, healthLabel(device.health)]
    .filter((value) => typeof value === 'string' && value)
    .map((value) => String(value));
  return html`<section class="structured-device" aria-label="设备状态">
    <div class="structured-device__head">
      <strong class="structured-device__name">${deviceName(device)}</strong>
      ${online === undefined ? nothing : html`<span class="structured-device__status ${online ? 'is-online' : 'is-offline'}">${online ? '在线' : '离线'}</span>`}
    </div>
    ${context.length > 0 ? html`<div class="structured-device__context">${context.map((value) => html`<span>${value}</span>`)}</div>` : nothing}
    ${state && Object.keys(state).length > 0
      ? html`<dl class="structured-properties">
          ${Object.entries(state).map(
            ([name, value]) => html`<div class="structured-property"><dt>${name}</dt><dd>${displayValue(value)}</dd></div>`,
          )}
        </dl>`
      : nothing}
  </section>`;
}

function renderTool(part: ToolPart): TemplateResult {
  const meta = TOOL_META[part.name];
  const structured = structuredToolResult(part);
  const icon = meta?.icon ?? '⚙';
  const failed = part.status === 'failed';
  const stateClass = failed
    ? 'activity__meta--failed'
    : part.result
      ? 'activity__meta--ok'
      : 'activity__meta--pending';
  const presentation = presentTool({
    name: part.name,
    viewKey: part.viewKey,
    args: part.args,
    result: part.result ?? null,
    evidence: part.evidence,
    status: failed ? 'failed' : part.result ? 'ok' : 'running',
    error: part.error,
  });
  const label =
    presentation.viewKey !== 'generic'
      ? `${presentation.title}${presentation.summary ? ` · ${presentation.summary}` : ''}`
      : toolSummary(part);
  return html`<details
    class="activity activity--tool log-entry ${failed ? 'activity--failed' : ''}"
  >
    <summary class="activity__summary">
      <span class="activity__icon" aria-hidden="true">${icon}</span>
      <span class="activity__label">${label}</span>
      <span class="activity__meta ${stateClass}"
        >${failed ? '失败' : part.result ? '完成' : '执行中'}</span
      >
    </summary>
    <div class="activity__detail process-detail">
      ${
        presentation.metrics?.length
          ? html`<dl class="activity__metrics">
              ${presentation.metrics.map(
                (metric) => html`<div><dt>${metric.label}</dt><dd>${metric.value}</dd></div>`,
              )}
            </dl>`
          : nothing
      }
      ${
        presentation.details?.length
          ? html`<dl class="activity__details">
              ${presentation.details.slice(0, 8).map(
                (detail) => html`<div><dt>${detail.label}</dt><dd>${detail.value}</dd></div>`,
              )}
            </dl>`
          : nothing
      }
      <div class="activity__detail-label">参数</div>
      <pre>${JSON.stringify(part.args ?? {}, null, 2)}</pre>
      ${
        part.result
          ? html`<div class="activity__detail-label">结果</div>
              ${structured ? renderStructuredResult(structured) : html`<pre>${part.result}</pre>`}`
          : nothing
      }
      ${
        part.evidence?.length
          ? html`<div class="activity__detail-label">证据</div>
              <pre>${JSON.stringify(part.evidence, null, 2)}</pre>`
          : nothing
      }
    </div>
  </details>`;
}

function renderSubagent(part: Extract<ContentPart, { kind: 'subagent' }>): TemplateResult {
  const status = part.status ?? 'running';
  return html`<details class="activity activity--subagent log-entry">
    <summary class="activity__summary">
      <span class="activity__icon" aria-hidden="true">⧉</span>
      <span class="activity__label">子代理 · ${part.description || part.id}</span>
      <span class="activity__meta">${SUBAGENT_STATUS[status] ?? status}</span>
    </summary>
    <div class="activity__detail process-detail">
      ${part.subagentType ? html`<div>类型 ${part.subagentType}</div>` : nothing}
      ${part.summary ? html`<div>${unsafeHTML(md(part.summary))}</div>` : nothing}
    </div>
  </details>`;
}

function renderSkill(part: Extract<ContentPart, { kind: 'skill' }>): TemplateResult {
  return activity(html`
    <span class="activity__icon" aria-hidden="true">◆</span>
    <span class="activity__label"
      >技能 · ${part.name}${part.version ? ` ${part.version}` : ''}</span
    >
  `);
}

function renderOrigin(part: Extract<ContentPart, { kind: 'origin' }>): TemplateResult {
  return activity(html`
    <span class="activity__icon" aria-hidden="true">▸</span>
    <span class="activity__label">${TRIGGER_LABEL[part.source] ?? part.source}</span>
    ${part.sourceId ? html`<span class="activity__meta">${part.sourceId.slice(0, 8)}</span>` : nothing}
  `);
}

function renderNotice(part: Extract<ContentPart, { kind: 'notice' }>): TemplateResult {
  return activity(
    html`
      <span class="activity__icon" aria-hidden="true">·</span>
      <span class="activity__label">${formatAt(part.at)} · ${part.text}</span>
    `,
    `activity--notice activity--${part.tone}`,
  );
}

function renderRetry(part: Extract<ContentPart, { kind: 'retry' }>): TemplateResult {
  const seconds = part.delayMs / 1000;
  const delay = Number.isInteger(seconds) ? String(seconds) : seconds.toFixed(1);
  return html`<div class="timeline-retry" role="status">
    <span class="timeline-retry__error">${part.error}</span>
    <span class="timeline-retry__meta"
      >${delay} 秒后重试 · 第 ${part.attempt}/${part.maxRetries} 次</span
    >
  </div>`;
}

/** Unknown/future part kinds stay readable without requiring a new UI branch. */
function renderUnknown(part: ContentPart): TemplateResult {
  const label = 'text' in part && typeof part.text === 'string' ? part.text : part.kind;
  return activity(html`
    <span class="activity__icon" aria-hidden="true">·</span>
    <span class="activity__label">${label}</span>
    <span class="activity__meta">${part.kind}</span>
  `);
}

function isProcessPart(part: ContentPart): boolean {
  return ['origin', 'skill', 'tool', 'reasoning', 'subagent', 'notice', 'retry'].includes(
    part.kind,
  );
}

function processSummary(items: Array<{ part: ContentPart; index: number }>): string {
  const tools = items.filter((item) => item.part.kind === 'tool').length;
  const retries = items.filter((item) => item.part.kind === 'retry').length;
  const subagents = items.filter((item) => item.part.kind === 'subagent').length;
  const details = [
    tools > 0 ? `${tools} 个工具` : '',
    retries > 0 ? `${retries} 次重试` : '',
    subagents > 0 ? `${subagents} 个子代理` : '',
  ].filter(Boolean);
  return ['执行过程', ...details].join(' · ');
}

function latestProcessText(items: Array<{ part: ContentPart; index: number }>): string {
  const part = items[items.length - 1]?.part;
  if (!part) return '';
  switch (part.kind) {
    case 'tool':
      return toolSummary(part);
    case 'reasoning':
      return part.streaming ? '正在思考中' : '思考过程';
    case 'retry':
      return `服务暂时不可用，第 ${part.attempt} 次重试`;
    case 'subagent':
      return `子代理 · ${part.description || part.id}`;
    case 'skill':
      return `技能 · ${part.name}`;
    case 'notice':
      return part.text;
    case 'origin':
      return TRIGGER_LABEL[part.source] ?? part.source;
    default:
      return '';
  }
}

function renderProcessRun(
  items: Array<{ part: ContentPart; index: number }>,
  current: boolean,
  todos: TodoItem[],
): TemplateResult {
  return html`<details class="turn-process">
    <summary class="turn-process__summary activity__summary">
      <span class="turn-process__summary-label">${processSummary(items)}</span>
      <span class="turn-process__latest" title=${latestProcessText(items)}
        >${latestProcessText(items)}</span
      >
    </summary>
    <div class="turn-process__content">
      ${items.map(({ part }) => (PART_RENDERERS[part.kind] ?? renderUnknown)(part as never))}
      ${current ? renderTodoPanel(todos) : nothing}
    </div>
  </details>`;
}

type PartRenderer = (part: never) => TemplateResult | typeof nothing;
const PART_RENDERERS: Record<string, PartRenderer> = {
  user: renderUser as PartRenderer,
  turn: renderTurn as PartRenderer,
  tool: renderTool as PartRenderer,
  reasoning: renderReasoning as PartRenderer,
  skill: renderSkill as PartRenderer,
  subagent: renderSubagent as PartRenderer,
  origin: renderOrigin as PartRenderer,
  notice: renderNotice as PartRenderer,
  retry: renderRetry as PartRenderer,
};

function renderTodoItems(todos: TodoItem[], className = ''): TemplateResult {
  return html`<ol class="timeline-todos__list ${className}">
    ${todos.map(
      (todo) => html`
        <li class="timeline-todo timeline-todo--${todo.status}">
          <span class="timeline-todo__mark" aria-hidden="true">
            ${todo.status === 'completed' ? '✓' : todo.status === 'in_progress' ? '▸' : '·'}
          </span>
          <span class="timeline-todo__text">${todo.content}</span>
        </li>
      `,
    )}
  </ol>`;
}

export function renderTodoPanel(todos: TodoItem[]): TemplateResult | typeof nothing {
  if (todos.length === 0) return nothing;
  const { done, total } = todoProgress(todos);
  const percent = total === 0 ? 0 : Math.round((done / total) * 100);
  return html`
    <section class="timeline-todos process-detail" aria-label="本轮计划">
      <div class="timeline-todos__head">
        <span>本轮计划</span>
        <span class="timeline-todos__count">${done}/${total}</span>
      </div>
      <div class="timeline-todos__meter" aria-hidden="true">
        <span style=${`width:${percent}%`}></span>
      </div>
      ${renderTodoItems(todos)}
    </section>
  `;
}

function currentWork(parts: ContentPart[], liveText: string, thinking: string): string {
  const last = parts[parts.length - 1];
  if (last?.kind === 'tool' && !last.result) return toolSummary(last);
  if (last?.kind === 'subagent' && (last.status ?? 'running') === 'running') {
    return `子代理 · ${last.description || last.id}`;
  }
  if (last?.kind === 'retry') return `等待第 ${last.attempt} 次重试`;
  if (thinking || last?.kind === 'reasoning') return '正在思考中';
  if (liveText) return '正在生成回复';
  return '等待模型响应';
}

function renderTurnFooter(
  parts: ContentPart[],
  sending: boolean,
  liveText: string,
  thinking: string,
  waitingForUser: boolean,
  turnActions?: TemplateResult,
): TemplateResult | typeof nothing {
  const lastTurn = [...parts].reverse().find((part) => part.kind === 'turn');
  const running = sending || (lastTurn?.kind === 'turn' && lastTurn.status === 'running');
  if (!lastTurn && !running) return nothing;
  const label = waitingForUser
    ? '等待批准'
    : running
      ? currentWork(parts, liveText, thinking)
      : lastTurn?.status === 'completed'
        ? '已完成'
        : lastTurn?.status === 'failed'
          ? '执行失败'
          : '已取消';
  const elapsed = lastTurn
    ? formatDuration(lastTurn.at, lastTurn.completedAt ?? new Date().toISOString())
    : '';
  const cost = lastTurn?.kind === 'turn' ? lastTurn.usage?.costUsd : undefined;
  return html`<footer class="turn-footer ${waitingForUser ? 'turn-footer--waiting' : ''}">
    <span class="turn-footer__icon" aria-hidden="true"
      >${running && !waitingForUser ? '◌' : lastTurn?.status === 'completed' && !waitingForUser ? '✓' : '!'}</span
    >
    <span class="turn-footer__state">${label}</span>
    <span class="turn-footer__meta"
      >${[elapsed, cost && cost > 0 ? `$${cost.toFixed(4)}` : ''].filter(Boolean).join(' · ')}</span
    >
    ${
      lastTurn?.kind === 'turn' && lastTurn.error
        ? html`<div class="turn-footer__error">${lastTurn.error}</div>`
        : nothing
    }
    ${turnActions ? html`<div class="turn-actions">${turnActions}</div>` : nothing}
  </footer>`;
}

export function renderTimeline(
  parts: ContentPart[],
  opts: {
    liveText?: string;
    thinking?: string;
    sending?: boolean;
    todos?: TodoItem[];
    waitingForUser?: boolean;
    turnActions?: TemplateResult;
  } = {},
): TemplateResult {
  const {
    liveText = '',
    thinking = '',
    sending = false,
    todos = [],
    waitingForUser = false,
    turnActions,
  } = opts;
  let lastStreamingAssistant = -1;
  parts.forEach((part, index) => {
    if (part.kind === 'assistant' && part.streaming) lastStreamingAssistant = index;
  });
  const headerTurns = new Set<string>();
  const groups: Array<Array<{ part: ContentPart; index: number }>> = [];
  let group: Array<{ part: ContentPart; index: number }> = [];
  parts.forEach((part, index) => {
    if (part.kind === 'turn' && group.some((item) => item.part.kind === 'turn')) {
      groups.push(group);
      group = [];
    }
    group.push({ part, index });
  });
  groups.push(group);

  return html`
    <div class="timeline">
      ${groups.map((items, groupIndex) => {
        const current = groupIndex === groups.length - 1;
        const process = items.filter((item) => isProcessPart(item.part));
        const hasTurn = items.some((item) => item.part.kind === 'turn');
        return html`
          <section class="timeline-turn-group ${current ? 'timeline-turn-group--current' : ''}">
            ${items.map(({ part, index }) => {
              if (isProcessPart(part)) {
                if (!hasTurn && process[0]?.index === index) {
                  return renderProcessRun(process, current, todos);
                }
                return nothing;
              }
              if (part.kind === 'turn') {
                return html`
                  ${renderTurn(part, current && waitingForUser)}
                  ${process.length > 0 ? renderProcessRun(process, current, todos) : nothing}
                `;
              }
              if (part.kind !== 'assistant') {
                return (PART_RENDERERS[part.kind] ?? renderUnknown)(part as never);
              }
              const turnKey = part.turnId ?? part.id;
              const showHeader = !headerTurns.has(turnKey);
              headerTurns.add(turnKey);
              return renderAssistant(
                part,
                liveText,
                showHeader,
                index === lastStreamingAssistant && !liveText,
              );
            })}
            ${
              current && liveText
                ? html`<article class="log-entry log-entry--assistant">
                    ${!hasTurn
                      ? html`<div class="assistant-message__head">
                          <span class="assistant-message__avatar" aria-hidden="true">脑</span>
                          <span class="assistant-message__name">大脑</span>
                        </div>`
                      : nothing}
                    <div class="timeline-assistant timeline-assistant--live">
                      ${unsafeHTML(md(liveText))}
                    </div>
                  </article>`
                : nothing
            }
            ${
              current && thinking
                ? renderReasoning({
                    kind: 'reasoning',
                    id: 'live-thinking',
                    text: thinking,
                    streaming: true,
                    at: '',
                  })
                : nothing
            }
            ${
              current
                ? renderTurnFooter(
                    items.map((item) => item.part),
                    sending,
                    liveText,
                    thinking,
                    waitingForUser,
                    turnActions,
                  )
                : nothing
            }
          </section>
        `;
      })}
    </div>
  `;
}
