import { afterEach, describe, expect, it, vi } from 'vitest';
import type { DeviceJobState, RuntimeEvent, RuntimeJob, TaskProjection } from '../../types/runtime.js';
import { RuntimeProjection } from './runtime-projection.js';
import './runtime-loop.js';
import type { RuntimeLoop } from './runtime-loop.js';

const mounted: RuntimeLoop[] = [];

function snapshot(jobs: RuntimeJob[] = []): TaskProjection {
  return {
    task: {
      id: 'task_1', workspaceId: 'ws_1', idempotencyKey: 'key_1', objective: 'Inspect devices',
      sessionKey: null, createdBy: 'user_1', status: 'active', currentRunId: 'run_1', stateVersion: 0,
      createdAt: '2026-10-27T10:00:00Z', updatedAt: '2026-10-27T10:00:00Z', endedAt: null,
    },
    runs: [], jobs, approvals: [], watches: [], evidence: [], summary: null, cursor: 0,
    associations: [], devices: [],
  };
}

function job(state: DeviceJobState): RuntimeJob {
  return {
    id: 'job_1', workspaceId: 'ws_1', deviceId: 'dev_1', tool: 'set_device',
    argsJson: { value: 21 }, state, stopRequestedAt: null,
    progressJson: { percent: 42, label: '42%' }, resultJson: state === 'done' ? { ok: true } : null,
    taskId: 'task_1', runId: 'run_1', toolCallId: 'call_1', driverCallId: null,
    timeoutMs: 1000, startedAt: '2026-10-27T10:00:00Z', endedAt: null,
  };
}

function event(over: Partial<RuntimeEvent>): RuntimeEvent {
  return {
    cursor: 1, eventId: 'event_1', eventType: 'read_device', occurredAt: '2026-10-27T10:01:00Z',
    workspaceId: 'ws_1', taskId: 'task_1', runId: 'run_1', toolCallId: null, jobId: null,
    approvalId: null, deviceId: null, sourceId: null, sourceSeq: null, evidenceId: null, data: {},
    ...over,
  };
}

async function mount(projection: RuntimeProjection): Promise<RuntimeLoop> {
  const loop = document.createElement('runtime-loop') as RuntimeLoop;
  loop.projection = projection;
  document.body.append(loop);
  mounted.push(loop);
  await loop.updateComplete;
  return loop;
}

afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
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

    (loop.querySelector<HTMLButtonElement>('button[data-device-id="dev_1"]'))!.click();
    expect(locate).toHaveBeenCalledOnce();
    expect(locate.mock.calls[0][0].detail).toEqual({ deviceId: 'dev_1' });
  });

  it('keeps cancelled_unknown factual and requests an explicit refresh', async () => {
    const loop = await mount(new RuntimeProjection(snapshot([job('cancelled_unknown')])));
    const refresh = vi.fn();
    loop.addEventListener('refresh-request', refresh);

    expect(loop.querySelector('[role="alert"]')?.textContent).toContain('Stop result unknown');
    expect(loop.textContent).not.toContain('All stopped');
    const button = [...loop.querySelectorAll<HTMLButtonElement>('button')]
      .find((candidate) => candidate.textContent === 'Refresh status');
    expect(button).toBeDefined();
    button!.click();
    expect(refresh).toHaveBeenCalledOnce();
  });

  it('renders ordered loop phases and image evidence as device-location actions', async () => {
    const projection = new RuntimeProjection(snapshot());
    projection.apply(event({ eventId: 'event_1', cursor: 1, eventType: 'reason.updated', data: { text: 'Inspect' } }));
    projection.apply(event({ eventId: 'event_2', cursor: 2, eventType: 'call_device', deviceId: 'dev_1' }));
    projection.apply(event({
      eventId: 'event_3', cursor: 3, eventType: 'evidence.created', deviceId: 'dev_1', sourceId: 'camera',
      sourceSeq: 4, evidenceId: 'evidence_1', data: { mime: 'image/jpeg' },
    }));
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

  it('fills ToolCard details from production-shaped live accepted and terminal events', async () => {
    const projection = new RuntimeProjection(snapshot());
    projection.apply(event({
      eventId: 'job:job_live:accepted', eventType: 'job.accepted', cursor: 1,
      jobId: 'job_live', deviceId: 'dev_1', toolCallId: 'call_live',
      data: { state: 'accepted', tool: 'set_device', argsJson: { value: 21 } },
    }));
    projection.apply(event({
      eventId: 'job:job_live:done', eventType: 'job.done', cursor: 2,
      jobId: 'job_live', deviceId: 'dev_1', toolCallId: 'call_live',
      data: { state: 'done', tool: 'set_device', argsJson: { value: 21 }, resultJson: { actual: 21 } },
    }));
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
      evidence: [{
        evidenceId: 'evidence_reload', mime: 'image/png', sourceSeq: 9, mediaTime: '2026-10-27T10:03:00Z',
        deviceId: 'dev_1', sourceId: 'camera', taskId: 'task_1', runId: 'run_1', toolCallId: null, jobId: null,
      }],
    });
    const loop = await mount(projection);
    const locate = vi.fn();
    loop.addEventListener('locate-device', locate);
    const evidence = loop.querySelector<HTMLButtonElement>('button[data-evidence-id="evidence_reload"]');

    expect(evidence).not.toBeNull();
    evidence!.click();
    expect(locate.mock.calls[0][0].detail).toEqual({ deviceId: 'dev_1', evidenceId: 'evidence_reload' });
  });
});
