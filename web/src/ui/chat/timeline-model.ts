import type { Question, SessionEvent, SessionEventPage, TodoItem } from '../../api/assistant.js';
import type { LiveFrame } from '../../types/session-protocol.js';

export interface TurnUsage {
  inputTokens?: number;
  cachedInputTokens?: number;
  outputTokens?: number;
  costUsd?: number;
}

export type TimelineBlock =
  | { kind: 'user'; id: string; text: string; at: string }
  | {
      kind: 'assistant';
      id: string;
      text: string;
      at: string;
      turnId?: string;
      stepId?: string;
      streaming?: boolean;
    }
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
      kind: 'question';
      id: string;
      questionId: string;
      questions: Question[];
      outcome: 'pending' | 'answered' | 'timed_out';
      at: string;
    }
  | {
      kind: 'approval';
      id: string;
      approvalId: string;
      tool: string;
      summary: string;
      outcome: 'pending' | 'resolved';
      at: string;
    }
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
  | {
      kind: 'reasoning';
      id: string;
      text: string;
      turnId?: string;
      stepId?: string;
      streaming?: boolean;
      at: string;
    }
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
  at?: string;
}

export interface PendingApproval {
  approvalId: string;
  tool: string;
  summary: string;
  turnId?: string;
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
  private todoSeq = 0;
  private pendingQuestionSeq = 0;
  private latestTurnCompletedSeq = 0;

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
    this.todoSeq = 0;
    this.pendingQuestionSeq = 0;
    this.latestTurnCompletedSeq = 0;
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

  applyLive(frame: LiveFrame): void {
    if (frame.frame === 'stream/delta') {
      const kind = frame.kind === 'reasoning' ? 'reasoning' : 'assistant';
      const id = `live:${frame.turnId}:${frame.stepId}:${kind}`;
      const existing = this.blocks.find((block) => block.id === id);
      if (existing && existing.kind === kind) {
        existing.text += frame.text;
        existing.streaming = true;
        return;
      }
      if (kind === 'reasoning') {
        this.blocks.push({
          kind: 'reasoning',
          id,
          text: frame.text,
          turnId: frame.turnId,
          stepId: frame.stepId,
          streaming: true,
          at: new Date().toISOString(),
        });
      } else {
        this.blocks.push({
          kind: 'assistant',
          id,
          text: frame.text,
          turnId: frame.turnId,
          stepId: frame.stepId,
          streaming: true,
          at: new Date().toISOString(),
        });
      }
      return;
    }
    if (frame.frame === 'stream/end') {
      const prefix = `live:${frame.turnId}:${frame.stepId}`;
      for (const block of this.blocks) {
        if (block.id.startsWith(prefix) && (block.kind === 'assistant' || block.kind === 'reasoning')) {
          block.streaming = false;
        }
      }
    }
  }

