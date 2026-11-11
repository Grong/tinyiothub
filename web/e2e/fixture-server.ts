/**
 * Local-only assistant browser fixture.
 *
 * Serves synthetic assistant session/HUD projections for Playwright. It never
 * talks to production or reads credentials; the retired Runtime/Harness
 * workbench endpoints were removed together with their views.
 */
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';

const port = Number(process.env.FIXTURE_PORT ?? process.env.E2E_FIXTURE_PORT ?? 3002);
const workspaceId = 'ws-e2e';
const sessionKey = 'agent:ws-e2e:default/e2e';
const origin = `http://127.0.0.1:${port}`;

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

async function handle(req: IncomingMessage, res: ServerResponse): Promise<void> {
  const url = new URL(req.url ?? '/', origin);
  const path = decodeURIComponent(url.pathname);
  req.resume();

  if (path === '/__fixtures__/health') {
    json(res, 200, { ok: true });
    return;
  }

  if (path === '/api/v1/users/me') {
    ok(res, { id: 'user_e2e', name: 'E2E User', role: 'admin' });
    return;
  }

  // App shell polls the disposition badge on every authenticated route.
  if (path === '/api/v1/judgments/summary') {
    ok(res, { needsYou: 0 });
    return;
  }

  // Assistant workbench projections (envelope for ApiClient routes, raw JSON
  // for the HUD/events/todos endpoints).
  if (path === '/api/v1/chat/sessions') {
    ok(res, {
      sessions: [
        {
          session_key: sessionKey,
          workspace_id: workspaceId,
          agent_id: 'default',
          label: '巡检',
          kind: 'chat',
          status: 'active',
          created_at: 1,
          updated_at: 1,
        },
        {
          session_key: 'agent:ws-e2e:__assistant__ws-e2e/scheduled-1',
          workspace_id: workspaceId,
          agent_id: '__assistant__ws-e2e',
          label: '巡检 · 当前',
          kind: 'scheduled',
          status: 'active',
          created_at: 2,
          updated_at: 2,
        },
      ],
    });
    return;
  }
  if (path === '/api/v1/chat/history') {
    ok(res, { messages: [] });
    return;
  }
  // Session keys contain both ':' and '/', so match the suffix greedily.
  const session = path.match(/^\/api\/v1\/chat\/sessions\/(.+)\/(events\/stream|events|todos)$/);
  if (session) {
    const suffix = session[2];
    if (suffix === 'events') {
      json(res, 200, { events: [], highWatermark: 0 });
      return;
    }
    if (suffix === 'todos') {
      json(res, 200, []);
      return;
    }
    // Keep the session event stream open so the view does not churn retries.
    res.writeHead(200, {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-store',
      Connection: 'keep-alive',
    });
    res.write(': connected\n\n');
    const keepalive = setInterval(() => res.write(': keepalive\n\n'), 15_000);
    req.on('close', () => clearInterval(keepalive));
    res.on('close', () => clearInterval(keepalive));
    return;
  }
  if (path === `/api/v1/workspaces/${workspaceId}/agent/hud`) {
    json(res, 200, {
      assistant: { status: 'idle', lastOutcome: null },
      devices: { total: 3, online: 2, offline: 1 },
      alarms: { highPriority: 0 },
      pending: { questions: 0, approvals: 0 },
      activity: { activeSessions: 0 },
      schedule: {
        enabled: true,
        intervalSeconds: 300,
        prompt: '',
        nextWakeAt: null,
        lastWakeAt: null,
        lastOutcome: null,
      },
      recentEvents: [],
    });
    return;
  }

  apiError(res, 404, 'fixture route missing');
}

createServer((req, res) => {
  handle(req, res).catch((error: unknown) => {
    apiError(res, 500, error instanceof Error ? error.message : 'fixture failure');
  });
}).listen(port, '127.0.0.1', () => {
  console.log(`assistant fixture server listening on ${origin}`);
});
