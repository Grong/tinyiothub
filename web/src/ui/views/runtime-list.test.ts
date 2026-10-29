import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { listRuntimeTasks } from '../../api/runtime.js';
import type { RuntimeTask } from '../../types/runtime.js';
import './runtime-list.js';
import type { RuntimeListView } from './runtime-list.js';

vi.mock('../../api/runtime.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/runtime.js')>();
  return { ...actual, listRuntimeTasks: vi.fn() };
});

const list = vi.mocked(listRuntimeTasks);
let view: RuntimeListView;

function task(id: string, status: RuntimeTask['status'], updatedAt: string): RuntimeTask {
  return {
    id,
    workspaceId: 'ws_1',
    idempotencyKey: `key_${id}`,
    objective: `Objective ${id}`,
    sessionKey: null,
    createdBy: 'user_1',
    status,
    currentRunId: null,
    stateVersion: 0,
    createdAt: updatedAt,
    updatedAt,
    endedAt: null,
  };
}

beforeEach(() => {
  localStorage.setItem('workspace-id', 'ws_1');
  window.history.replaceState({}, '', '/');
});

afterEach(() => {
  view?.remove();
  localStorage.clear();
  vi.clearAllMocks();
});

async function mount(): Promise<void> {
  view = document.createElement('view-runtime-list') as RuntimeListView;
  document.body.append(view);
  await view.updateComplete;
  await Promise.resolve();
  await view.updateComplete;
}

describe('Runtime Task list shell', () => {
  it('shows Current/Recent groups, Task links and New task action', async () => {
    list.mockResolvedValue({
      code: 0,
      msg: '',
      result: {
        tasks: [
          task('current', 'paused', '2026-10-27T12:00:00Z'),
          task('recent', 'completed', '2026-10-27T11:00:00Z'),
        ],
        limit: 50,
        offset: 0,
      },
    });
    await mount();

    expect(view.querySelector('h1')?.textContent).toBe('Runtime');
    expect(view.textContent).toContain('Current');
    expect(view.textContent).toContain('Recent');
    expect(view.textContent).toContain('Online');
    expect(view.querySelector('a[href="#/runtime/new"]')?.textContent).toContain('New task');
    expect(view.querySelector('a[href="#/runtime/tasks/current"]')?.textContent).toContain(
      'Objective current',
    );
    expect(view.querySelector('a[href="#/runtime/tasks/recent"]')?.textContent).toContain(
      'Objective recent',
    );
  });

  it('shows an empty state and pagination based on the server offset', async () => {
    list.mockResolvedValue({ code: 0, msg: '', result: { tasks: [], limit: 20, offset: 20 } });
    await mount();
    expect(view.textContent).toContain('No Runtime tasks');
    expect(view.querySelector<HTMLButtonElement>('.runtime-next')?.disabled).toBe(true);
    expect(view.querySelector<HTMLButtonElement>('.runtime-previous')?.disabled).toBe(false);
  });

  it('loads the next page and exposes failures as Offline', async () => {
    list.mockResolvedValue({
      code: 0,
      msg: '',
      result: {
        tasks: [
          task('one', 'active', '2026-10-27T12:00:00Z'),
          task('two', 'active', '2026-10-27T11:00:00Z'),
        ],
        limit: 2,
        offset: 0,
      },
    });
    await mount();
    list.mockRejectedValueOnce(new TypeError('offline'));
    view.querySelector<HTMLButtonElement>('.runtime-next')!.click();
    await view.updateComplete;
    await Promise.resolve();
    await view.updateComplete;

    expect(list.mock.calls[1].slice(0, 3)).toEqual(['ws_1', 2, 2]);
    expect(view.textContent).toContain('Offline');
    expect(view.textContent).toContain('Network error');

    // The failed page must not advance the offset; retry asks for the same page.
    list.mockResolvedValueOnce({
      code: 0,
      msg: '',
      result: {
        tasks: [task('three', 'active', '2026-10-27T10:00:00Z')],
        limit: 2,
        offset: 2,
      },
    });
    view.querySelector<HTMLButtonElement>('.runtime-next')!.click();
    await view.updateComplete;
    await Promise.resolve();
    await view.updateComplete;

    expect(list.mock.calls[2].slice(0, 3)).toEqual(['ws_1', 2, 2]);
    expect(view.textContent).toContain('Online');
  });
});
