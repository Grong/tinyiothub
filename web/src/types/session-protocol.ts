/**
 * Session protocol v2 wire contract.
 *
 * Durable `SessionEvent` values are the only replayable timeline. `LiveFrame`
 * values are transient SSE frames used only for the current connection; a
 * reconnect recovers the final content from `assistant/message`.
 */

export const SESSION_PROTOCOL_VERSION = 2 as const;

export const SESSION_EVENT_TYPES = [
  'turn/start',
  'step/start',
  'tool/call',
  'tool/result',
  'assistant/message',
  'assistant/attempt',
  'step/end',
  'turn/end',
  'todo/write',
  'skill/loaded',
  'skill/completed',
  'question/asked',
  'question/answered',
  'approval/requested',
  'approval/resolved',
  'subagent/started',
  'subagent/completed',
  'trigger/received',
  'session/start',
  'session/end',
] as const;

export type SessionEventType = (typeof SESSION_EVENT_TYPES)[number];

export type StreamKind = 'text' | 'reasoning';

export interface AssistantStreamSegment {
  kind: StreamKind;
  /** Milliseconds since this model attempt started. */
  atMs: number;
  text: string;
}

export interface TurnUsage {
  inputTokens?: number;
  cachedInputTokens?: number;
  outputTokens?: number;
  costUsd?: number;
}

export interface SessionTodo {
  content: string;
  status: 'pending' | 'in_progress' | 'completed';
  activeForm?: string;
}

export interface SessionQuestion {
  question: string;
  header: string;
  options: Array<{ label: string; description: string }>;
  multiSelect: boolean;
}

export interface SessionEventDataMap {
  'turn/start': { trigger?: string };
  'step/start': { index: number };
  'tool/call': { callId: string; name: string; args: unknown };
  'tool/result': {
    callId: string;
    name: string;
    result: string;
    evidence?: unknown[];
    isError?: boolean;
    durationMs?: number;
  };
  'assistant/message': {
    messageId: string;
    text: string;
    reasoning: string;
    stream: AssistantStreamSegment[];
    usage?: TurnUsage;
  };
  'assistant/attempt': {
    attempt: number;
    maxAttempts: number;
    stream: AssistantStreamSegment[];
    error?: string;
    retryDelayMs?: number;
  };
  'step/end': { outcome: string; usage?: TurnUsage };
  'turn/end': { outcome: string; reason?: string; usage?: TurnUsage };
  'todo/write': { todos: SessionTodo[] };
  'skill/loaded': { skillId: string; name: string; version: string };
  'skill/completed': {
    skillId: string;
    status: 'completed' | 'skipped' | 'blocked';
    summary: string;
    nextSkill?: string | null;
  };
  'question/asked': { questionId: string; questions: SessionQuestion[] };
  'question/answered': {
    questionId: string;
    timedOut: boolean;
    answers: Array<{
      questionIndex: number;
      selected: string[];
      text?: string | null;
    }>;
  };
  'approval/requested': { approvalId: string; action: string; summary: string };
  'approval/resolved': { approvalId: string; resolution: string };
  'subagent/started': { subagentId: string; description: string; subagentType?: string };
  'subagent/completed': {
    subagentId: string;
    status: string;
    summary?: string;
  };
  'trigger/received': { trigger: string; sourceId?: string };
  'session/start': { kind: string };
  'session/end': { outcome: string };
}

export type SessionEvent = {
  [K in SessionEventType]: {
    v: typeof SESSION_PROTOCOL_VERSION;
    seq: number;
    eventId: string;
    sessionId: string;
    type: K;
    at: string;
    turnId?: string;
    stepId?: string;
    data: SessionEventDataMap[K];
  };
}[SessionEventType];

export type TurnPhase =
  | 'thinking'
  | 'requesting'
  | 'tool'
  | 'waiting_user'
  | 'verifying'
  | 'done'
  | 'retrying';

export interface StreamStartFrame {
  v: typeof SESSION_PROTOCOL_VERSION;
  frame: 'stream/start';
  streamId: string;
  turnId: string;
  stepId: string;
  attempt: number;
}

export interface StreamDeltaFrame {
  v: typeof SESSION_PROTOCOL_VERSION;
  frame: 'stream/delta';
  streamId: string;
  turnId: string;
  stepId: string;
  index: number;
  kind: StreamKind;
  text: string;
}

export interface StreamEndFrame {
  v: typeof SESSION_PROTOCOL_VERSION;
  frame: 'stream/end';
  turnId: string;
  stepId: string;
  status: string;
}

export interface StreamSnapshotFrame {
  v: typeof SESSION_PROTOCOL_VERSION;
  frame: 'stream/snapshot';
  streamId: string;
  turnId: string;
  stepId: string;
  status: 'running' | 'completed' | 'failed';
  segments: AssistantStreamSegment[];
}

export interface StatusFrame {
  v: typeof SESSION_PROTOCOL_VERSION;
  frame: 'status';
  turnId: string;
  stepId?: string;
  phase: TurnPhase;
  detail?: string;
}

export interface ToolProgressFrame {
  v: typeof SESSION_PROTOCOL_VERSION;
  frame: 'tool/progress';
  turnId: string;
  stepId: string;
  callId: string;
  phase: string;
  message: string;
  percent?: number;
}

export interface ResyncFrame {
  v: typeof SESSION_PROTOCOL_VERSION;
  frame: 'resync';
  reason: string;
}

export type LiveFrame =
  | StreamStartFrame
  | StreamDeltaFrame
  | StreamEndFrame
  | StreamSnapshotFrame
  | StatusFrame
  | ToolProgressFrame
  | ResyncFrame;

export type HarnessFrame =
  | {
      v: typeof SESSION_PROTOCOL_VERSION;
      frame: 'event';
      event: SessionEvent;
    }
  | LiveFrame;
