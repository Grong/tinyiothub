import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client.js';
import { stopRuntimeJob } from '../../api/runtime.js';
import type {
  DeviceJobState,
  RuntimeApproval,
  RuntimeEvent,
  RuntimeJob,
  TaskProjection,
} from '../../types/runtime.js';
import { RuntimeProjection } from './runtime-projection.js';
import './runtime-loop.js';
import type { RuntimeLoop } from './runtime-loop.js';

vi.mock('../../api/runtime.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/runtime.js')>();
  return { ...actual, stopRuntimeJob: vi.fn() };
});

const stop = vi.mocked(stopRuntimeJob);
const mounted: RuntimeLoop[] = [];

function snapshot(jobs: RuntimeJob[] = [], taskId = 'task_1'): TaskProjection {
  return {
    task: {
      id: taskId,
      workspaceId: 'ws_1',
      idempotencyKey: `key_${taskId}`,
      objective: 'Inspect devices',
      sessionKey: null,
      createdBy: 'user_1',
      status: 'active',
      currentRunId: 'run_1',
      stateVersion: 0,
      createdAt: '2026-10-27T10:00:00Z',
      updatedAt: '2026-10-27T10:00:00Z',
      endedAt: null,
    },
    runs: [],
    jobs,
    approvals: [],
    watches: [],
    evidence: [],
    summary: null,
    cursor: 0,
    associations: [],
    devices: [],
  };
}

function job(state: DeviceJobState, over: Partial<RuntimeJob> = {}): RuntimeJob {
  return {
    id: 'job_1',
    workspaceId: 'ws_1',
    deviceId: 'dev_1',
    tool: 'set_device',
    argsJson: { value: 21 },
    state,
    stopRequestedAt: null,
    progressJson: { percent: 42, label: '42%' },
    resultJson: state === 'done' ? { ok: true } : null,
    taskId: 'task_1',
    runId: 'run_1',
    toolCallId: 'call_1',
    driverCallId: null,
    timeoutMs: 1000,
    startedAt: '2026-10-27T10:00:00Z',
    endedAt: null,
    ...over,
  };
}

function approval(over: Partial<RuntimeApproval> = {}): RuntimeApproval {
  return {
    id: 'approval_1',
    workspaceId: 'ws_1',
    taskId: 'task_1',
    runId: 'run_1',
    toolCallId: 'call_1',
    deviceId: 'dev_1',
    tool: 'set_device',
    argsJson: { value: 21 },
    reason: 'Raise the pump',
    state: 'pending',
    decision: null,
    deciderId: null,
    requestedAt: '2026-10-27T10:00:00Z',
    decidedAt: null,
    expiresAt: '2026-10-27T10:05:00Z',
    decisionEventId: 'decision_event_1',
    updatedAt: '2026-10-27T10:00:00Z',
    ...over,
  };
}

function event(over: Partial<RuntimeEvent>): RuntimeEvent {
  return {
    cursor: 1,
    eventId: 'event_1',
    eventType: 'read_device',
    occurredAt: '2026-10-27T10:01:00Z',
    workspaceId: 'ws_1',
    taskId: 'task_1',
    runId: 'run_1',
    toolCallId: null,
    jobId: null,
    approvalId: null,
    deviceId: null,
    sourceId: null,
    sourceSeq: null,
    evidenceId: null,
    data: {},
    ...over,
  };
}

async function mount(
  projection: RuntimeProjection,
  options: { readOnly?: boolean } = {},
): Promise<RuntimeLoop> {
  const loop = document.createElement('runtime-loop') as RuntimeLoop;
  loop.projection = projection;
  loop.readOnly = options.readOnly ?? false;
  document.body.append(loop);
  mounted.push(loop);
  await loop.updateComplete;
  return loop;
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
} {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function ok(result: TaskProjection) {
  return { code: 0, msg: '', result };
}

async function flush(loop: RuntimeLoop): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await loop.updateComplete;
}

interface JobCardFixture {
  readonly loop: RuntimeLoop;
  readonly text: string;
  readonly stopButton: HTMLButtonElement | null;
  readonly confirmStop: HTMLButtonElement | null;
}

