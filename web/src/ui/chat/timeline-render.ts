import { html, nothing, type TemplateResult } from 'lit';
import { unsafeHTML } from 'lit/directives/unsafe-html.js';
import { md } from '../shared/markdown.js';
import type { TimelineBlock } from './timeline-model.js';
import type { TodoItem } from '../../api/assistant.js';

const TOOL_ICONS: Record<string, string> = {
  get_thing: '◎',
  search_things: '⌕',
  read_properties: '◫',
  write_properties: '✎',
  send_command: '▸',
  get_device_history: '◷',
  list_alarms: '⚠',
  acknowledge_alarm: '✓',
  resolve_alarm: '✓',
  query_data: '⌕',
  aggregate_data: 'Σ',
  todo_write: '☑',
  ask_user: '?',
  spawn_subagent: '⧉',
};

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

export function toolSummary(block: Extract<TimelineBlock, { kind: 'tool' }>): string {
  const arg = firstValue(block.args);
  const head = `${block.name}(${arg})`;
  if (!block.result) return head;
  const result = block.result.replace(/\s+/g, ' ').trim().slice(0, 60);
  return `${head} → ${result}`;
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

export function renderTodoPanel(todos: TodoItem[]): TemplateResult | typeof nothing {
  if (todos.length === 0) return nothing;
  return html`
    <section class="timeline-todos" aria-label="任务清单">
      <div class="timeline-todos__head">${summarizeTodos(todos)}</div>
      <ol class="timeline-todos__list">
        ${todos.map(
          (todo) => html`
            <li class="timeline-todo timeline-todo--${todo.status}">
              <span class="timeline-todo__mark" aria-hidden="true">
                ${todo.status === 'completed' ? '✓' : todo.status === 'in_progress' ? '▣' : '○'}
              </span>
              <span class="timeline-todo__text">${todo.content}</span>
            </li>
          `,
        )}
      </ol>
    </section>
  `;
}

function renderTool(block: Extract<TimelineBlock, { kind: 'tool' }>): TemplateResult {
  return html`
    <details class="timeline-tool">
      <summary class="timeline-tool__summary">
        <span class="timeline-tool__icon" aria-hidden="true">${Object.hasOwn(TOOL_ICONS, block.name) ? TOOL_ICONS[block.name] : '⚙'}</span>
        <span class="timeline-tool__text">${toolSummary(block)}</span>
        <span
          class="timeline-tool__state ${block.result ? 'timeline-tool__state--ok' : 'timeline-tool__state--pending'}"
          >${block.result ? '完成' : '执行中'}</span
        >
      </summary>
      <div class="timeline-tool__body">
        <div class="timeline-tool__label">参数</div>
        <pre class="timeline-tool__pre">${JSON.stringify(block.args ?? {}, null, 2)}</pre>
        ${
          block.result
            ? html`
                <div class="timeline-tool__label">结果</div>
                <pre class="timeline-tool__pre">${block.result}</pre>
              `
            : nothing
        }
      </div>
    </details>
  `;
}

function renderSubagent(block: Extract<TimelineBlock, { kind: 'subagent' }>): TemplateResult {
  const status = block.status ?? 'running';
  return html`
    <details class="timeline-subagent">
      <summary class="timeline-subagent__summary">
        <span class="timeline-subagent__icon" aria-hidden="true">⧉</span>
        <span class="timeline-subagent__title">子代理：${block.description || block.id}</span>
        <span class="timeline-subagent__status timeline-subagent__status--${status}"
          >${status}</span
        >
      </summary>
      <div class="timeline-subagent__body">
        ${block.subagentType ? html`<div class="timeline-subagent__meta">类型：${block.subagentType}</div>` : nothing}
        ${
          block.summary
            ? html`<div class="timeline-subagent__summary-text">
                ${unsafeHTML(md(block.summary))}
              </div>`
            : nothing
        }
      </div>
    </details>
  `;
}

export function renderTimeline(
  blocks: TimelineBlock[],
  opts: { liveText?: string; sending?: boolean } = {},
): TemplateResult {
  const { liveText = '', sending = false } = opts;
  return html`
    <div class="timeline">
      ${blocks.map((block) => {
        switch (block.kind) {
          case 'user':
            return html`<div class="timeline-user">
              <div class="timeline-user__bubble">${block.text}</div>
            </div>`;
          case 'assistant':
            return html`<article class="timeline-assistant">
              ${unsafeHTML(md(block.text))}
            </article>`;
          case 'turn':
            return html`<div
              class="timeline-turn timeline-turn--${block.status}"
              aria-hidden="true"
            ></div>`;
          case 'tool':
            return renderTool(block);
          case 'skill':
            return html`<div class="timeline-skill">
              已加载「${block.name}」技能${block.version ? ` · ${block.version}` : ''}
            </div>`;
          case 'subagent':
            return renderSubagent(block);
          case 'notice':
            return html`<div class="timeline-notice timeline-notice--${block.tone}">
              ${block.text}
            </div>`;
          default:
            return nothing;
        }
      })}
      ${
        liveText
          ? html`<article class="timeline-assistant timeline-assistant--live">
              ${unsafeHTML(md(liveText))}
            </article>`
          : nothing
      }
      ${sending && !liveText ? html`<div class="timeline-dots" aria-label="正在思考"><span></span><span></span><span></span></div>` : nothing}
    </div>
  `;
}
