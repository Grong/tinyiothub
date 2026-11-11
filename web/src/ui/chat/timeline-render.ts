import { html, nothing, type TemplateResult } from 'lit';
import { unsafeHTML } from 'lit/directives/unsafe-html.js';
import { md } from '../shared/markdown.js';
import type { ContentPart } from './timeline-model.js';
import type { TodoItem } from '../../api/assistant.js';

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

export function toolSummary(block: ToolPart): string {
  const arg = firstValue(block.args);
  const verb = TOOL_META[block.name]?.verb ?? block.name;
  const head = arg ? `${verb} ${arg}` : verb;
  const output =
    block.status === 'failed' && block.error
      ? block.error
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

function renderTurn(part: Extract<ContentPart, { kind: 'turn' }>): TemplateResult {
  const time = formatAt(part.at);
  const meta = [
    part.toolCalls ? `${part.toolCalls} 次工具` : '',
    formatTokens((part.usage?.inputTokens ?? 0) + (part.usage?.outputTokens ?? 0))
      ? `${formatTokens((part.usage?.inputTokens ?? 0) + (part.usage?.outputTokens ?? 0))} tokens`
      : '',
    formatDuration(part.at, part.completedAt),
  ]
    .filter(Boolean)
    .join(' · ');
  return html`
    <div class="timeline-turn timeline-turn--${part.status}">
      <time datetime=${part.at}>${time || '—'}</time>
      <span class="timeline-turn__rule" aria-hidden="true"></span>
      <span class="timeline-turn__tail">
        ${meta ? html`<span class="timeline-turn__meta">${meta}</span>` : nothing}
        <span class="timeline-turn__state">${TURN_STATE[part.status] ?? part.status}</span>
      </span>
    </div>
    ${part.error ? html`<div class="timeline-turn__error">${part.error}</div>` : nothing}
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
  return html`<article class="log-entry log-entry--assistant">
    ${
      showHeader
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
    <div class="activity__detail">${part.text}</div>
  </details>`;
}

function renderTool(part: ToolPart, compact = false): TemplateResult {
  const meta = TOOL_META[part.name];
  const icon = meta?.icon ?? '⚙';
  const failed = part.status === 'failed';
  const hideMeta = compact && !failed && Boolean(part.result);
  const stateClass = failed
    ? 'activity__meta--failed'
    : part.result
      ? 'activity__meta--ok'
      : 'activity__meta--pending';
  return html`<details
    class="activity activity--tool log-entry ${failed ? 'activity--failed' : ''}"
  >
    <summary class="activity__summary">
      <span class="activity__icon" aria-hidden="true">${icon}</span>
      <span class="activity__label">${toolSummary(part)}</span>
      ${
        hideMeta
          ? nothing
          : html`<span class="activity__meta ${stateClass}"
              >${failed ? '失败' : part.result ? '完成' : '执行中'}</span
            >`
      }
    </summary>
    <div class="activity__detail">
      <div class="activity__detail-label">参数</div>
      <pre>${JSON.stringify(part.args ?? {}, null, 2)}</pre>
      ${
        part.result
          ? html`<div class="activity__detail-label">结果</div>
              <pre>${part.result}</pre>`
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

function renderToolRun(items: Array<{ part: ContentPart; index: number }>): TemplateResult {
  const compact = items.length > 1;
  return html`<div class="timeline-tools">
    ${compact ? html`<div class="timeline-tools__count">${items.length} 次工具调用</div>` : nothing}
    ${items.map(({ part }) => renderTool(part as ToolPart, compact))}
  </div>`;
}

function renderSubagent(part: Extract<ContentPart, { kind: 'subagent' }>): TemplateResult {
  const status = part.status ?? 'running';
  return html`<details class="activity activity--subagent log-entry">
    <summary class="activity__summary">
      <span class="activity__icon" aria-hidden="true">⧉</span>
      <span class="activity__label">子代理 · ${part.description || part.id}</span>
      <span class="activity__meta">${SUBAGENT_STATUS[status] ?? status}</span>
    </summary>
    <div class="activity__detail">
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

export function renderTodoPanel(todos: TodoItem[]): TemplateResult | typeof nothing {
  if (todos.length === 0) return nothing;
  const { done, total } = todoProgress(todos);
  const percent = total === 0 ? 0 : Math.round((done / total) * 100);
  return html`
    <section class="timeline-todos" aria-label="本轮计划">
      <div class="timeline-todos__head">
        <span>本轮计划</span>
        <span class="timeline-todos__count">${done}/${total}</span>
      </div>
      <div class="timeline-todos__meter" aria-hidden="true">
        <span style=${`width:${percent}%`}></span>
      </div>
      <ol class="timeline-todos__list">
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
      </ol>
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

function renderTrailingStatus(
  parts: ContentPart[],
  sending: boolean,
  liveText: string,
  thinking: string,
): TemplateResult | typeof nothing {
  const lastTurn = [...parts].reverse().find((part) => part.kind === 'turn');
  const running = sending || (lastTurn?.kind === 'turn' && lastTurn.status === 'running');
  if (running) {
    const elapsed = lastTurn ? formatDuration(lastTurn.at, new Date().toISOString()) : '';
    return activity(
      html`
        <span class="activity__icon" aria-hidden="true">◌</span>
        <span class="activity__label">${currentWork(parts, liveText, thinking)}</span>
        <span class="activity__meta">${elapsed ? `已处理 ${elapsed}` : '进行中'}</span>
      `,
      'activity--current',
    );
  }
  if (lastTurn?.kind === 'turn' && lastTurn.completedAt) {
    const label =
      lastTurn.status === 'completed'
        ? '已完成'
        : lastTurn.status === 'failed'
          ? '执行失败'
          : '已取消';
    return activity(
      html`
        <span class="activity__icon" aria-hidden="true"
          >${lastTurn.status === 'completed' ? '✓' : '!'}</span
        >
        <span class="activity__label">${label}</span>
        <span class="activity__meta">${formatDuration(lastTurn.at, lastTurn.completedAt)}</span>
      `,
      `activity--current ${lastTurn.status === 'completed' ? 'activity--done' : 'activity--failed'}`,
    );
  }
  return nothing;
}

export function renderTimeline(
  parts: ContentPart[],
  opts: { liveText?: string; thinking?: string; sending?: boolean; todos?: TodoItem[] } = {},
): TemplateResult {
  const { liveText = '', thinking = '', sending = false, todos = [] } = opts;
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
        const consumed = new Set<number>();
        return html`
          <section class="timeline-turn-group ${current ? 'timeline-turn-group--current' : ''}">
            ${items.map(({ part, index }) => {
              if (consumed.has(index)) return nothing;
              if (part.kind === 'tool') {
                const run: Array<{ part: ToolPart; index: number }> = [{ part, index }];
                let next = index + 1;
                while (items[next]?.part.kind === 'tool') {
                  const item = items[next];
                  if (item.part.kind === 'tool') run.push({ part: item.part, index: item.index });
                  next += 1;
                }
                for (const item of run) consumed.add(item.index);
                return renderToolRun(run);
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
                    <div class="assistant-message__head">
                      <span class="assistant-message__avatar" aria-hidden="true">脑</span>
                      <span class="assistant-message__name">大脑</span>
                    </div>
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
            ${current ? renderTodoPanel(todos) : nothing}
            ${current ? renderTrailingStatus(parts, sending, liveText, thinking) : nothing}
          </section>
        `;
      })}
    </div>
  `;
}