  ingestEvent(event: SessionEvent): void {
    if (this.seen.has(event.eventId)) return;
    this.seen.add(event.eventId);
    this.cursor = Math.max(this.cursor, event.seq);
    const payload = event.payloadJson ?? {};
    const at = event.occurredAt;

    switch (event.eventType) {
      case 'trigger/received': {
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
      case 'turn/start':
        this.pushBlock(
          { kind: 'turn', id: asString(payload.turnId) || event.eventId, status: 'running', at },
          event.seq,
        );
        break;
      case 'turn/end': {
        const id = asString(payload.turnId);
        const status = asString(payload.outcome);
        const mapped =
          status === 'completed' ? 'completed' : status === 'failed' ? 'failed' : 'cancelled';
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
        if (this.pendingQuestion && event.seq >= this.pendingQuestionSeq) {
          const pendingId = this.pendingQuestion.questionId;
          const pendingBlock = this.blocks.find(
            (block) => block.kind === 'question' && block.questionId === pendingId,
          );
          if (pendingBlock?.kind === 'question') pendingBlock.outcome = 'timed_out';
          this.pendingQuestion = null;
          this.pendingQuestionSeq = 0;
        }
        this.latestTurnCompletedSeq = Math.max(this.latestTurnCompletedSeq, event.seq);
        this.pendingApprovals = this.pendingApprovals.filter((approval) => approval.turnId !== id);
        break;
      }
      case 'assistant/message': {
        const turnId = asString(payload.turnId);
        const stepId = asString(payload.stepId);
        const text = asString(payload.text);
        const reasoning = asString(payload.reasoning);
        if (stepId) {
          const prefix = `live:${turnId}:${stepId}`;
          this.blocks = this.blocks.filter((block) => !block.id.startsWith(prefix));
        }
        if (reasoning) {
          const id = `reasoning-${stepId || event.eventId}`;
          const existing = this.blocks.find((block) => block.kind === 'reasoning' && block.id === id);
          if (existing?.kind === 'reasoning') {
            existing.text = reasoning;
            existing.streaming = false;
          } else {
            this.pushBlock({ kind: 'reasoning', id, text: reasoning, turnId, streaming: false, at }, event.seq);
          }
        }
        if (text) {
          const id = asString(payload.messageId) || event.eventId;
          this.pushBlock({ kind: 'assistant', id, text, at, turnId, stepId }, event.seq);
        }
        break;
      }
      case 'assistant/attempt': {
        this.pushBlock(
          {
            kind: 'retry',
            id: event.eventId,
            turnId: asString(payload.turnId),
            attempt: asNumber(payload.attempt) ?? 0,
            maxRetries: asNumber(payload.maxAttempts) ?? 0,
            delayMs: asNumber(payload.retryDelayMs) ?? 0,
            error: asString(payload.error),
            at,
          },
          event.seq,
        );
        break;
      }
      case 'user/message': {
        const text = asString(payload.text);
        if (!text) break;
        const id = asString(payload.messageId) || event.eventId;
        this.pushBlock({ kind: 'user', id, text, at }, event.seq);
        if (!this.firstUserText) this.firstUserText = text;
        break;
      }
      case 'tool/call': {
        const id = asString(payload.toolCallId) || event.eventId;
        const name = asString(payload.tool);
        if (name === 'canvas') break;
        this.pushBlock({ kind: 'tool', id, name, args: payload.args ?? null, at }, event.seq);
        break;
      }
      case 'tool/result': {
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
          this.pushBlock(
            { kind: 'tool', id, name, args: null, result, evidence, status, error, at },
            event.seq,
          );
        }
        break;
      }
      case 'skill/loaded':
        this.pushBlock(
          {
            kind: 'skill',
            id: asString(payload.skillId) || event.eventId,
            name: asString(payload.name),
            version: asString(payload.version),
            at,
          },
          event.seq,
        );
        break;
      case 'todo/write':
        if (Array.isArray(payload.todos) && event.seq >= this.todoSeq) {
          this.todoSeq = event.seq;
          this.todos = payload.todos as TodoItem[];
        }
        break;
      case 'question/asked':
        if (event.seq > this.latestTurnCompletedSeq && event.seq >= this.pendingQuestionSeq) {
          const questionId = asString(payload.questionId);
          const questions = Array.isArray(payload.questions) ? (payload.questions as Question[]) : [];
          this.pendingQuestion = { questionId, questions, at };
          this.pendingQuestionSeq = event.seq;
          this.pushBlock(
            { kind: 'question', id: questionId || event.eventId, questionId, questions, outcome: 'pending', at },
            event.seq,
          );
        }
        break;
      case 'question/answered':
        const answeredId = asString(payload.questionId);
        if (this.pendingQuestion?.questionId === answeredId && event.seq >= this.pendingQuestionSeq) {
          this.pendingQuestion = null;
          this.pendingQuestionSeq = 0;
        }
        const questionBlock = this.blocks.find(
          (block) => block.kind === 'question' && block.questionId === answeredId,
        );
        if (questionBlock?.kind === 'question') {
          questionBlock.outcome = payload.timedOut ? 'timed_out' : 'answered';
        }
        this.pushBlock(
          {
            kind: 'notice',
            id: asString(payload.questionId) || event.eventId,
            tone: payload.timedOut ? 'timed_out' : 'answered',
            text: payload.timedOut ? '等待回答超时' : '已提交回答',
            at,
          },
          event.seq,
        );
        break;
      case 'approval/requested': {
        const approvalId = asString(payload.approvalId);
        if (!this.pendingApprovals.some((pending) => pending.approvalId === approvalId)) {
          this.pendingApprovals.push({
            approvalId,
            tool: asString(payload.action),
            summary: asString(payload.summary),
            turnId: asString(payload.turnId) || undefined,
          });
        }
        this.pushBlock(
          {
            kind: 'approval',
            id: approvalId || event.eventId,
            approvalId,
            tool: asString(payload.action),
            summary: asString(payload.summary),
            outcome: 'pending',
            at,
          },
          event.seq,
        );
        break;
      }
      case 'approval/resolved': {
        const approvalId = asString(payload.approvalId);
        this.pendingApprovals = this.pendingApprovals.filter(
          (pending) => pending.approvalId !== approvalId,
        );
        const approvalBlock = this.blocks.find(
          (block) => block.kind === 'approval' && block.approvalId === approvalId,
        );
        if (approvalBlock?.kind === 'approval') approvalBlock.outcome = 'resolved';
        this.pushBlock(
          {
            kind: 'notice',
            id: approvalId || event.eventId,
            tone: 'answered',
            text: `审批：${asString(payload.resolution) || 'unknown'}`,
            at,
          },
          event.seq,
        );
        break;
      }
      case 'subagent.started': {
        this.pushBlock(
          {
            kind: 'subagent',
            id: asString(payload.subagentId) || event.eventId,
            description: asString(payload.description),
            subagentType: asString(payload.subagentType),
            status: 'running',
            at,
          },
          event.seq,
        );
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
          this.pushBlock(
            { kind: 'subagent', id: id || event.eventId, description: id, status, summary, at },
            event.seq,
          );
        }
        break;
      }
      case 'session/end':
        this.pushBlock(
          {
            kind: 'notice',
            id: event.eventId,
            tone: 'cancelled',
            text: `会话结束：${asString(payload.outcome) || 'unknown'}`,
            at,
          },
          event.seq,
        );
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
