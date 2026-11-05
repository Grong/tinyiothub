import { describe, expect, it } from 'vitest';
import type { RuntimeEvent, RuntimeTask, TaskProjection } from '../../types/runtime.js';
import { groupRuntimeTasks, RuntimeProjection } from './runtime-projection.js';

export function task(id = 'task_1', over: Partial<RuntimeTask> = {}): RuntimeTask {
  return {
    id,
    workspaceId: 'ws_1',
    idempotencyKey: `key_${id}`,
    objective: 'Inspect pumps',
    sessionKey: null,
    createdBy: 'user_1',
    status: 'active',
    currentRunId: null,
    stateVersion: 0,
    createdAt: '2026-10-27T10:00:00Z',
    updatedAt: '2026-10-27T10:00:00Z',
    endedAt: null,
    ...over,
  };
}

export function snapshotWithNoDevices(over: Partial<TaskProjection> = {}): TaskProjection {
  return {
    task: task(),
    runs: [],
    jobs: [],
    approvals: [],
    watches: [],
    evidence: [],
    associations: [],
    devices: [],
    summary: null,
    cursor: 0,
    ...over,
  };
}

export function event(
  eventId: string,
  eventType: string,
  data: Record<string, unknown> = {},
  over: Partial<RuntimeEvent> = {},
): RuntimeEvent {
  return {
    cursor: Number(eventId.replace(/\D/g, '')) || 1,
    eventId,
    eventType,
    occurredAt: '2026-10-27T10:01:00Z',
    workspaceId: 'ws_1',
    taskId: 'task_1',
    runId: null,
    toolCallId: null,
    jobId: null,
    approvalId: null,
    deviceId: null,
    sourceId: null,
    sourceSeq: null,
    evidenceId: null,
    data,
    ...over,
  };
}