async function jobCardFixture(
  over: Partial<RuntimeJob> = {},
  options: { readOnly?: boolean; taskId?: string } = {},
): Promise<JobCardFixture> {
  const projection = new RuntimeProjection(
    snapshot([job((over.state ?? 'running') as DeviceJobState, over)], options.taskId),
  );
  const loop = await mount(projection, options);
  return {
    loop,
    get text() {
      return loop.textContent ?? '';
    },
    get stopButton() {
      return loop.querySelector<HTMLButtonElement>('button[data-action="stop-job"]');
    },
    get confirmStop() {
      return loop.querySelector<HTMLButtonElement>('button[data-action="confirm-stop"]');
    },
  };
}

beforeEach(() => {
  stop.mockReset();
});

afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
  vi.clearAllMocks();
});

describe('runtime-loop', () => {
  it('renders job facts without percentage progress and locates a referenced device', async () => {
    const projection = new RuntimeProjection(snapshot([job('running')]));
    const loop = await mount(projection);
    const locate = vi.fn();
    loop.addEventListener('locate-device', locate);

    expect(loop.textContent).toContain('set_device');
    expect(loop.textContent).toContain('running');
    expect(loop.textContent).not.toContain('%');
    expect(loop.textContent).not.toContain('42');

    loop.querySelector<HTMLButtonElement>('button[data-device-id="dev_1"]')!.click();
    expect(locate).toHaveBeenCalledOnce();
    expect(locate.mock.calls[0][0].detail).toEqual({ deviceId: 'dev_1' });
  });

  it('keeps cancelled_unknown factual and requests an explicit refresh', async () => {
    const loop = await mount(new RuntimeProjection(snapshot([job('cancelled_unknown')])));
    const refresh = vi.fn();
    loop.addEventListener('refresh-request', refresh);

    expect(loop.querySelector('[role="alert"]')?.textContent).toContain('Stop result unknown');
    expect(loop.textContent).not.toContain('All stopped');
    const button = [...loop.querySelectorAll<HTMLButtonElement>('button')].find(
      (candidate) => candidate.textContent?.replace(/\s+/g, ' ').trim() === 'Refresh status',
    );
    expect(button).toBeDefined();
    button!.click();
    expect(refresh).toHaveBeenCalledOnce();
  });

  it('renders ordered loop phases and image evidence as device-location actions', async () => {
    const projection = new RuntimeProjection(snapshot());
    projection.apply(
      event({
        eventId: 'event_1',
        cursor: 1,
        eventType: 'reason.updated',
        data: { text: 'Inspect' },
      }),
    );
    projection.apply(
      event({ eventId: 'event_2', cursor: 2, eventType: 'call_device', deviceId: 'dev_1' }),
    );
    projection.apply(
      event({
        eventId: 'event_3',
        cursor: 3,
        eventType: 'evidence.created',
        deviceId: 'dev_1',
        sourceId: 'camera',
        sourceSeq: 4,
        evidenceId: 'evidence_1',
        data: { mime: 'image/jpeg' },
      }),
    );
    const loop = await mount(projection);
    const locate = vi.fn();
    loop.addEventListener('locate-device', locate);

    expect(loop.textContent).toContain('Reason');
    expect(loop.textContent).toContain('Tool');
    expect(loop.textContent).toContain('Result');
    const evidence = loop.querySelector<HTMLButtonElement>('button[data-evidence-id="evidence_1"]');
    expect(evidence?.textContent).toContain('Image evidence');
    evidence!.click();
    expect(locate.mock.calls[0][0].detail).toEqual({ deviceId: 'dev_1', evidenceId: 'evidence_1' });
  });

  it('marks expired image evidence unavailable without a device-location action', async () => {
    const projection = new RuntimeProjection(snapshot());
    projection.replaceWithSnapshot({
      ...projection.snapshot,
      associations: [{ deviceId: 'dev_1', sourceIds: ['camera'] }],
      evidence: [
        {
          evidenceId: 'evidence_expired',
          mime: 'image/png',
          sourceSeq: 9,
          mediaTime: '2026-10-27T10:03:00Z',
          expiresAt: '2026-10-27T10:05:00Z',
          deviceId: 'dev_1',
          sourceId: 'camera',
          taskId: 'task_1',
          runId: 'run_1',
          toolCallId: null,
          jobId: null,
        },
      ],
    });
    const loop = await mount(projection);
    const locate = vi.fn();
    loop.addEventListener('locate-device', locate);

    expect(loop.textContent).toContain('Image evidence unavailable (expired)');
    expect(loop.querySelector('button[data-evidence-id="evidence_expired"]')).toBeNull();
    expect(loop.querySelector('[data-evidence-expired="evidence_expired"]')).not.toBeNull();
    expect(locate).not.toHaveBeenCalled();
  });

  it('fills ToolCard details from production-shaped live accepted and terminal events', async () => {
    const projection = new RuntimeProjection(snapshot());
    projection.apply(
      event({
        eventId: 'job:job_live:accepted',
        eventType: 'job.accepted',
        cursor: 1,
        jobId: 'job_live',
        deviceId: 'dev_1',
        toolCallId: 'call_live',
        data: { state: 'accepted', tool: 'set_device', argsJson: { value: 21 } },
      }),
    );
    projection.apply(
      event({
        eventId: 'job:job_live:done',
        eventType: 'job.done',
        cursor: 2,
        jobId: 'job_live',
        deviceId: 'dev_1',
        toolCallId: 'call_live',
        data: {
          state: 'done',
          tool: 'set_device',
          argsJson: { value: 21 },
          resultJson: { actual: 21 },
        },
      }),
    );
    const loop = await mount(projection);
    const card = loop.querySelector<HTMLElement>('[data-job-id="job_live"]');

    expect(card?.querySelector('strong')?.textContent).toBe('set_device');
    expect(card?.textContent).toContain('"value": 21');
    expect(card?.textContent).toContain('"actual": 21');
    expect(card?.textContent).not.toContain('Device job');
  });

  it('renders linked persisted image evidence after a snapshot reload without a transient event', async () => {
    const projection = new RuntimeProjection(snapshot());
    projection.replaceWithSnapshot({
      ...projection.snapshot,
      associations: [{ deviceId: 'dev_1', sourceIds: ['camera'] }],
      evidence: [
        {
          evidenceId: 'evidence_reload',
          mime: 'image/png',
          sourceSeq: 9,
          mediaTime: '2026-10-27T10:03:00Z',
          expiresAt: '2099-10-27T10:03:00Z',
          deviceId: 'dev_1',
          sourceId: 'camera',
          taskId: 'task_1',
          runId: 'run_1',
          toolCallId: null,
          jobId: null,
        },
      ],
    });
    const loop = await mount(projection);
    const locate = vi.fn();
    loop.addEventListener('locate-device', locate);
    const evidence = loop.querySelector<HTMLButtonElement>(
      'button[data-evidence-id="evidence_reload"]',
    );

    expect(evidence).not.toBeNull();
    evidence!.click();
    expect(locate.mock.calls[0][0].detail).toEqual({
      deviceId: 'dev_1',
      evidenceId: 'evidence_reload',
    });
  });

  it('stops only the owned active job after inline confirmation', async () => {
    const response = deferred<{ code: number; msg: string; result: TaskProjection }>();
    stop.mockReturnValue(response.promise);
    const card = await jobCardFixture({ id: 'job_1', taskId: 'task_1', state: 'running' });
    const projectionEvents: TaskProjection[] = [];
    card.loop.addEventListener('projection-change', (event) => {
      projectionEvents.push((event as CustomEvent<TaskProjection>).detail);
    });

    expect(card.stopButton!.textContent).toContain('Stop job');
    card.stopButton!.click();
    await card.loop.updateComplete;
    expect(stop).not.toHaveBeenCalled();
    expect(card.text).toContain('only this job');

    card.confirmStop!.click();
    expect(stop).toHaveBeenCalledOnce();
    expect(stop).toHaveBeenCalledWith('ws_1', 'task_1', 'job_1', expect.any(AbortSignal));
    await card.loop.updateComplete;
    expect(card.text).toContain('Stopping…');
    expect(card.text).toContain('only this job');

    card.confirmStop!.click();
    expect(stop).toHaveBeenCalledOnce();

    response.resolve(ok(snapshot([job('cancelled')])));
    await flush(card.loop);
    expect(projectionEvents).toHaveLength(1);
    expect(projectionEvents[0].jobs[0].state).toBe('cancelled');
  });

  it('keeps a job on Keep job without issuing a stop request', async () => {
    const card = await jobCardFixture({ state: 'running' });

    card.stopButton!.click();
    await card.loop.updateComplete;
    card.loop.querySelector<HTMLButtonElement>('button[data-action="keep-job"]')!.click();
    await card.loop.updateComplete;

    expect(stop).not.toHaveBeenCalled();
    expect(card.stopButton).not.toBeNull();
    expect(card.confirmStop).toBeNull();
  });

  it('reports an unknown stop result without retry or a success claim', async () => {
    stop.mockRejectedValue(new ApiError(0, 'Network error'));
    const card = await jobCardFixture({ state: 'running' });
    const refresh = vi.fn();
    card.loop.addEventListener('refresh-request', refresh);

    card.stopButton!.click();
    await card.loop.updateComplete;
    card.confirmStop!.click();
    await flush(card.loop);

    expect(stop).toHaveBeenCalledOnce();
    expect(card.text).toContain('Stop result unknown');
    expect(card.text).not.toContain('All stopped');
    expect(card.loop.querySelector('button[data-action="stop-job"]')).toBeNull();
    const refreshButton = card.loop.querySelector<HTMLButtonElement>(
      'button[data-action="refresh-status"]',
    )!;
    refreshButton.click();
    expect(refresh).toHaveBeenCalledOnce();
    expect(stop).toHaveBeenCalledOnce();
  });

  it.each(['done', 'failed', 'cancelled', 'interrupted'] as const)(
    'offers no Stop for a %s job',
    async (state) => {
      const card = await jobCardFixture({ state });
      expect(card.stopButton).toBeNull();
      expect(card.loop.querySelector('[data-executable]')).toBeNull();
    },
  );

  it('offers no Stop for a foreign Task job, a cancelled_unknown job, or history', async () => {
    const foreign = await jobCardFixture({ state: 'running', taskId: 'task_2' });
    expect(foreign.stopButton).toBeNull();

    const unknown = await jobCardFixture({ state: 'cancelled_unknown' });
    expect(unknown.stopButton).toBeNull();

    const history = await jobCardFixture({ state: 'running' }, { readOnly: true });
    expect(history.stopButton).toBeNull();
  });

  it('renders an approval inline in the Agent Loop with read-only arguments', async () => {
    const projection = new RuntimeProjection({ ...snapshot(), approvals: [approval()] });
    projection.replaceWithSnapshot({ ...projection.snapshot, approvals: [approval()] });
    const loop = await mount(projection);

    const card = loop.querySelector('runtime-approval');
    expect(card).not.toBeNull();
    expect(card?.textContent).toContain('set_device');
    expect(card?.textContent).toContain('dev_1');
    expect(card?.textContent).toContain('"value": 21');
    expect(card?.querySelectorAll('[data-executable]')).toHaveLength(2);
  });

  it('renders one approval card when request and decision share an approval id', async () => {
    const projection = new RuntimeProjection({ ...snapshot(), approvals: [approval()] });
    projection.apply(
      event({
        eventId: 'event_1',
        cursor: 1,
        eventType: 'approval.requested',
        approvalId: 'approval_1',
        data: { state: 'pending' },
      }),
    );
    projection.apply(
      event({
        eventId: 'event_2',
        cursor: 2,
        eventType: 'approval.decided',
        approvalId: 'approval_1',
        data: { state: 'approved' },
      }),
    );
    const loop = await mount(projection);

    expect(loop.querySelectorAll('runtime-approval')).toHaveLength(1);
    expect(
      loop.querySelector('[data-approval-id="approval_1"]')?.getAttribute('data-approval-state'),
    ).toBe('approved');
  });

  it('renders an approval without executable actions in history', async () => {
    const projection = new RuntimeProjection({ ...snapshot(), approvals: [approval()] });
    const loop = await mount(projection, { readOnly: true });

    expect(loop.querySelector('runtime-approval')).not.toBeNull();
    expect(loop.querySelectorAll('[data-executable]')).toHaveLength(0);
  });
});
