import { afterEach, describe, expect, it, vi } from 'vitest';
import type { RuntimeEvent, TaskProjection } from '../../types/runtime.js';
import { RuntimeProjection } from './runtime-projection.js';
import './runtime-flow.js';
import type { RuntimeFlow } from './runtime-flow.js';

const mounted: RuntimeFlow[] = [];

function projectionWithEdges(): RuntimeProjection {
  const snapshot: TaskProjection = {
    task: {
      id: 'task_1', workspaceId: 'ws_1', idempotencyKey: 'key_1', objective: 'Inspect flow',
      sessionKey: null, createdBy: 'user_1', status: 'active', currentRunId: 'run_1', stateVersion: 0,
      createdAt: '2026-10-27T10:00:00Z', updatedAt: '2026-10-27T10:00:00Z', endedAt: null,
    },
    runs: [], jobs: [], approvals: [], watches: [], evidence: [], summary: null, cursor: 0,
  };
  const projection = new RuntimeProjection(snapshot);
  const base: RuntimeEvent = {
    cursor: 1, eventId: 'event_1', eventType: 'call_device', occurredAt: '2026-10-27T10:01:00Z',
    workspaceId: 'ws_1', taskId: 'task_1', runId: 'run_1', toolCallId: null, jobId: null,
    approvalId: null, deviceId: 'dev_1', sourceId: null, sourceSeq: null, evidenceId: null, data: {},
  };
  projection.apply({ ...base, cursor: 3, eventId: 'event_3', eventType: 'read_device', sourceId: 'source_1' });
  projection.apply(base);
  projection.apply({
    ...base, cursor: 2, eventId: 'event_2', eventType: 'evidence.created', deviceId: 'dev_2',
    sourceId: 'camera', sourceSeq: 2, evidenceId: 'evidence_1', data: { mime: 'image/png' },
  });
  return projection;
}

async function mount(projection: RuntimeProjection, reducedMotion = false): Promise<RuntimeFlow> {
  const flow = document.createElement('runtime-flow') as RuntimeFlow;
  flow.projection = projection;
  flow.reducedMotion = reducedMotion;
  document.body.append(flow);
  mounted.push(flow);
  await flow.updateComplete;
  return flow;
}

afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
});

describe('runtime-flow', () => {
  it('uses projection ordering for visual CONTROL and DATA lanes and device ports', async () => {
    const flow = await mount(projectionWithEdges());
    const eventIds = (lane: string) => [...flow.querySelectorAll<HTMLElement>(`[data-lane="${lane}"] [data-event-id]`)]
      .map((event) => event.dataset.eventId);

    expect(eventIds('control')).toEqual(['event_1']);
    expect(eventIds('data')).toEqual(['event_2', 'event_3']);
    expect([...flow.querySelectorAll<HTMLElement>('[data-device-port]')].map((port) => port.dataset.deviceId))
      .toEqual(['dev_1', 'dev_2']);
    expect(flow.textContent).toContain('visual lanes');
  });

  it('selects historical events explicitly while device ports only locate devices', async () => {
    const flow = await mount(projectionWithEdges());
    const selectEvent = vi.fn();
    const locate = vi.fn();
    flow.addEventListener('event-select', selectEvent);
    flow.addEventListener('locate-device', locate);

    flow.querySelector<HTMLButtonElement>('button[data-device-port][data-device-id="dev_1"]')!.click();
    expect(locate.mock.calls[0][0].detail).toEqual({ deviceId: 'dev_1' });
    expect(selectEvent).not.toHaveBeenCalled();

    flow.querySelector<HTMLButtonElement>('button[data-event-id="event_2"]')!.click();
    expect(selectEvent).toHaveBeenCalledOnce();
    expect(selectEvent.mock.calls[0][0].detail).toEqual({ eventId: 'event_2' });
  });

  it('replaces particles with a static bus, icons, and text for reduced motion', async () => {
    const flow = await mount(projectionWithEdges(), true);

    expect(flow.querySelector('.flow-particle')).toBeNull();
    expect(flow.querySelector('.flow-static')).not.toBeNull();
    expect(flow.textContent).toContain('Static data bus');
  });
});
