import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client.js';
import { decideRuntimeApproval, getRuntimeTask } from '../../api/runtime.js';
import type { RuntimeApproval, RuntimeApprovalDecisionResult } from '../../types/runtime.js';
import type { RuntimeProjectedApproval } from './runtime-projection.js';
import { safeJson } from './runtime-approval.js';
import './runtime-approval.js';
import type { RuntimeApprovalView } from './runtime-approval.js';

vi.mock('../../api/runtime.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/runtime.js')>();
  return { ...actual, decideRuntimeApproval: vi.fn(), getRuntimeTask: vi.fn() };
});

const decide = vi.mocked(decideRuntimeApproval);
const get = vi.mocked(getRuntimeTask);

function pendingApproval(over: Partial<RuntimeApproval> = {}): RuntimeApproval {
  return {
    id: 'approval_1', workspaceId: 'ws_1', taskId: 'task_1', runId: 'run_1',
    toolCallId: 'call_1', deviceId: 'dev_1', tool: 'set_device', argsJson: { value: 21, mode: 'manual' },
    reason: 'Raise the pump', state: 'pending', decision: null, deciderId: null,
    requestedAt: '2026-10-27T10:00:00Z', decidedAt: null, expiresAt: '2026-10-27T10:05:00Z',
    decisionEventId: 'decision_event_1', updatedAt: '2026-10-27T10:00:00Z',
    ...over,
  };
}

function approvedApproval(): RuntimeApproval {
  return pendingApproval({
    state: 'approved', decision: 'approve', deciderId: 'admin_1', decidedAt: '2026-10-27T10:03:00Z',
  });
}

function rejectedApproval(): RuntimeApproval {
  return pendingApproval({
    state: 'rejected', decision: 'reject', deciderId: 'admin_1', decidedAt: '2026-10-27T10:03:00Z',
  });
}

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void; reject: (error: unknown) => void } {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

function decisionResult(approval: RuntimeApproval, deduplicated = false): { code: number; msg: string; result: RuntimeApprovalDecisionResult } {
  return { code: 0, msg: '', result: { approval, deduplicated } };
}

interface ApprovalFixture {
  readonly element: RuntimeApprovalView;
  readonly text: string;
  readonly args: HTMLElement | null;
  readonly approveButton: HTMLButtonElement | undefined;
  readonly rejectButton: HTMLButtonElement | undefined;
  readonly executableControls: HTMLElement[];
  readonly projectionEvents: unknown[];
  readonly decisionApi: typeof decide;
  readonly refreshApi: typeof get;
  readonly switchTask: (taskId: string) => Promise<void>;
}

async function approvalFixture(approval: RuntimeProjectedApproval): Promise<ApprovalFixture> {
  const element = document.createElement('runtime-approval') as RuntimeApprovalView;
  element.workspaceId = 'ws_1';
  element.taskId = 'task_1';
  element.approval = approval;
  const projectionEvents: unknown[] = [];
  element.addEventListener('projection-change', (event) => {
    projectionEvents.push((event as CustomEvent<unknown>).detail);
  });
  element.addEventListener('refresh-request', () => {
    void get('ws_1', 'task_1');
  });
  document.body.append(element);
  await element.updateComplete;
  return {
    element,
    get text() { return element.textContent ?? ''; },
    get args() { return element.querySelector<HTMLElement>('[data-approval-args]'); },
    get approveButton() {
      return element.querySelector<HTMLButtonElement>('button[data-action="approve"]') ?? undefined;
    },
    get rejectButton() {
      return element.querySelector<HTMLButtonElement>('button[data-action="reject"]') ?? undefined;
    },
    get executableControls() { return [...element.querySelectorAll<HTMLElement>('[data-executable]')]; },
    projectionEvents,
    decisionApi: decide,
    refreshApi: get,
    switchTask: async (taskId) => {
      element.taskId = taskId;
      await element.updateComplete;
    },
  };
}

function withDeferredDecision() {
  const pending = deferred<{ code: number; msg: string; result: RuntimeApprovalDecisionResult }>();
  decide.mockReturnValue(pending.promise);
  return pending;
}

async function flush(element: RuntimeApprovalView): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await element.updateComplete;
}

beforeEach(() => {
  decide.mockReset();
  get.mockReset();
  get.mockResolvedValue({ code: 0, msg: '', result: null });
});

afterEach(() => {
  document.body.innerHTML = '';
  vi.clearAllMocks();
});

