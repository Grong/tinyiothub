import type { Question, SessionEvent, SessionEventPage, TodoItem } from '../../api/assistant.js';

export type TimelineBlock =
  | { kind: 'user'; id: string; text: string; at: string }
  | { kind: 'assistant'; id: string; text: string; at: string }
  | { kind: 'turn'; id: string; status: 'running' | 'completed' | 'failed' | 'cancelled'; at: string }
  | {
      kind: 'tool';
      id: string;
      name: string;
      args: unknown;
      result?: string;
      evidence?: unknown[];
      at: string;
    }
  | { kind: 'skill'; id: string; name: string; version: string; at: string }
  | {
      kind: 'subagent';
      id: string;
      description: string;
      subagentType?: string;
      status?: string;
      summary?: string;
      at: string;
    }
  | {
      kind: 'notice';
      id: string;
      tone: 'answered' | 'timed_out' | 'cancelled';
      text: string;
      at: string;
    };

export interface PendingQuestion {
  questionId: string;
  questions: Question[];
}

function asString(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

export class TimelineModel {
  blocks: TimelineBlock[] = [];
  todos: TodoItem[] = [];
  pendingQuestion: PendingQuestion | null = null;
  cursor = 0;
  highWatermark = 0;
  firstUserText: string | null = null;

  private seen = new Set<string>();
  private seqOf = new WeakMap<TimelineBlock, number>();

  reset(): void {
    this.blocks = [];
    this.todos = [];
    this.pendingQuestion = null;
    this.cursor = 0;
    this.highWatermark = 0;
    this.firstUserText = null;
    this.seen.clear();
    this.seqOf = new WeakMap();
  }

  /** Inserts a new block so `blocks` stays ordered by `seq`, even if events arrive out of order. */
  private pushBlock(block: TimelineBlock, seq: number): void {
    this.seqOf.set(block, seq);
    let index = this.blocks.length;
    while (index > 0) {
      const prev = this.blocks[index - 1];
      if (!prev || (this.seqOf.get(prev) ?? 0) <= seq) break;
      index -= 1;
    }
    this.blocks.splice(index, 0, block);
  }

  ingest(page: SessionEventPage): void {
    this.highWatermark = Math.max(this.highWatermark, page.highWatermark);
    const ordered = [...page.events].sort((a, b) => a.seq - b.seq);
    for (const event of ordered) this.ingestEvent(event);
  }

  ingestEvent(event: SessionEvent): void {
    if (this.seen.has(event.eventId)) return;
    this.seen.add(event.eventId);
    this.cursor = Math.max(this.cursor, event.seq);
    const payload = event.payloadJson ?? {};
    const at = event.occurredAt;

    switch (event.eventType) {
      case 'turn.started':
        this.pushBlock({ kind: 'turn', id: asString(payload.turnId) || event.eventId, status: 'running', at }, event.seq);
        break;
      case 'turn.completed': {
        const id = asString(payload.turnId);
        const status = asString(payload.outcome);
        const block = this.blocks.find((item) => item.kind === 'turn' && item.id === id);
        const mapped = status === 'completed' ? 'completed' : status === 'failed' ? 'failed' : 'cancelled';
        if (block && block.kind === 'turn') block.status = mapped;
        else this.pushBlock({ kind: 'turn', id: id || event.eventId, status: mapped, at }, event.seq);
        break;
      }
      case 'message.created': {
        const role = asString(payload.role);
        const text = asString(payload.text);
        if (!text) break;
        const id = asString(payload.messageId) || event.eventId;
        if (role === 'user') {
          this.pushBlock({ kind: 'user', id, text, at }, event.seq);
          if (!this.firstUserText) this.firstUserText = text;
        } else {
          this.pushBlock({ kind: 'assistant', id, text, at }, event.seq);
        }
        break;
      }
      case 'tool.called': {
        const id = asString(payload.toolCallId) || event.eventId;
        const name = asString(payload.tool);
        if (name === 'canvas') break;
        this.pushBlock({ kind: 'tool', id, name, args: payload.args ?? null, at }, event.seq);
        break;
      }
      case 'tool.returned': {
        const id = asString(payload.toolCallId) || event.eventId;
        const name = asString(payload.tool);
        if (name === 'canvas') break;
        const result = asString(payload.result);
        const evidence = Array.isArray(payload.evidence) ? payload.evidence : undefined;
        const block = this.blocks.find((item) => item.kind === 'tool' && item.id === id);
        if (block && block.kind === 'tool') {
          block.result = result;
          block.evidence = evidence;
        } else {
          this.pushBlock({ kind: 'tool', id, name, args: null, result, evidence, at }, event.seq);
        }
        break;
      }
      case 'skill.loaded':
        this.pushBlock({
          kind: 'skill',
          id: asString(payload.skillId) || event.eventId,
          name: asString(payload.name),
          version: asString(payload.version),
          at,
        }, event.seq);
        break;
      case 'todo.updated':
        if (Array.isArray(payload.todos)) this.todos = payload.todos as TodoItem[];
        break;
      case 'question.asked':
        this.pendingQuestion = {
          questionId: asString(payload.questionId),
          questions: Array.isArray(payload.questions) ? (payload.questions as Question[]) : [],
        };
        break;
      case 'question.answered':
        if (this.pendingQuestion?.questionId === asString(payload.questionId)) {
          this.pendingQuestion = null;
        }
        this.pushBlock({
          kind: 'notice',
          id: asString(payload.questionId) || event.eventId,
          tone: payload.timedOut ? 'timed_out' : 'answered',
          text: payload.timedOut ? '等待回答超时' : '已提交回答',
          at,
        }, event.seq);
        break;
      case 'subagent.started': {
        this.pushBlock({
          kind: 'subagent',
          id: asString(payload.subagentId) || event.eventId,
          description: asString(payload.description),
          subagentType: asString(payload.subagentType),
          status: 'running',
          at,
        }, event.seq);
        break;
      }
      case 'subagent.completed': {
        const id = asString(payload.subagentId);
        const block = this.blocks.find((item) => item.kind === 'subagent' && item.id === id);
        const status = asString(payload.status);
        const summary = asString(payload.summary);
        if (block && block.kind === 'subagent') {
          block.status = status;
          block.summary = summary;
        } else {
          this.pushBlock({ kind: 'subagent', id: id || event.eventId, description: id, status, summary, at }, event.seq);
        }
        break;
      }
      case 'session.ended':
        this.pushBlock({
          kind: 'notice',
          id: event.eventId,
          tone: 'cancelled',
          text: `会话结束：${asString(payload.outcome) || 'unknown'}`,
          at,
        }, event.seq);
        break;
      default:
        break;
    }
  }
}

export function timelineFromHistory(
  messages: Array<{ role: string; content?: Array<{ type: string; text?: string }> }>,
): TimelineBlock[] {
  const blocks: TimelineBlock[] = [];
  let index = 0;
  for (const message of messages) {
    if (message.role !== 'user' && message.role !== 'assistant') continue;
    const text = (message.content ?? [])
      .filter((block) => block.type === 'text' && block.text)
      .map((block) => block.text as string)
      .join('\n');
    if (!text) continue;
    index += 1;
    blocks.push({
      kind: message.role === 'user' ? 'user' : 'assistant',
      id: `history-${index}`,
      text,
      at: '',
    });
  }
  return blocks;
}
