/**
 * Canonical Harness hash routes plus the one-way legacy Runtime mapping.
 *
 * Canonical:
 *   #/harness
 *   #/harness/supervisor
 *   #/harness/compaction
 *   #/harness/tasks
 *   #/harness/tasks/new
 *   #/harness/tasks/{task_id}?view=flow&device=…&at=…&event=…&from=…
 *
 * Old #/runtime list/new/task links are redirected once by the app router and
 * keep the encoded Task ID and replay/device query unchanged. Retired
 * #/harness/workflows… links fall back to the overview instead of breaking a
 * stale bookmark.
 */

import {
  decodeHashId,
  parseRuntimeHash,
  splitHashQuery,
  type RuntimeHistory,
  type RuntimeTaskUrlState,
  type RuntimeView,
} from './runtime-url-state.js';

export type HarnessRoute =
  | { name: 'overview'; anchor?: string }
  | { name: 'supervisor' }
  | { name: 'compaction' }
  | { name: 'tasks' }
  | { name: 'new' }
  | {
      name: 'task';
      taskId: string;
      view: RuntimeView;
      device: string | null;
      history: RuntimeHistory | null;
      from: string | null;
    };

export interface HarnessTaskUrlState extends RuntimeTaskUrlState {
  /** Safe in-app parent location, e.g. `#/harness#main-task:1`. */
  from?: string | null;
}

/** Only in-app Harness locations are valid return paths (no javascript: URLs). */
function safeReturnPath(value: string | null): string | null {
  if (!value) return null;
  return value === '#/harness' || value.startsWith('#/harness/') || value.startsWith('#/harness#')
    ? value
    : null;
}

export function parseHarnessHash(hash: string): HarnessRoute | null {
  const { path, params } = splitHashQuery(hash);
  if (path === '#/harness') return { name: 'overview' };
  if (path.startsWith('#/harness#')) {
    const anchor = decodeHashId(path.slice('#/harness#'.length));
    return anchor ? { name: 'overview', anchor } : { name: 'overview' };
  }
  if (path === '#/harness/tasks') return { name: 'tasks' };
  if (path === '#/harness/tasks/new') return { name: 'new' };
  // The Workflow experiment was retired. Any stale workflow/template/dry-run
  // hash lands on the overview rather than rendering a broken route.
  if (path === '#/harness/workflows' || path.startsWith('#/harness/workflows/')) {
    return { name: 'overview' };
  }
  if (path === '#/harness/supervisor') {
    return { name: 'supervisor' };
  }
  if (path === '#/harness/compaction') {
    return { name: 'compaction' };
  }

  const taskMatch = path.match(/^#\/harness\/tasks\/([^/]+)$/);
  if (!taskMatch) return null;
  const taskId = decodeHashId(taskMatch[1]);
  if (!taskId) return null;

  const at = params.get('at');
  const event = params.get('event');
  const history: RuntimeHistory = {};
  if (at) history.at = at;
  if (event) history.event = event;
  return {
    name: 'task',
    taskId,
    view: params.get('view') === 'flow' ? 'flow' : 'loop',
    device: params.get('device') || null,
    history: at || event ? history : null,
    from: safeReturnPath(params.get('from')),
  };
}

export function toHarnessTaskUrl(taskId: string, state: HarnessTaskUrlState = {}): string {
  const params = new URLSearchParams();
  if (state.view === 'flow') params.set('view', 'flow');
  if (state.device) params.set('device', state.device);
  if (state.at) params.set('at', state.at);
  if (state.event) params.set('event', state.event);
  const from = safeReturnPath(state.from ?? null);
  if (from) params.set('from', from);
  const query = params.toString();
  return `#/harness/tasks/${encodeURIComponent(taskId)}${query ? `?${query}` : ''}`;
}

/** Map a legacy #/runtime hash to its canonical Harness URL, or null if invalid. */
export function runtimeToHarnessUrl(hash: string): string | null {
  const route = parseRuntimeHash(hash);
  if (!route) return null;
  if (route.name === 'list') return '#/harness/tasks';
  if (route.name === 'new') return '#/harness/tasks/new';
  return toHarnessTaskUrl(route.taskId, {
    view: route.view,
    device: route.device,
    at: route.history?.at,
    event: route.history?.event,
  });
}
