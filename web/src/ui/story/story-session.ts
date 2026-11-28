/** 会话列表/时间文案的共享助手：story 页与星云流水页共用。 */

import type { SessionSummary } from '../../api/assistant.js';

export const SESSION_KIND_LABEL: Record<string, string> = {
  chat: '对话',
  scheduled: '巡检',
  event: '事件处置',
};

/** 默认记录：优先定时巡检里最近更新的；没有巡检再取全部里最近的。 */
export function pickDefaultSession(sessions: SessionSummary[]): SessionSummary | null {
  if (sessions.length === 0) return null;
  const scheduled = sessions.filter((session) => session.kind === 'scheduled');
  const candidates = scheduled.length > 0 ? scheduled : sessions;
  return [...candidates].sort(
    (left, right) =>
      right.updatedAt - left.updatedAt ||
      (right.status === 'active' ? 1 : 0) - (left.status === 'active' ? 1 : 0),
  )[0];
}

export function formatClock(iso: string): string {
  const date = new Date(iso);
  if (!Number.isFinite(date.getTime())) return '—';
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
}

export function formatSessionTime(ms: number): string {
  const date = new Date(ms);
  if (!Number.isFinite(date.getTime())) return '—';
  const clock = date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
  return `${date.getMonth() + 1}月${date.getDate()}日 ${clock}`;
}

export function sessionLabel(session: SessionSummary): string {
  return (
    session.label ||
    `${SESSION_KIND_LABEL[session.kind] ?? '会话'} · ${formatSessionTime(session.updatedAt)}`
  );
}
