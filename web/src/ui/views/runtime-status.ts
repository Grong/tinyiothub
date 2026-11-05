/**
 * Single source for Runtime status guards, labels and tones.
 *
 * The authoritative copy lives in `harness-status.ts`; this module only adapts
 * it to the platform Runtime status unions. Components must import these
 * helpers instead of keeping their own status maps.
 */

import type { ApprovalState, DeviceJobState, TaskStatus } from '../../types/runtime.js';
import { harnessStatusCopy, type HarnessStatusTone } from './harness-status.js';

export const TASK_STATUSES: ReadonlySet<TaskStatus> = new Set([
  'active',
  'waiting_job',
  'waiting_watch',
  'needs_approval',
  'paused',
  'cancelling',
  'completed',
  'failed',
  'cancelled',
]);

export const TERMINAL_TASK_STATUSES: ReadonlySet<TaskStatus> = new Set([
  'completed',
  'failed',
  'cancelled',
]);

export const PAUSABLE_TASK_STATUSES: ReadonlySet<TaskStatus> = new Set([
  'active',
  'waiting_job',
  'waiting_watch',
  'needs_approval',
]);

export const CANCELLABLE_TASK_STATUSES: ReadonlySet<TaskStatus> = new Set([
  'active',
  'waiting_job',
  'waiting_watch',
  'needs_approval',
  'paused',
]);

export const JOB_STATES: ReadonlySet<DeviceJobState> = new Set([
  'accepted',
  'running',
  'done',
  'failed',
  'rejected',
  'cancelled',
  'cancelled_unknown',
  'interrupted',
]);

export const APPROVAL_STATES: ReadonlySet<ApprovalState> = new Set([
  'pending',
  'approved',
  'rejected',
  'expired',
  'unavailable',
]);

export function isTaskStatus(value: string): value is TaskStatus {
  return TASK_STATUSES.has(value as TaskStatus);
}

export function isJobState(value: string): value is DeviceJobState {
  return JOB_STATES.has(value as DeviceJobState);
}

export function isApprovalState(value: string): value is ApprovalState {
  return APPROVAL_STATES.has(value as ApprovalState);
}

/** Status copy is never defined per component; it always comes from here. */
export function taskStatusLabel(status: TaskStatus): string {
  return harnessStatusCopy(status).label;
}

export function taskStatusTone(status: TaskStatus): HarnessStatusTone {
  return harnessStatusCopy(status).tone;
}
