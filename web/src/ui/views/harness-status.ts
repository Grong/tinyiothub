/**
 * Shared Harness status vocabulary and timestamp formatting.
 *
 * Contract (DR14=A): status is always text plus an icon. Colour is decoration
 * only. The five persistent Harness states use a fixed Chinese mapping:
 * scheduled=计划, pending=等待调度, unknown=结果未知,
 * cancelled_unknown=停止结果未知, simulation=模拟运行.
 */

import { html, type TemplateResult } from 'lit';

export type HarnessStatusTone = 'ok' | 'warn' | 'danger' | 'accent' | 'muted';

export interface HarnessStatusEntry {
  label: string;
  icon: string;
  tone: HarnessStatusTone;
}

const statusEntries = {
  scheduled: { label: '计划', icon: '◷', tone: 'accent' },
  pending: { label: '等待调度', icon: '◷', tone: 'warn' },
  unknown: { label: '结果未知', icon: '?', tone: 'warn' },
  cancelled_unknown: { label: '停止结果未知', icon: '⚠', tone: 'warn' },
  simulation: { label: '模拟运行', icon: '▶', tone: 'warn' },

  active: { label: '进行中', icon: '●', tone: 'accent' },
  running: { label: '进行中', icon: '●', tone: 'accent' },
  waiting_job: { label: '等待设备', icon: '◷', tone: 'accent' },
  waiting_watch: { label: '等待观察', icon: '◷', tone: 'accent' },
  needs_approval: { label: '等待审批', icon: '⚠', tone: 'warn' },
  paused: { label: '已暂停', icon: '⏸', tone: 'muted' },
  cancelling: { label: '取消中', icon: '…', tone: 'warn' },
  completed: { label: '已完成', icon: '✓', tone: 'ok' },
  failed: { label: '失败', icon: '✕', tone: 'danger' },
  cancelled: { label: '已取消', icon: '⊘', tone: 'muted' },
  enabled: { label: '已启用', icon: '●', tone: 'ok' },
  disabled: { label: '未启用', icon: '○', tone: 'muted' },
} as const satisfies Record<string, HarnessStatusEntry>;

/** Fixed public mapping. Tests extend this map whenever a status is added. */
export const HarnessStatusCopy: Readonly<Record<string, HarnessStatusEntry>> = statusEntries;

const fallback: HarnessStatusEntry = { label: '未知状态', icon: '?', tone: 'muted' };

export function harnessStatusCopy(status: string): HarnessStatusEntry {
  return HarnessStatusCopy[status] ?? fallback;
}

export function renderHarnessStatus(status: string): TemplateResult {
  const copy = harnessStatusCopy(status);
  return html`<span class="harness-status harness-status--${copy.tone}" data-status=${status}>
    <span class="harness-status__icon" aria-hidden="true">${copy.icon}</span>
    <span class="harness-status__label">${copy.label}</span>
  </span>`;
}

const displayTimeZone =
  Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';

/** Explicit timezone, never a bare local clock. */
export function currentTimeZone(): string {
  return displayTimeZone;
}

export function formatHarnessTime(value: string | null): string {
  if (!value) return '暂无';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    timeZoneName: 'short',
  }).format(date);
}

/**
 * Keep a single polite announcement for repeated identical messages; callers
 * store the result in one `role=status` region.
 */
export function nextAnnouncement(previous: string, message: string): string {
  const next = message.trim();
  if (!next) return previous;
  return next === previous ? previous : next;
}
