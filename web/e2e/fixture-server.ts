/**
 * Local-only Runtime browser fixture.
 *
 * Serves synthetic API projections, a real HLS playlist under /allowed/ and
 * /denied/ (CORS intentionally differs), and counters that Playwright may
 * inspect. It never talks to production, reads credentials, or proxies media
 * through the Hub.
 */
import { createReadStream, existsSync } from 'node:fs';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { dirname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const mediaDir = join(here, 'fixtures', 'media');
const port = Number(process.env.FIXTURE_PORT ?? 3002);
const workspaceId = 'ws_e2e';
const origin = `http://127.0.0.1:${port}`;

type Json = Record<string, unknown>;

const counters = {
  mediaRequests: 0,
  cancelRequests: 0,
  decisionRequests: {} as Record<string, number>,
};

let releaseDelayedDecision: (() => void) | null = null;

function now(): string {
  return new Date().toISOString();
}

function future(minutes = 5): string {
  return new Date(Date.now() + minutes * 60_000).toISOString();
}

function task(id: string, status = 'active'): Json {
  return {
    id,
    workspaceId,
    idempotencyKey: `fixture-${id}`,
    objective: `Fixture objective ${id}`,
    sessionKey: `agent:${workspaceId}:e2e/${id}`,
    createdBy: 'user_e2e',
    status,
    currentRunId: status === 'active' || status === 'needs_approval' ? `run_${id}` : null,
    stateVersion: 1,
    createdAt: '2026-10-27T10:00:00Z',
    updatedAt: '2026-10-27T10:05:00Z',
    endedAt: status === 'completed' ? '2026-10-27T10:05:00Z' : null,
  };
}

function device(deviceId: string, dataKind = 'video'): Json {
  return {
    deviceId,
    health: 'ok',
    zone: 'Entrance',
    position: 'rack-1',
    sources: dataKind === 'none' ? [] : [{ id: 'live', dataKind }],
    fields: [],
  };
}

function approval(id: string, taskId: string, state = 'pending'): Json {
  return {
    id,
    workspaceId,
    taskId,
    runId: `run_${taskId}`,
    toolCallId: `call_${id}`,
    deviceId: 'cam-allowed',
    tool: 'set_device',
    argsJson: { power: 'on' },
    reason: 'The fixture needs a durable decision.',
    state,
    decision: state === 'approved' ? 'approve' : state === 'rejected' ? 'reject' : null,
    deciderId: state === 'pending' ? null : 'user_e2e',
    requestedAt: '2026-10-27T10:00:00Z',
    decidedAt: state === 'pending' ? null : '2026-10-27T10:01:00Z',
    expiresAt: '2099-10-27T10:05:00Z',
    decisionEventId: `decision_${id}`,
    updatedAt: '2026-10-27T10:01:00Z',
  };
}

function runtimeEvent(taskId: string, cursor: number, eventType: string, extra: Json = {}): Json {
  return {
    cursor,
    eventId: `event_${cursor}`,
    eventType,
    occurredAt: '2026-10-27T10:01:00Z',
    workspaceId,
    taskId,
    runId: `run_${taskId}`,
    toolCallId: null,
    jobId: null,
    approvalId: null,
    deviceId: null,
    sourceId: null,
    sourceSeq: null,
    evidenceId: null,
    data: {},
    ...extra,
  };
}

interface Fixture {
  projection: Json;
  events: Json[];
}

function projection(
  id: string,
  options: {
    status?: string;
    devices?: Json[];
    approvals?: Json[];
    jobs?: Json[];
    evidence?: Json[];
  } = {},
): Json {
  const status = options.status ?? 'active';
  return {
    task: task(id, status),
    runs: [],
    jobs: options.jobs ?? [],
    approvals: options.approvals ?? [],
    watches: [],
    evidence: options.evidence ?? [],
    associations: (options.devices ?? []).map((item) => ({
      deviceId: item.deviceId,
      sourceIds: (item.sources as Json[]).map((source) => source.id),
    })),
    devices: options.devices ?? [],
    summary: null,
    cursor: 0,
  };
}

const videoDevices = [
  {
    ...device('cam-allowed'),
    fields: [{ name: 'status', value: 'online', unit: null }],
  },
];

function runningJob(id: string, deviceId = 'cam-allowed'): Json {
  return {
    workspaceId,
    deviceId,
    tool: 'call_device',
    argsJson: { action: 'snapshot' },
    state: 'running',
    stopRequestedAt: null,
    progressJson: null,
    resultJson: null,
    taskId: 'task_fixture',
    runId: 'run_fixture',
    toolCallId: 'call_fixture',
    driverCallId: 'driver_fixture',
    timeoutMs: 30_000,
    startedAt: '2026-10-27T10:00:00Z',
    endedAt: null,
    id,
  };
}

function evidence(id: string, deviceId = 'cam-allowed', expiresAt = future(60 * 24 * 7)): Json {
  return {
    evidenceId: id,
    mime: 'image/jpeg',
    sourceSeq: 1,
    mediaTime: '2026-10-27T10:01:00Z',
    expiresAt,
    deviceId,
    sourceId: 'live',
    taskId: 'task_video',
    runId: 'run_task_video',
    toolCallId: null,
    jobId: null,
  };
}

const fixtures: Record<string, Fixture> = {
  task_video: {
    projection: projection('task_video', {
      devices: videoDevices,
      jobs: [runningJob('job_video')],
      evidence: [evidence('evidence_video')],
    }),
    events: [runtimeEvent('task_video', 1, 'read_device', { deviceId: 'cam-allowed' })],
  },
  task_timeline: {
    projection: projection('task_timeline', { devices: videoDevices }),
    events: [
      runtimeEvent('task_timeline', 1, 'run.started', { runId: 'run_task_timeline' }),
      runtimeEvent('task_timeline', 2, 'reason.updated', { text: 'Checking the entrance camera' }),
      runtimeEvent('task_timeline', 3, 'read_device', { deviceId: 'cam-allowed' }),
    ],
  },
  task_timeline_paged: {
    projection: projection('task_timeline_paged', { devices: videoDevices }),
    events: Array.from({ length: 120 }, (_, index) => {
      const cursor = index + 1;
      return cursor % 2 === 1
        ? runtimeEvent('task_timeline_paged', cursor, 'run.note', { text: `step ${cursor}` })
        : runtimeEvent('task_timeline_paged', cursor, 'read_device', { deviceId: 'cam-allowed' });
    }),
  },
  task_two_cameras: {
    projection: projection('task_two_cameras', {
      devices: [device('cam-a'), device('cam-b')],
    }),
    events: [
      runtimeEvent('task_two_cameras', 1, 'read_device', { deviceId: 'cam-a' }),
      runtimeEvent('task_two_cameras', 2, 'read_device', { deviceId: 'cam-b' }),
    ],
  },
  task_camera_and_sensor: {
    projection: projection('task_camera_and_sensor', {
      devices: [
        device('cam-a'),
        {
          ...device('sensor-only', 'none'),
          fields: [{ name: 'temperature', value: '21', unit: 'C' }],
        },
      ],
    }),
    events: [
      runtimeEvent('task_camera_and_sensor', 1, 'read_device', { deviceId: 'cam-a' }),
      runtimeEvent('task_camera_and_sensor', 2, 'read_device', { deviceId: 'sensor-only' }),
    ],
  },
  task_expired_mjpeg: {
    projection: projection('task_expired_mjpeg', { devices: [device('cam-expired-mjpeg')] }),
    events: [],
  },
  task_unsafe_playback: {
    projection: projection('task_unsafe_playback', { devices: [device('cam-unsafe')] }),
    events: [],
  },
  task_expired_evidence: {
    projection: projection('task_expired_evidence', {
      devices: videoDevices,
      evidence: [evidence('evidence_expired', 'cam-allowed', '2026-10-27T10:05:00Z')],
    }),
    events: [],
  },
  task_cors_allowed: {
    projection: projection('task_cors_allowed', { devices: videoDevices }),
    events: [],
  },
  task_cors_denied: {
    projection: projection('task_cors_denied', { devices: [device('cam-denied')] }),
    events: [],
  },
  task_expired_signature: {
    projection: projection('task_expired_signature', { devices: [device('cam-expired')] }),
    events: [],
  },
  task_unsupported_protocol: {
    projection: projection('task_unsupported_protocol', { devices: [device('cam-webrtc')] }),
    events: [],
  },
  task_without_video: {
    projection: projection('task_without_video', {
      devices: [
        {
          ...device('sensor-only', 'none'),
          fields: [{ name: 'temperature', value: '21', unit: 'C' }],
        },
      ],
    }),
    events: [],
  },
  task_history: {
    projection: projection('task_history', {
      status: 'completed',
      devices: videoDevices,
      evidence: [evidence('evidence_history')],
    }),
    events: [
      runtimeEvent('task_history', 1, 'read_device', { deviceId: 'cam-allowed' }),
      runtimeEvent('task_history', 2, 'evidence.created', {
        deviceId: 'cam-allowed',
        sourceId: 'live',
        evidenceId: 'evidence_history',
      }),
    ],
  },
  task_approval: {
    projection: projection('task_approval', {
      status: 'needs_approval',
      devices: videoDevices,
      approvals: [approval('approval_once', 'task_approval')],
    }),
    events: [],
  },
  task_reject: {
    projection: projection('task_reject', {
      status: 'needs_approval',
      devices: videoDevices,
      approvals: [approval('approval_reject', 'task_reject')],
    }),
    events: [],
  },
  task_approval_conflict: {
    projection: projection('task_approval_conflict', {
      status: 'needs_approval',
      devices: videoDevices,
      approvals: [approval('approval_conflict', 'task_approval_conflict')],
    }),
    events: [],
  },
  task_approval_expired: {
    projection: projection('task_approval_expired', {
      status: 'needs_approval',
      devices: videoDevices,
      approvals: [approval('approval_expired', 'task_approval_expired', 'expired')],
    }),
    events: [],
  },
  task_approval_unavailable: {
    projection: projection('task_approval_unavailable', {
      status: 'needs_approval',
      devices: videoDevices,
      approvals: [approval('approval_unavailable', 'task_approval_unavailable', 'unavailable')],
    }),
    events: [],
  },
  task_approval_delayed: {
    projection: projection('task_approval_delayed', {
      status: 'needs_approval',
      devices: videoDevices,
      approvals: [approval('approval_delayed', 'task_approval_delayed')],
    }),
    events: [],
  },
  task_approval_history: {
    projection: projection('task_approval_history', {
      status: 'completed',
      devices: videoDevices,
      approvals: [approval('approval_historic', 'task_approval_history', 'approved')],
    }),
    events: [
      runtimeEvent('task_approval_history', 1, 'approval.decided', {
        deviceId: 'cam-allowed',
        approvalId: 'approval_historic',
      }),
    ],
  },
  task_sse_resync: {
    projection: projection('task_sse_resync', { devices: videoDevices }),
    events: [runtimeEvent('task_sse_resync', 1, 'read_device', { deviceId: 'cam-allowed' })],
  },
  task_empty: {
    projection: projection('task_empty'),
    events: [],
  },
  task_partial: {
    projection: {
      ...projection('task_partial', { devices: [] }),
      associations: [
        { deviceId: 'ghost-device', sourceIds: ['camera'] },
        { deviceId: 'cam-allowed', sourceIds: ['live'] },
      ],
      evidence: [{ ...evidence('evidence_orphan', 'ghost-device'), sourceId: 'camera' }],
    },
    events: [],
  },
  task_offline: {
    projection: projection('task_offline', { devices: videoDevices }),
    events: [],
  },
  task_error: {
    projection: projection('task_error', { devices: videoDevices }),
    events: [],
  },
  task_cancel: {
    projection: projection('task_cancel', {
      devices: videoDevices,
      jobs: [runningJob('job_cancel')],
    }),
    events: [],
  },
};

const taskOrder = Object.keys(fixtures);

function taskProjection(id: string): Json | null {
  return fixtures[id]?.projection ?? null;
}

function currentTask(id: string): Json | null {
  const found = taskProjection(id);
  return found ? (found.task as Json) : null;
}

function setApprovalState(taskId: string, state: string, decision: string | null): Json | null {
  const found = taskProjection(taskId);
  if (!found) return null;
  const approvals = found.approvals as Json[];
  const existing = approvals[0];
  if (!existing) return null;
  existing.state = state;
  existing.decision = decision;
  existing.deciderId = 'user_e2e';
  existing.decidedAt = now();
  return existing;
}

function playbackFor(deviceId: string): Json | null {
  if (deviceId === 'cam-expired') {
    return {
      protocol: 'hls',
      url: `${origin}/allowed/live.m3u8?sig=expired`,
      expiresAt: '2026-10-27T10:05:00Z',
    };
  }
  if (deviceId === 'cam-expired-mjpeg') {
    return {
      protocol: 'mjpeg',
      url: `${origin}/allowed/frame-a.png?sig=expired`,
      expiresAt: '2026-10-27T10:05:00Z',
    };
  }
  if (deviceId === 'cam-unsafe') {
    return {
      protocol: 'mjpeg',
      url: 'javascript:alert(1)',
      expiresAt: future(),
    };
  }
  if (deviceId === 'cam-webrtc') {
    return {
      protocol: 'webrtc',
      url: `${origin}/allowed/live.m3u8?sig=fixture`,
      expiresAt: future(),
    };
  }
  if (deviceId === 'cam-a' || deviceId === 'cam-b') {
    return {
      protocol: 'mjpeg',
      url: `${origin}/allowed/frame-${deviceId === 'cam-a' ? 'a' : 'b'}.png`,
      expiresAt: future(),
    };
  }
  const path = deviceId === 'cam-denied' ? 'denied' : 'allowed';
  return { protocol: 'hls', url: `${origin}/${path}/live.m3u8?sig=fixture`, expiresAt: future() };
}

function json(
  res: ServerResponse,
  status: number,
  payload: unknown,
  headers: Record<string, string> = {},
): void {
  res.writeHead(status, {
    'Content-Type': 'application/json',
    'Cache-Control': 'no-store',
    ...headers,
  });
  res.end(JSON.stringify(payload));
}

function ok(res: ServerResponse, result: unknown): void {
  json(res, 200, { code: 0, msg: '', result });
}

function apiError(res: ServerResponse, status: number, message: string): void {
  json(res, status, { code: status, msg: message, result: null });
}

function handleEventStream(
  req: IncomingMessage,
  res: ServerResponse,
  taskId: string,
  after: number,
): void {
  if (taskId === 'task_offline') {
    apiError(res, 503, 'stream offline');
    return;
  }
  res.writeHead(200, {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-store',
    Connection: 'keep-alive',
  });
  const fixture = fixtures[taskId];
  if (!fixture) {
    res.end();
    return;
  }
  const snapshot = fixture.projection;
  const send = (event: string, data: unknown): void => {
    res.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
  };
  send('snapshot', { snapshot, cursor: snapshot.cursor });

  if (taskId === 'task_sse_resync') {
    // Exercise duplicate suppression with an event newer than the seeded page,
    // then an authoritative cursor reset.
    const next = Math.max(after, Number(fixture.events[0]?.cursor ?? 0)) + 1;
    const event = { ...fixture.events[0], cursor: next, eventId: `event_${next}` };
    send('event', { event });
    send('event', { event });
    send('resync', { snapshot, cursor: 0 });
  } else {
    for (const event of fixture.events) {
      if ((event.cursor as number) > after) send('event', { event });
    }
  }

  const keepalive = setInterval(() => res.write(': keepalive\n\n'), 15_000);
  req.on('close', () => clearInterval(keepalive));
  res.on('close', () => clearInterval(keepalive));
}

function serveMedia(_req: IncomingMessage, res: ServerResponse, path: string): void {
  counters.mediaRequests += 1;
  const allowed = path.startsWith('/allowed/');
  const relative = allowed ? path.slice('/allowed/'.length) : path.slice('/denied/'.length);
  const safe = normalize(relative).replace(/^(\.\.(\/|\\|$))+/, '');
  const file = join(mediaDir, safe);
  if (!file.startsWith(mediaDir) || !existsSync(file)) {
    res.writeHead(404);
    res.end('not found');
    return;
  }
  const headers: Record<string, string> = { 'Cache-Control': 'no-store' };
  if (allowed) headers['Access-Control-Allow-Origin'] = '*';
  res.writeHead(200, headers);
  createReadStream(file).pipe(res);
}

async function handle(req: IncomingMessage, res: ServerResponse): Promise<void> {
  const url = new URL(req.url ?? '/', origin);
  const path = decodeURIComponent(url.pathname);
  req.resume();

  if (path === '/__fixtures__/health') {
    json(res, 200, { ok: true });
    return;
  }
  if (path === '/__fixtures__/counts') {
    json(res, 200, counters);
    return;
  }
  if (path === '/__fixtures__/reset') {
    counters.mediaRequests = 0;
    counters.cancelRequests = 0;
    counters.decisionRequests = {};
    releaseDelayedDecision?.();
    releaseDelayedDecision = null;
    json(res, 200, { ok: true });
    return;
  }
  if (path === '/__fixtures__/release-decision') {
    releaseDelayedDecision?.();
    releaseDelayedDecision = null;
    json(res, 200, { ok: true });
    return;
  }
  if (path.startsWith('/allowed/') || path.startsWith('/denied/')) {
    serveMedia(req, res, path);
    return;
  }

  if (path === '/api/v1/users/me') {
    ok(res, { id: 'user_e2e', name: 'E2E User', role: 'admin' });
    return;
  }

  const api = path.match(/^\/api\/v1\/workspaces\/([^/]+)\/(.*)$/);
  if (!api) {
    apiError(res, 404, 'fixture route missing');
    return;
  }
  const [, workspace, rest] = api;
  if (workspace !== workspaceId) {
    apiError(res, 404, 'workspace missing');
    return;
  }

  if (rest === 'heartbeat/config') {
    ok(res, {
      runtimeTasksEnabled: true,
      runtimeMediaEnabled: true,
      runtimeWorkbenchEnabled: true,
    });
    return;
  }
  if (rest === 'users/me') {
    ok(res, { id: 'user_e2e', name: 'E2E User', role: 'admin' });
    return;
  }
  if (rest === 'agent/tasks') {
    ok(res, {
      tasks: taskOrder.map((id) => currentTask(id)).filter((item): item is Json => item !== null),
      limit: 50,
      offset: 0,
    });
    return;
  }

  const detail = rest.match(/^agent\/tasks\/([^/]+)$/);
  if (detail) {
    const taskId = detail[1];
    if (taskId === 'task_error') {
      apiError(res, 500, 'fixture failure');
      return;
    }
    const found = taskProjection(taskId);
    if (!found) {
      apiError(res, 404, 'Task not found');
      return;
    }
    ok(res, found);
    return;
  }

  const stream = rest.match(/^agent\/tasks\/([^/]+)\/stream$/);
  if (stream) {
    const afterParam = Number(url.searchParams.get('after') ?? '0');
    handleEventStream(req, res, stream[1], Number.isFinite(afterParam) ? afterParam : 0);
    return;
  }

  const timeline = rest.match(/^agent\/tasks\/([^/]+)\/events$/);
  if (timeline) {
    const fixture = fixtures[timeline[1]];
    if (!fixture) {
      apiError(res, 404, 'Task not found');
      return;
    }
    const requestedLimit = Number(url.searchParams.get('limit'));
    const limit = Number.isFinite(requestedLimit)
      ? Math.min(200, Math.max(1, Math.floor(requestedLimit)))
      : 100;
    const beforeParam = url.searchParams.get('before');
    const before = beforeParam === null ? null : Number(beforeParam);
    const eligible =
      before !== null && Number.isFinite(before)
        ? fixture.events.filter((event) => (event.cursor as number) < before)
        : fixture.events;
    const page = eligible.slice(-limit);
    ok(res, {
      events: page,
      oldestCursor: page.length ? page[0].cursor : null,
      hasMore: eligible.length > page.length,
    });
    return;
  }

  const playback = rest.match(/^devices\/([^/]+)\/sources\/([^/]+)\/playback$/);
  if (playback) {
    const descriptor = playbackFor(playback[1]);
    if (!descriptor) {
      apiError(res, 503, 'playback_unavailable');
      return;
    }
    ok(res, descriptor);
    return;
  }

  const decision = rest.match(/^agent\/tasks\/([^/]+)\/approvals\/([^/]+)\/(approve|reject)$/);
  if (decision) {
    const taskId = decision[1];
    counters.decisionRequests[taskId] = (counters.decisionRequests[taskId] ?? 0) + 1;
    if (taskId === 'task_approval_conflict') {
      apiError(res, 409, 'Approval decision conflict');
      return;
    }
    if (taskId === 'task_approval_delayed') {
      await new Promise<void>((resolve) => {
        releaseDelayedDecision = resolve;
      });
    }
    const reject = decision[3] === 'reject';
    const updated = setApprovalState(
      taskId,
      reject ? 'rejected' : 'approved',
      reject ? 'reject' : 'approve',
    );
    if (!updated) {
      apiError(res, 404, 'Approval not found');
      return;
    }
    ok(res, { approval: updated, deduplicated: false });
    return;
  }

  const control = rest.match(/^agent\/tasks\/([^/]+)\/(pause|resume|cancel|stop)$/);
  if (control) {
    const taskId = control[1];
    if (control[2] === 'cancel') counters.cancelRequests += 1;
    const found = taskProjection(taskId);
    if (!found) {
      apiError(res, 404, 'Task not found');
      return;
    }
    if (control[2] === 'cancel') {
      (found.task as Json).status = 'cancelled';
      (found.task as Json).endedAt = now();
    } else if (control[2] === 'pause') {
      (found.task as Json).status = 'paused';
    } else if (control[2] === 'resume') {
      (found.task as Json).status = 'active';
    }
    ok(res, found);
    return;
  }

  apiError(res, 404, 'fixture route missing');
}

createServer((req, res) => {
  handle(req, res).catch((error: unknown) => {
    apiError(res, 500, error instanceof Error ? error.message : 'fixture failure');
  });
}).listen(port, '127.0.0.1', () => {
  console.log(`runtime fixture server listening on ${origin}`);
});