describe('RuntimeProjection', () => {
  it('deduplicates event ids and derives devices only from actual references', () => {
    const projection = new RuntimeProjection(snapshotWithNoDevices());

    projection.apply(event('event_1', 'list_devices', { candidates: ['dev_1'] }));
    expect(projection.devices).toEqual([]);

    projection.apply(event('event_2', 'read_device', { deviceId: 'dev_1' }, { deviceId: 'dev_1' }));
    projection.apply(event('event_2', 'read_device', { deviceId: 'dev_1' }, { deviceId: 'dev_1' }));

    expect(projection.devices.map((device) => device.deviceId)).toEqual(['dev_1']);
    expect(projection.steps).toHaveLength(2);
  });

  it('orders deltas by cursor and keeps first-reference device order', () => {
    const projection = new RuntimeProjection(snapshotWithNoDevices());
    projection.apply(event('event_3', 'call_device', {}, { cursor: 3, deviceId: 'dev_3' }));
    projection.apply(event('event_2', 'watch.created', {}, { cursor: 2, deviceId: 'dev_2' }));
    projection.apply(
      event(
        'event_4',
        'evidence.created',
        {},
        {
          cursor: 4,
          deviceId: 'dev_2',
          sourceId: 'camera',
          evidenceId: 'evidence_1',
        },
      ),
    );

    expect(projection.steps.map((step) => step.eventId)).toEqual(['event_2', 'event_3', 'event_4']);
    expect(projection.devices.map((device) => device.deviceId)).toEqual(['dev_2', 'dev_3']);
  });

  it('derives devices from Task jobs, approvals, watches and source evidence only', () => {
    const projection = new RuntimeProjection(snapshotWithNoDevices());
    projection.apply(
      event(
        'event_1',
        'job.accepted',
        { state: 'accepted' },
        {
          jobId: 'job_1',
          deviceId: 'dev_job',
        },
      ),
    );
    projection.apply(
      event(
        'event_2',
        'approval.requested',
        { state: 'pending' },
        {
          approvalId: 'approval_1',
          deviceId: 'dev_approval',
        },
      ),
    );
    projection.apply(event('event_3', 'watch.created', {}, { deviceId: 'dev_watch' }));
    projection.apply(
      event(
        'event_4',
        'read_device',
        {},
        {
          deviceId: 'dev_source',
          sourceId: 'source_1',
          evidenceId: 'evidence_1',
        },
      ),
    );

    expect(projection.devices.map((device) => device.deviceId)).toEqual([
      'dev_job',
      'dev_approval',
      'dev_watch',
      'dev_source',
    ]);
    expect(projection.jobs[0]).toMatchObject({ id: 'job_1', state: 'accepted' });
    expect(projection.approvals[0]).toMatchObject({ id: 'approval_1', state: 'pending' });
  });

  it('consumes authoritative snapshot associations instead of regrouping entity arrays', () => {
    const projection = new RuntimeProjection(
      snapshotWithNoDevices({
        watches: [
          {
            id: 'watch_1',
            condition: { thingId: 'dev_watch' },
            persistent: true,
            timeoutSecs: 30,
            createdAt: '2026-10-27T10:00:00Z',
          },
        ],
        associations: [
          { deviceId: 'dev_read', sourceIds: ['telemetry'] },
          { deviceId: 'dev_watch', sourceIds: [] },
          { deviceId: 'dev_evidence', sourceIds: ['camera'] },
        ],
        evidence: [
          {
            evidenceId: 'evidence_only',
            mime: 'image/jpeg',
            sourceSeq: 3,
            mediaTime: '2026-10-27T10:02:00Z',
            expiresAt: '2099-10-27T10:02:00Z',
            deviceId: 'dev_evidence',
            sourceId: 'camera',
            taskId: 'task_1',
            runId: null,
            toolCallId: null,
            jobId: null,
          },
        ],
      }),
    );

    expect(projection.devices.map((device) => device.deviceId)).toEqual([
      'dev_read',
      'dev_watch',
      'dev_evidence',
    ]);
    expect(projection.devices[0].sourceIds).toEqual(['telemetry']);
    expect(projection.devices[2].sourceIds).toEqual(['camera']);

    projection.replaceWithSnapshot(projection.snapshot);
    expect(projection.devices.map((device) => device.deviceId)).toEqual([
      'dev_read',
      'dev_watch',
      'dev_evidence',
    ]);
    expect(projection.evidence[0].deviceId).toBe('dev_evidence');
  });

  it('replaceWithSnapshot clears all prior Task-owned ordering and associations', () => {
    const projection = new RuntimeProjection(snapshotWithNoDevices());
    projection.apply(event('event_1', 'read_device', {}, { deviceId: 'old_device' }));

    projection.replaceWithSnapshot(
      snapshotWithNoDevices({
        task: task('task_2'),
        cursor: 20,
      }),
    );

    expect(projection.task.id).toBe('task_2');
    expect(projection.steps).toEqual([]);
    expect(projection.devices).toEqual([]);
    expect(projection.jobs).toEqual([]);
    expect(projection.approvals).toEqual([]);
    expect(projection.evidence).toEqual([]);
    expect(projection.cursor).toBe(20);
  });

  it('retains shown steps across a same-Task snapshot with a higher cursor', () => {
    const projection = new RuntimeProjection(snapshotWithNoDevices());
    projection.apply(event('event_5', 'read_device', {}, { cursor: 5, deviceId: 'dev_5' }));

    projection.replaceWithSnapshot(
      snapshotWithNoDevices({
        cursor: 20,
        associations: [{ deviceId: 'dev_snapshot', sourceIds: ['live'] }],
      }),
    );

    expect(projection.steps.map((step) => step.eventId)).toEqual(['event_5']);
    expect(projection.devices.map((device) => device.deviceId)).toEqual(['dev_snapshot', 'dev_5']);
    expect(projection.cursor).toBe(20);

    projection.apply(event('event_21', 'read_device', {}, { cursor: 21, deviceId: 'dev_21' }));
    expect(projection.steps.map((step) => step.eventId)).toEqual(['event_5', 'event_21']);
  });

  it('does not move the same-Task cursor guard backwards', () => {
    const projection = new RuntimeProjection(snapshotWithNoDevices());
    projection.apply(event('event_30', 'task.completed', { stateVersion: 1 }, { cursor: 30 }));

    projection.replaceWithSnapshot(snapshotWithNoDevices({ cursor: 10 }));

    expect(projection.cursor).toBe(30);
    expect(projection.task.status).toBe('completed');
  });

  it('freezes history events without conflating history with connection state', () => {
    const projection = new RuntimeProjection(snapshotWithNoDevices(), { history: true });
    projection.setConnection('offline');
    projection.apply(event('event_1', 'read_device', {}, { deviceId: 'dev_1' }));

    expect(projection.frozenHistory).toBe(true);
    expect(projection.steps).toEqual([]);
    expect(projection.connection).toBe('offline');
    expect(projection.stale).toBe(true);
  });

  it('freezes steps restored by the in-page history transition', () => {
    const projection = new RuntimeProjection(snapshotWithNoDevices());
    projection.restoreHistorySteps([event('event_1', 'read_device', {}, { deviceId: 'dev_1' })]);

    // Both the live delta and a bounded page must be ignored once frozen; this
    // fails if the frozenHistory guard in mergeSteps regresses.
    projection.apply(event('event_2', 'read_device', {}, { deviceId: 'dev_2' }));
    projection.mergeSteps([event('event_3', 'read_device', {}, { deviceId: 'dev_3' })]);

    expect(projection.steps.map((step) => step.eventId)).toEqual(['event_1']);
    expect(projection.eventIds.has('event_2')).toBe(false);
    expect(projection.eventIds.has('event_3')).toBe(false);
  });

  it('tracks high-watermark separately from replay progress and deduplicates by cursor', () => {
    const projection = new RuntimeProjection(snapshotWithNoDevices());
    projection.replaceWithSnapshot(snapshotWithNoDevices({ cursor: 20 }), 20, 25);
    projection.apply(event('event_11', 'read_device', {}, { cursor: 11 }), 25);
    projection.apply(event('event_11_other_id', 'read_device', {}, { cursor: 11 }), 25);

    expect(projection.cursor).toBe(20);
    expect(projection.highWatermark).toBe(25);
    expect(projection.steps.map((step) => step.eventId)).toEqual(['event_11']);
  });

  it('marks an interrupted unclosed text tail without pretending replay is complete', () => {
    const projection = new RuntimeProjection(snapshotWithNoDevices());
    projection.apply(
      event(
        'event_9',
        'run.interrupted',
        { incomplete_tail: true, last_persisted_seq: 8 },
        { cursor: 9 },
      ),
    );

    expect(projection.incompleteTail).toBe(true);
    expect(projection.lastPersistedSeq).toBe(8);
  });

  it('marks an interrupted tail when frozen history steps are restored', () => {
    const projection = new RuntimeProjection(snapshotWithNoDevices(), { history: true });
    projection.restoreHistorySteps([
      event(
        'event_9',
        'run.interrupted',
        { incomplete_tail: true, last_persisted_seq: 8 },
        { cursor: 9 },
      ),
    ]);

    expect(projection.incompleteTail).toBe(true);
    expect(projection.lastPersistedSeq).toBe(8);
  });

  it('round-trips live deviceFacts through the projection snapshot', () => {
    const projection = new RuntimeProjection(snapshotWithNoDevices());
    projection.apply(
      event(
        'event_1',
        'read_device',
        {
          deviceFacts: {
            deviceId: 'dev_camera',
            health: 'ok',
            zone: 'North',
            position: 'rack-9',
            sources: [{ id: 'live', dataKind: 'video' }],
            fields: [{ name: 'temperature', value: null, unit: 'C' }],
          },
        },
        { deviceId: 'dev_camera' },
      ),
    );

    const roundTrip = new RuntimeProjection(projection.snapshot);
    const camera = roundTrip.devices.find((device) => device.deviceId === 'dev_camera');

    expect(camera).toMatchObject({
      health: 'ok',
      zone: 'North',
      position: 'rack-9',
      sources: [{ id: 'live', dataKind: 'video' }],
      fields: [{ name: 'temperature', value: null, unit: 'C' }],
    });
    expect(roundTrip.snapshot.devices.map((device) => device.deviceId)).toEqual(['dev_camera']);
  });

  it('groups current and recent Tasks by latest activity', () => {
    const groups = groupRuntimeTasks([
      task('old-current', { updatedAt: '2026-10-27T09:00:00Z' }),
      task('done', { status: 'completed', updatedAt: '2026-10-27T11:00:00Z' }),
      task('new-current', { status: 'paused', updatedAt: '2026-10-27T12:00:00Z' }),
      task('failed', { status: 'failed', updatedAt: '2026-10-27T10:00:00Z' }),
    ]);

    expect(groups.current.map((item) => item.id)).toEqual(['new-current', 'old-current']);
    expect(groups.recent.map((item) => item.id)).toEqual(['done', 'failed']);
  });
});
