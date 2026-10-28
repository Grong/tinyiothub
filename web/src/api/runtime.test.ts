import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiClient, ApiError } from './client.js';
import {
  createRuntimeTask,
  decideRuntimeApproval,
  getRuntimeTask,
  listRuntimeTasks,
  pauseRuntimeTask,
  RuntimeCreateModel,
} from './runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Runtime task transport', () => {
  it.each([400, 409])('sends the caller key and does not retry HTTP %i', async (code) => {
    const post = vi.spyOn(ApiClient, 'post').mockRejectedValue(new ApiError(code, 'rejected'));

    await expect(createRuntimeTask('ws_1', 'inspect', 'key_1')).rejects.toMatchObject({ code });

    expect(post).toHaveBeenCalledOnce();
    expect(post).toHaveBeenCalledWith(
      '/workspaces/ws_1/agent/tasks',
      { text: 'inspect' },
      { headers: { 'Idempotency-Key': 'key_1' }, signal: undefined },
    );
  });

  it('retains the key only for an unchanged network retry', () => {
    vi.stubGlobal('crypto', { randomUUID: vi.fn()
      .mockReturnValueOnce('key-first')
      .mockReturnValueOnce('key-second') });
    const form = new RuntimeCreateModel();

    const first = form.keyFor('inspect pumps');
    const retry = form.keyFor('inspect pumps');
    const edited = form.keyFor('inspect pumps and valves');

    expect(retry).toBe(first);
    expect(edited).not.toBe(first);
    vi.unstubAllGlobals();
  });

  it('uses the actual list/get/control/approval routes and forwards cancellation', async () => {
    const get = vi.spyOn(ApiClient, 'get').mockResolvedValue({ code: 0, msg: '', result: null });
    const post = vi.spyOn(ApiClient, 'post').mockResolvedValue({ code: 0, msg: '', result: null });
    const signal = new AbortController().signal;

    await listRuntimeTasks('ws/1', 20, 40, signal);
    await getRuntimeTask('ws/1', 'task:1', signal);
    await pauseRuntimeTask('ws/1', 'task:1', signal);
    await decideRuntimeApproval('ws/1', 'task:1', 'approval/1', 'approve', signal);

    expect(get).toHaveBeenNthCalledWith(
      1,
      '/workspaces/ws%2F1/agent/tasks',
      { limit: 20, offset: 40 },
      signal,
    );
    expect(get).toHaveBeenNthCalledWith(
      2,
      '/workspaces/ws%2F1/agent/tasks/task%3A1',
      undefined,
      signal,
    );
    expect(post).toHaveBeenNthCalledWith(
      1,
      '/workspaces/ws%2F1/agent/tasks/task%3A1/pause',
      {},
      { signal },
    );
    expect(post).toHaveBeenNthCalledWith(
      2,
      '/workspaces/ws%2F1/agent/tasks/task%3A1/approvals/approval%2F1/approve',
      {},
      { signal },
    );
  });
});
