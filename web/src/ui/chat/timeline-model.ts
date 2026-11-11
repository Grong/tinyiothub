import type { Question, SessionEvent, SessionEventPage, TodoItem } from '../../api/assistant.js';

export interface TurnUsage {
  inputTokens?: number;
  cachedInputTokens?: number;
  outputTokens?: number;
  costUsd?: number;
}

export type TimelineBlock =
  | { kind: 'user'; id: string; text: string; at: string }
  | { kind: 'assistant'; id: string; text: string; at: string; turnId?: string; streaming?: boolean }
  | {
      kind: 'turn';
      id: string;
      status: 'running' | 'completed' | 'failed' | 'cancelled';
      at: string;
      completedAt?: string;
      toolCalls?: number;
      usage?: TurnUsage;
      error?: string;
      source?: string;
    }
  | {
      kind: 'retry';
      id: string;
      turnId: string;
      attempt: number;
      maxRetries: number;
      delayMs: number;
      error: string;
      at: string;
    }
  | { kind: 'origin'; id: string; source: string; sourceId?: string; at: string }
  | {
      kind: 'tool';
      id: string;
      name: string;
      args: unknown;
      result?: string;
      evidence?: unknown[];
      status?: 'ok' | 'failed';
      error?: string;
      at: string;
    }
  | { kind: 'skill'; id: string; name: string; version: string; at: string }
  | { kind: 'reasoning'; id: string; text: string; turnId?: string; streaming?: boolean; at: string }
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

export interface PendingApproval {
  approvalId: string;
  tool: string;
  summary: string;
}

function asString(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function asNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function usageFrom(value: unknown): TurnUsage | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const usage = value as Record<string, unknown>;
  const result: TurnUsage = {
    inputTokens: asNumber(usage.inputTokens),
    cachedInputTokens: asNumber(usage.cachedInputTokens),
    outputTokens: asNumber(usage.outputTokens),
    costUsd: asNumber(usage.costUsd),
  };
  return Object.values(result).some((item) => item !== undefined) ? result : undefined;
}

/** Tool payloads only carry a string; surface the structured failure when present. */
export function toolFailure(result?: string): string | undefined {
  if (!result) return undefined;
  try {
    const parsed = JSON.parse(result) as unknown;
    if (parsed && typeof parsed === 'object') {
      const record = parsed as Record<string, unknown>;
      if (record.success === false) {
        return asString(record.error) || '工具执行失败';
      }
      if (typeof record.error === 'string' && record.error.trim()) {
        return record.error;
      }
    }
  } catch {
    // Plain text results are common; fall through to the prefix check.
  }
  const text = result.trim();
  return /^(error|failed|失败)[:：]/i.test(text) ? text.slice(0, 200) : undefined;
}

export type ContentPart = TimelineBlock;

export class TimelineModel {
  blocks: TimelineBlock[] = [];
  /** Generic ordered content parts consumed by the renderer registry. */
  get parts(): ContentPart[] {
    return this.blocks;
  }
  todos: TodoItem[] = [];
  pendingQuestion: PendingQuestion | null = null;
  pendingApprovals: PendingApproval[] = [];
  cursor = 0;
  highWatermark = 0;
  firstUserText: string | null = null;

  private seen = new Set<string>();
  private seqOf = new WeakMap<TimelineBlock, number>();

