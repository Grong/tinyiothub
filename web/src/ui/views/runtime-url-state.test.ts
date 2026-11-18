import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  isHistoryState,
  parseRuntimeHash,
  replaceRuntimeTask,
  switchTaskUrl,
  toLiveRuntimeTaskUrl,
  toRuntimeTaskUrl,
} from './runtime-url-state.js';

afterEach(() => {
  window.history.replaceState({}, '', '/');
  vi.restoreAllMocks();
});

describe('Runtime hash URL state', () => {
  it('parses exactly the list, new and Task routes', () => {
    expect(parseRuntimeHash('#/runtime')).toMatchObject({ name: 'list' });
    expect(parseRuntimeHash('#/runtime/new')).toMatchObject({ name: 'new' });
    expect(parseRuntimeHash('#/runtime/tasks/task%3A1')).toMatchObject({
      name: 'task', taskId: 'task:1', view: 'loop', history: null,
    });
    expect(parseRuntimeHash('#/runtime/other')).toBeNull();
    expect(parseRuntimeHash('/runtime')).toBeNull();
  });

  it('enters history only for at or event and clears task state on switch', () => {
    expect(parseRuntimeHash('#/runtime/tasks/t1?view=flow&device=d1')).toMatchObject({
      view: 'flow', device: 'd1', history: null,
    });
    expect(parseRuntimeHash('#/runtime/tasks/t1?event=e2')).toMatchObject({ history: { event: 'e2' } });
    expect(parseRuntimeHash('#/runtime/tasks/t1?at=2026-10-27T10%3A00%3A00Z')).toMatchObject({
      history: { at: '2026-10-27T10:00:00Z' },
    });
    expect(switchTaskUrl('t2', 'flow')).toBe('#/runtime/tasks/t2?view=flow');
  });

  it('round-trips shared view, device and history state', () => {
    const url = toRuntimeTaskUrl('task:1', {
      view: 'flow', device: 'dev/1', at: '2026-10-27T10:00:00Z', event: 'event:2',
    });
    expect(url).toBe('#/runtime/tasks/task%3A1?view=flow&device=dev%2F1&at=2026-10-27T10%3A00%3A00Z&event=event%3A2');
    expect(parseRuntimeHash(url)).toMatchObject({
      taskId: 'task:1', view: 'flow', device: 'dev/1',
      history: { at: '2026-10-27T10:00:00Z', event: 'event:2' },
    });
  });

  it('detects history only from at or event and builds a live URL that preserves Task, view and device', () => {
    expect(isHistoryState(null)).toBe(false);
    expect(isHistoryState({})).toBe(false);
    expect(isHistoryState({ event: 'e2' })).toBe(true);
    expect(isHistoryState({ at: '2026-10-27T10:00:00Z' })).toBe(true);

    expect(toLiveRuntimeTaskUrl('task:1', { view: 'flow', device: 'dev/1' }))
      .toBe('#/runtime/tasks/task%3A1?view=flow&device=dev%2F1');
    expect(toLiveRuntimeTaskUrl('t1')).toBe('#/runtime/tasks/t1');
  });

  it('replace-navigates after create and notifies the hash router', () => {
    const replace = vi.spyOn(window.history, 'replaceState');
    const listener = vi.fn();
    window.addEventListener('hashchange', listener, { once: true });

    replaceRuntimeTask('task:1');

    expect(replace).toHaveBeenCalledWith({}, '', '#/runtime/tasks/task%3A1');
    expect(listener).toHaveBeenCalledOnce();
  });
});