describe('runtime-approval', () => {
  it('approves once with server identity and disables repeat submission', async () => {
    const pending = withDeferredDecision();
    const view = await approvalFixture(pendingApproval());

    view.approveButton!.click();
    view.approveButton!.click();
    await view.element.updateComplete;

    expect(decide).toHaveBeenCalledOnce();
    expect(decide).toHaveBeenCalledWith('ws_1', 'task_1', 'approval_1', 'approve', expect.any(AbortSignal));
    expect(decide.mock.calls[0]).toHaveLength(5);
    expect(view.approveButton!.disabled).toBe(true);
    expect(view.rejectButton!.disabled).toBe(true);
    expect(view.text).toContain('Approving');
    expect(view.text).not.toContain('Approved');

    pending.resolve(decisionResult(approvedApproval()));
    await flush(view.element);

    expect(view.text).toContain('Approved');
    expect(view.text).toContain('admin_1');
    expect(view.approveButton).toBeUndefined();
    expect(view.rejectButton).toBeUndefined();
    expect(get).toHaveBeenCalledOnce();
  });

  it('rejects once and shows the server-returned rejected state', async () => {
    const pending = withDeferredDecision();
    const view = await approvalFixture(pendingApproval());

    view.rejectButton!.click();
    view.rejectButton!.click();
    await view.element.updateComplete;

    expect(decide).toHaveBeenCalledOnce();
    expect(decide).toHaveBeenCalledWith('ws_1', 'task_1', 'approval_1', 'reject', expect.any(AbortSignal));
    expect(view.text).toContain('Rejecting');
    expect(view.text).not.toContain('Rejected');

    pending.resolve(decisionResult(rejectedApproval()));
    await flush(view.element);

    expect(view.text).toContain('Rejected');
    expect(view.approveButton).toBeUndefined();
    expect(view.rejectButton).toBeUndefined();
  });

  it('treats a deduplicated same-decision replay as the authoritative approved result', async () => {
    const pending = withDeferredDecision();
    const view = await approvalFixture(pendingApproval());

    view.approveButton!.click();
    await view.element.updateComplete;
    pending.resolve(decisionResult(approvedApproval(), true));
    await flush(view.element);

    expect(view.text).toContain('Approved');
    expect(view.text).not.toContain('Approving');
    expect(view.executableControls).toEqual([]);
  });

  it('shows frozen arguments verbatim and refreshes after a decision conflict', async () => {
    const approval = pendingApproval();
    const view = await approvalFixture(approval);
    expect(view.args!.textContent).toBe(safeJson(approval.argsJson));
    expect(view.text).toContain('set_device');
    expect(view.text).toContain('dev_1');
    expect(view.text).toContain('Raise the pump');
    expect(view.text).toContain(approval.expiresAt);
    expect(view.text).not.toContain('decision_event_1');

    decide.mockRejectedValue(new ApiError(409, 'Approval already decided'));
    view.approveButton!.click();
    await flush(view.element);

    expect(get).toHaveBeenCalledOnce();
    expect(view.text).toContain('Approval already decided');
    expect(view.approveButton!.disabled).toBe(false);
    expect(view.rejectButton!.disabled).toBe(false);

    const sent = decide.mock.calls[0];
    expect(sent[0]).toBe('ws_1');
    expect(sent[1]).toBe('task_1');
    expect(sent[2]).toBe('approval_1');
    expect(sent[3]).toBe('approve');
  });

  it.each(['rejected', 'expired', 'unavailable'] as const)('renders %s without actions', async (state) => {
    const view = await approvalFixture(pendingApproval({ state }));

    expect(view.executableControls).toEqual([]);
    expect(view.text.toLowerCase()).toContain(state);
    expect(view.approveButton).toBeUndefined();
    expect(view.rejectButton).toBeUndefined();
  });

  it('does not apply a decision response after Task switch and aborts the old request', async () => {
    const pending = withDeferredDecision();
    const view = await approvalFixture(pendingApproval());

    view.approveButton!.click();
    await view.element.updateComplete;
    const signal = decide.mock.calls[0][4] as AbortSignal;
    expect(signal.aborted).toBe(false);

    await view.switchTask('task_2');
    expect(signal.aborted).toBe(true);

    pending.resolve(decisionResult(approvedApproval()));
    await flush(view.element);

    expect(view.projectionEvents).toEqual([]);
    expect(get).not.toHaveBeenCalled();
    expect(view.approveButton!.disabled).toBe(false);
  });

  it('never renders executable controls in history mode', async () => {
    const view = await approvalFixture(pendingApproval());
    view.element.readOnly = true;
    await view.element.updateComplete;

    expect(view.executableControls).toEqual([]);
    expect(view.text.toLowerCase()).toContain('pending');
  });

  it('keeps the approval visible with a message when the decision request fails', async () => {
    const view = await approvalFixture(pendingApproval());
    decide.mockRejectedValue(new ApiError(503, 'Approval service unavailable'));

    view.approveButton!.click();
    await flush(view.element);

    expect(view.text).toContain('Approval service unavailable');
    expect(view.approveButton!.disabled).toBe(false);
    expect(view.approveButton).toBeDefined();
  });

  it('shows the decider and time for an already approved approval', async () => {
    const view = await approvalFixture(approvedApproval());

    expect(view.text).toContain('Approved');
    expect(view.text).toContain('admin_1');
    expect(view.text).toContain('2026-10-27T10:03:00Z');
    expect(view.executableControls).toEqual([]);
  });

  it('renders native focusable decision buttons for keyboard use', async () => {
    const view = await approvalFixture(pendingApproval());

    view.approveButton!.focus();
    expect(document.activeElement).toBe(view.approveButton);
    view.rejectButton!.focus();
    expect(document.activeElement).toBe(view.rejectButton);
  });

  it('renders nothing for an incomplete approval instead of an empty card', async () => {
    const view = await approvalFixture({ ...pendingApproval(), tool: undefined });

    expect(view.element.textContent?.trim()).toBe('');
    expect(view.executableControls).toEqual([]);
  });
});