  reset(): void {
    this.blocks = [];
    this.todos = [];
    this.pendingQuestion = null;
    this.pendingApprovals = [];
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
      case 'trigger.received': {
        const source = asString(payload.trigger) || 'event';
        this.pushBlock(
          {
            kind: 'origin',
            id: event.eventId,
            source,
            sourceId: asString(payload.sourceId) || undefined,
            at,
          },
          event.seq,
        );
        break;
      }
      case 'turn.started':
        this.pushBlock({ kind: 'turn', id: asString(payload.turnId) || event.eventId, status: 'running', at }, event.seq);
        break;
      case 'turn.completed': {
        const id = asString(payload.turnId);
        const status = asString(payload.outcome);
        const mapped = status === 'completed' ? 'completed' : status === 'failed' ? 'failed' : 'cancelled';
        const block = this.blocks.find((item) => item.kind === 'turn' && item.id === id);
        const patch = {
          status: mapped,
          completedAt: at,
          toolCalls: asNumber(payload.toolCalls),
          usage: usageFrom(payload.usage),
          error: asString(payload.error) || undefined,
          source: asString(payload.source) || undefined,
        } as const;
        if (block && block.kind === 'turn') Object.assign(block, patch);
        else this.pushBlock({ kind: 'turn', id: id || event.eventId, at, ...patch }, event.seq);
        for (const item of this.blocks) {
          if ((item.kind === 'reasoning' || item.kind === 'assistant') && item.turnId === id) {
            item.streaming = false;
          }
        }
        break;
      }
      case 'turn.retry': {
        const turnId = asString(payload.turnId);
        while (this.blocks.length > 0) {
          const last = this.blocks[this.blocks.length - 1];
          const isFailedAttemptContent =
            (last?.kind === 'assistant' || last?.kind === 'reasoning') &&
            last.turnId === turnId &&
            last.streaming === true;
          if (!isFailedAttemptContent) break;
          this.blocks.pop();
        }
        this.pushBlock(
          {
            kind: 'retry',
            id: event.eventId,
            turnId,
            attempt: asNumber(payload.attempt) ?? 0,
            maxRetries: asNumber(payload.maxRetries) ?? 0,
            delayMs: asNumber(payload.delayMs) ?? 0,
            error: asString(payload.error) || '模型请求失败',
            at,
          },
          event.seq,
        );
        break;
      }
      case 'step.thinking': {
        const turnId = asString(payload.turnId);
        const text = asString(payload.thinking);
        if (!text) break;
        const last = this.blocks[this.blocks.length - 1];
        if (last?.kind === 'reasoning' && last.turnId === turnId) {
          last.text += text;
          last.streaming = true;
        } else {
          this.pushBlock(
            {
              kind: 'reasoning',
              id: `thinking-${turnId}`,
              text,
              turnId,
              streaming: true,
              at,
            },
            event.seq,
          );
        }
        break;
      }
      case 'step.text': {
        const turnId = asString(payload.turnId);
        const text = asString(payload.text);
        if (!text) break;
        const last = this.blocks[this.blocks.length - 1];
        if (last?.kind === 'assistant' && last.streaming && last.turnId === turnId) {
          last.text += text;
        } else {
          this.pushBlock(
            { kind: 'assistant', id: `stream-${event.eventId}`, text, at, turnId, streaming: true },
            event.seq,
          );
        }
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
          const turnId = asString(payload.turnId);
          const streamed = this.blocks.filter(
            (block) => block.kind === 'assistant' && block.streaming && block.turnId === turnId,
          );
          if (streamed.length > 0) {
            // step.text already produced the chronological text segments;
            // finalizing them avoids duplicating the full turn text.
            for (const block of streamed) {
              if (block.kind === 'assistant') block.streaming = false;
            }
          } else {
            this.pushBlock({ kind: 'assistant', id, text, at, turnId }, event.seq);
          }
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
        const error = toolFailure(result);
        const status = error ? 'failed' : 'ok';
        const block = this.blocks.find((item) => item.kind === 'tool' && item.id === id);
        if (block && block.kind === 'tool') {
          block.result = result;
          block.evidence = evidence;
          block.status = status;
          block.error = error;
        } else {
          this.pushBlock({ kind: 'tool', id, name, args: null, result, evidence, status, error, at }, event.seq);
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
      case 'approval.requested': {
        const approvalId = asString(payload.approvalId);
        if (!this.pendingApprovals.some((pending) => pending.approvalId === approvalId)) {
          this.pendingApprovals.push({
            approvalId,
            tool: asString(payload.action),
            summary: asString(payload.summary),
          });
        }
        break;
      }
      case 'approval.resolved': {
        const approvalId = asString(payload.approvalId);
        this.pendingApprovals = this.pendingApprovals.filter(
          (pending) => pending.approvalId !== approvalId,
        );
        this.pushBlock({
          kind: 'notice',
          id: approvalId || event.eventId,
          tone: 'answered',
          text: `审批：${asString(payload.resolution) || 'unknown'}`,
          at,
        }, event.seq);
        break;
      }
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
