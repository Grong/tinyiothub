import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client.js';
import { createRuntimeTask } from '../../api/runtime.js';
import './runtime-new.js';
import type { RuntimeNewView } from './runtime-new.js';

vi.mock('../../api/runtime.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/runtime.js')>();
  return { ...actual, createRuntimeTask: vi.fn() };
});

const create = vi.mocked(createRuntimeTask);
let view: RuntimeNewView;

beforeEach(() => {
  window.history.replaceState({}, '', '/');
  view = document.createElement('view-runtime-new') as RuntimeNewView;
  view.workspaceId = 'ws_1';
  document.body.append(view);
});

afterEach(() => {
  view.remove();
  vi.clearAllMocks();
});

async function update(): Promise<void> {
  await view.updateComplete;
}

function setObjective(value: string): void {
  const input = view.querySelector('textarea')!;
  input.value = value;
  input.dispatchEvent(new Event('input', { bubbles: true }));
}

function submit(): void {
  view.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
}

describe('Runtime new Task shell', () => {
  it('renders an Objective label, continuous-Task explanation and Create/Cancel actions', async () => {
    await update();
    const label = view.querySelector('label');
    expect(label?.textContent).toContain('Objective');
    expect(label?.getAttribute('for')).toBe(view.querySelector('textarea')?.id);
    expect(view.textContent).toContain('continuous');
    expect(view.querySelector('button[type="submit"]')?.textContent).toContain('Create task');
    expect(view.querySelector('button[type="button"]')?.textContent).toContain('Cancel');
  });

  it('prevents duplicate pending submits and preserves the objective', async () => {
    let resolve!: (value: Awaited<ReturnType<typeof createRuntimeTask>>) => void;
    create.mockReturnValue(new Promise((done) => { resolve = done; }));
    await update();
    setObjective('  Inspect pumps  ');

    submit();
    submit();
    await update();

    expect(create).toHaveBeenCalledOnce();
    expect(create.mock.calls[0].slice(0, 3)).toEqual(['ws_1', 'Inspect pumps', expect.any(String)]);
    expect(view.querySelector('textarea')?.value).toBe('  Inspect pumps  ');
    expect(view.querySelector('button[type="submit"]')?.textContent).toContain('Creating task');
    expect(view.querySelector<HTMLButtonElement>('button[type="submit"]')?.disabled).toBe(true);

    resolve({ code: 0, msg: '', result: { taskId: 'task_1', status: 'accepted', deduplicated: false } });
    await update();
  });

  it.each([400, 409])('shows HTTP %s without silently retrying', async (status) => {
    create.mockRejectedValue(new ApiError(status, `HTTP ${status}`));
    await update();
    setObjective('Inspect pumps');
    submit();
    await update();
    await update();

    expect(create).toHaveBeenCalledOnce();
    expect(view.textContent).toContain(`HTTP ${status}`);
    expect(view.querySelector('textarea')?.value).toBe('Inspect pumps');
  });

  it('reuses the key for a manual network retry and replace-navigates after acceptance', async () => {
    create.mockRejectedValueOnce(new TypeError('network down'));
    await update();
    setObjective('Inspect pumps');
    submit();
    await update();
    await update();
    expect(view.textContent).toContain('Network error');
    const firstKey = create.mock.calls[0][2];

    create.mockResolvedValueOnce({
      code: 0, msg: '', result: { taskId: 'task:1', status: 'accepted', deduplicated: false },
    });
    const replace = vi.spyOn(window.history, 'replaceState');
    submit();
    await update();
    await update();

    expect(create.mock.calls[1][2]).toBe(firstKey);
    expect(replace).toHaveBeenCalledWith({}, '', '#/runtime/tasks/task%3A1');
    expect(window.location.hash).toBe('#/runtime/tasks/task%3A1');
  });

  it('Cancel returns to Runtime without creating a Task', async () => {
    await update();
    view.querySelector<HTMLButtonElement>('button[type="button"]')!.click();
    expect(window.location.hash).toBe('#/runtime');
    expect(create).not.toHaveBeenCalled();
  });
});
