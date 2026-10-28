import { afterEach, describe, expect, it, vi } from 'vitest';
import type { RuntimeEvent, RuntimeJob, TaskProjection } from '../../types/runtime.js';
import { RuntimeProjection } from './runtime-projection.js';
import './runtime-devices.js';
import type { RuntimeDevices } from './runtime-devices.js';

const mounted: RuntimeDevices[] = [];

function projectionFromFixtureB(): RuntimeProjection {
  const snapshot: TaskProjection = {
    task: {
      id: 'task_b', workspaceId: 'ws_1', idempotencyKey: 'key_b', objective: 'Inspect fixture B',
      sessionKey: null, createdBy: 'user_1', status: 'active', currentRunId: null, stateVersion: 0,
      createdAt: '2026-10-27T10:00:00Z', updatedAt: '2026-10-27T10:00:00Z', endedAt: null,
    },
    runs: [], jobs: [], approvals: [], watches: [], evidence: [], summary: null, cursor: 0,
  };
  const projection = new RuntimeProjection(snapshot);
  const base: RuntimeEvent = {
    cursor: 1, eventId: 'event_1', eventType: 'list_devices', occurredAt: '2026-10-27T10:01:00Z',
    workspaceId: 'ws_1', taskId: 'task_b', runId: null, toolCallId: null, jobId: null,
    approvalId: null, deviceId: null, sourceId: null, sourceSeq: null, evidenceId: null,
    data: { candidates: ['list-only-device'] },
  };
  projection.apply(base);
  projection.apply({ ...base, cursor: 2, eventId: 'event_2', eventType: 'read_device', deviceId: 'fixture-b-camera' });
  projection.apply({ ...base, cursor: 3, eventId: 'event_3', eventType: 'call_device', deviceId: 'fixture-b-valve' });
  return projection;
}

async function mount(projection: RuntimeProjection): Promise<RuntimeDevices> {
  const devices = document.createElement('runtime-devices') as RuntimeDevices;
  devices.projection = projection;
  document.body.append(devices);
  mounted.push(devices);
  await devices.updateComplete;
  return devices;
}

afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
});

describe('runtime-devices', () => {
  it('renders only actual associations in first-reference order without scenario cards', async () => {
    const devices = await mount(projectionFromFixtureB());
    const cards = [...devices.querySelectorAll<HTMLElement>('[data-device-card]')];

    expect(devices.querySelector('[data-scenario]')).toBeNull();
    expect(cards.map((card) => card.dataset.deviceId)).toEqual(['fixture-b-camera', 'fixture-b-valve']);
    expect(devices.textContent).not.toContain('list-only-device');
  });

  it('shows active job state and Ready/Busy without fake progress or invented declarations', async () => {
    const projection = projectionFromFixtureB();
    const running: RuntimeJob = {
      id: 'job_camera', workspaceId: 'ws_1', deviceId: 'fixture-b-camera', tool: 'read_device',
      argsJson: {}, state: 'running', stopRequestedAt: null, progressJson: { percent: 80 },
      resultJson: null, taskId: 'task_b', runId: null, toolCallId: null, driverCallId: null,
      timeoutMs: 1000, startedAt: '2026-10-27T10:02:00Z', endedAt: null,
    };
    projection.replaceWithSnapshot({
      ...projection.snapshot,
      jobs: [running, { ...running, id: 'job_valve_done', deviceId: 'fixture-b-valve', state: 'done' }],
    });
    const devices = await mount(projection);
    const camera = devices.querySelector<HTMLElement>('[data-device-id="fixture-b-camera"]');
    const valve = devices.querySelector<HTMLElement>('[data-device-id="fixture-b-valve"]');

    expect(camera?.textContent).toContain('Busy');
    expect(camera?.textContent).toContain('running');
    expect(valve?.textContent).toContain('Ready');
    expect(devices.textContent).not.toContain('%');
    expect(devices.textContent).toContain('Health unavailable');
    expect(devices.textContent).toContain('Location unavailable');
    expect(devices.textContent).toContain('Declaration media unavailable');
  });

  it('emits device location and focuses the card heading with reduced-motion scrolling', async () => {
    const devices = await mount(projectionFromFixtureB());
    const locate = vi.fn();
    devices.addEventListener('locate-device', locate);
    const button = devices.querySelector<HTMLButtonElement>('button[data-device-id="fixture-b-valve"]');
    button!.click();
    expect(locate.mock.calls[0][0].detail).toEqual({ deviceId: 'fixture-b-valve' });

    const card = devices.querySelector<HTMLElement>('[data-device-card][data-device-id="fixture-b-valve"]')!;
    const heading = card.querySelector<HTMLElement>('h3')!;
    const scroll = vi.fn();
    card.scrollIntoView = scroll;
    const focus = vi.spyOn(heading, 'focus');
    devices.focusDevice('fixture-b-valve', true);

    expect(scroll).toHaveBeenCalledWith({ behavior: 'auto', block: 'nearest' });
    expect(focus).toHaveBeenCalledWith({ preventScroll: true });
  });
});
