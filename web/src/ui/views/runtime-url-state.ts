export type RuntimeView = 'loop' | 'flow';

export interface RuntimeHistory {
  at?: string;
  event?: string;
}

export type RuntimeRoute =
  | { name: 'list' }
  | { name: 'new' }
  | {
      name: 'task';
      taskId: string;
      view: RuntimeView;
      device: string | null;
      history: RuntimeHistory | null;
    };

export interface RuntimeTaskUrlState {
  view?: RuntimeView;
  device?: string | null;
  at?: string | null;
  event?: string | null;
}

function queryFromHash(hash: string): { path: string; params: URLSearchParams } {
  const queryIndex = hash.indexOf('?');
  return {
    path: queryIndex < 0 ? hash : hash.slice(0, queryIndex),
    params: new URLSearchParams(queryIndex < 0 ? '' : hash.slice(queryIndex + 1)),
  };
}

function decodeTaskId(value: string): string | null {
  try {
    const taskId = decodeURIComponent(value);
    return taskId ? taskId : null;
  } catch {
    return null;
  }
}

export function parseRuntimeHash(hash: string): RuntimeRoute | null {
  const { path, params } = queryFromHash(hash);
  if (path === '#/runtime') return { name: 'list' };
  if (path === '#/runtime/new') return { name: 'new' };
  const match = path.match(/^#\/runtime\/tasks\/([^/]+)$/);
  if (!match) return null;
  const taskId = decodeTaskId(match[1]);
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
  };
}

export function toRuntimeTaskUrl(taskId: string, state: RuntimeTaskUrlState = {}): string {
  const params = new URLSearchParams();
  if (state.view === 'flow') params.set('view', 'flow');
  if (state.device) params.set('device', state.device);
  if (state.at) params.set('at', state.at);
  if (state.event) params.set('event', state.event);
  const query = params.toString();
  return `#/runtime/tasks/${encodeURIComponent(taskId)}${query ? `?${query}` : ''}`;
}

export function switchTaskUrl(taskId: string, view: RuntimeView = 'loop'): string {
  return toRuntimeTaskUrl(taskId, { view });
}

export function replaceRuntimeTask(taskId: string, view: RuntimeView = 'loop'): void {
  window.history.replaceState({}, '', switchTaskUrl(taskId, view));
  window.dispatchEvent(new HashChangeEvent('hashchange'));
}
