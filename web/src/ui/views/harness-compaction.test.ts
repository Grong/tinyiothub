import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client.js';
import {
  compareHarnessCompaction,
  replaceHarnessCompactionSnapshot,
} from '../../api/harness.js';
import type {
  HarnessCompactionComparison,
  HarnessProjectionSnapshot,
} from '../../types/harness.js';
import './harness-compaction.js';
import type { HarnessCompactionView } from './harness-compaction.js';

vi.mock('../../api/harness.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/harness.js')>();
  return {
    ...actual,
    compareHarnessCompaction: vi.fn(),
    replaceHarnessCompactionSnapshot: vi.fn(),
  };
});

const compareCompaction = vi.mocked(compareHarnessCompaction);
const replaceSnapshot = vi.mocked(replaceHarnessCompactionSnapshot);
let view: HarnessCompactionView;

function comparison(
  overrides: Partial<HarnessCompactionComparison> = {},
): HarnessCompactionComparison {
  return {
    projectionName: 'supervisor',
    subjectId: 'supervisor:ws_1',
    projectionVersion: 1,
    boundaryLastSeq: 2,
    snapshotLastSeq: 2,
    snapshotStateVersion: 3,
    seqFirst: 1,
    seqLast: 2,
    lastCompleteSeq: 2,
    observedHighWatermark: 2,
    appendedDuringCompare: false,
    status: 'matched',
    matched: true,
    canonicalHash: 'rebuilt-hash-abc',
    snapshotCanonicalHash: 'rebuilt-hash-abc',
    canonicalState: { projectionName: 'supervisor' },
    differences: [],
    evidenceRefs: [
      { seq: 1, eventId: 'event-1', eventType: 'step.text', reference: 'event:event-1@1' },
      { seq: 2, eventId: 'event-2', eventType: 'step.text', reference: 'event:event-2@2' },
    ],
    canReplace: true,
    readOnly: false,
    ...overrides,
  };
}

function snapshot(overrides: Partial<HarnessProjectionSnapshot> = {}): HarnessProjectionSnapshot {
  return {
    projectionName: 'supervisor',
    subjectId: 'supervisor:ws_1',
    lastSeq: 2,
    projectionVersion: 1,
    stateVersion: 4,
    stateJson: { projectionName: 'supervisor' },
    updatedAt: '2026-11-03T00:00:00Z',
    ...overrides,
  };
}

function response<T>(result: T) {
  return { code: 0, msg: '', result };
}

async function settle(): Promise<void> {
  for (let index = 0; index < 6; index += 1) {
    await Promise.resolve();
    await view.updateComplete;
  }
  await new Promise((resolve) => setTimeout(resolve, 0));
  await view.updateComplete;
}

async function mount(): Promise<void> {
  view = document.createElement('view-harness-compaction') as HarnessCompactionView;
  view.workspaceId = 'ws_1';
  document.body.append(view);
  await settle();
}

function input(name: string): HTMLInputElement {
  return view.querySelector<HTMLInputElement>(`[data-field="${name}"]`)!;
}

function setInput(name: string, value: string): void {
  const field = input(name);
  field.value = value;
  field.dispatchEvent(new Event('input', { bubbles: true }));
}

async function submitCompare(): Promise<void> {
  const form = view.querySelector<HTMLFormElement>('[data-compaction-form]')!;
  form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  await settle();
}

function replaceButton(): HTMLButtonElement | null {
  return view.querySelector<HTMLButtonElement>('[data-action="replace"]');
}

function recompareButton(): HTMLButtonElement | null {
  return view.querySelector<HTMLButtonElement>('[data-action="recompare"]');
}

beforeEach(() => {
  document.body.innerHTML = '';
  compareCompaction.mockReset();
  replaceSnapshot.mockReset();
});

afterEach(() => {
  view?.remove();
});

describe('harness compaction inspector', () => {
  it('shows the fixed boundary, canonical hash and evidence, and replaces only on a match', async () => {
    compareCompaction.mockResolvedValue(response(comparison()));
    replaceSnapshot.mockResolvedValue(response(snapshot()));
    await mount();
    setInput('last-seq', '2');
    await submitCompare();

    expect(compareCompaction).toHaveBeenCalledWith(
      'ws_1',
      {
        projectionName: 'supervisor',
        subjectId: 'supervisor:ws_1',
        lastSeq: 2,
        projectionVersion: 1,
      },
      expect.anything(),
    );
    expect(view.querySelector('[data-compaction-status]')?.textContent).toContain(
      '同边界匹配',
    );
    const facts = view.querySelector('[data-compaction-facts]');
    expect(facts?.textContent).toContain('固定边界');
    expect(facts?.textContent).toContain('seq 1 – 2');
    const factValue = (label: string): string | undefined =>
      Array.from(facts?.querySelectorAll('div') ?? [])
        .find((row) => row.querySelector('dt')?.textContent === label)
        ?.querySelector('dd')?.textContent ?? undefined;
    expect(factValue('projection_version')).toContain('1');
    expect(factValue('last complete seq')).toContain('2');
    expect(view.querySelector('[data-canonical-hash]')?.textContent).toContain(
      'rebuilt-hash-abc',
    );
    expect(view.querySelector('[data-snapshot-hash]')?.textContent).toContain(
      'rebuilt-hash-abc',
    );
    expect(view.querySelectorAll('[data-evidence-seq]')).toHaveLength(2);

    expect(replaceButton()).not.toBeNull();
    replaceButton()?.click();
    await settle();
    expect(replaceSnapshot).toHaveBeenCalledWith('ws_1', {
      projectionName: 'supervisor',
      subjectId: 'supervisor:ws_1',
      lastSeq: 2,
      projectionVersion: 1,
      expectedLastSeq: 2,
      expectedStateVersion: 3,
    });
    expect(view.textContent).toContain('已替换投影快照');
    expect(replaceButton()?.disabled).toBe(true);
  });

  it('keeps the latest snapshot on a hash mismatch and offers recompare without auto retry', async () => {
    compareCompaction.mockResolvedValue(
      response(
        comparison({
          status: 'hash_mismatch',
          matched: false,
          canReplace: false,
          canonicalHash: 'rebuilt-hash',
          snapshotCanonicalHash: 'snapshot-hash',
          differences: [
            {
              path: '/events/0/payload/text',
              rebuilt: 'one',
              snapshot: 'tampered',
            },
          ],
        }),
      ),
    );
    await mount();
    setInput('last-seq', '2');
    await submitCompare();

    expect(view.querySelector('[data-compaction-status]')?.textContent).toContain(
      'Hash 不匹配',
    );
    expect(view.textContent).toContain('保留最新快照');
    expect(view.querySelector('[data-differences]')?.textContent).toContain(
      '/events/0/payload/text',
    );
    expect(replaceButton()).toBeNull();
    expect(recompareButton()).not.toBeNull();

    const calls = compareCompaction.mock.calls.length;
    await settle();
    expect(compareCompaction.mock.calls.length).toBe(calls);
  });

  it('keeps a persistent inline error and enables recompare after a CAS conflict', async () => {
    compareCompaction.mockResolvedValue(response(comparison()));
    replaceSnapshot.mockRejectedValue(
      new ApiError(409, '投影快照已被更新，保留最新快照'),
    );
    await mount();
    setInput('last-seq', '2');
    await submitCompare();
    replaceButton()?.click();
    await settle();

    const alert = view.querySelector('[data-compaction-result][role="alert"]');
    expect(alert?.textContent).toContain('投影快照已被更新，保留最新快照');
    expect(view.textContent).toContain('重新比较');
    expect(view.querySelector('[data-action="recompare"]')).not.toBeNull();

    const calls = compareCompaction.mock.calls.length;
    await settle();
    expect(compareCompaction.mock.calls.length).toBe(calls);

    recompareButton()?.click();
    await settle();
    expect(compareCompaction.mock.calls.length).toBe(calls + 1);
  });

  it('is read-only without Agent management permission', async () => {
    compareCompaction.mockResolvedValue(
      response(comparison({ canReplace: false, readOnly: true })),
    );
    await mount();
    setInput('last-seq', '2');
    await submitCompare();

    expect(replaceButton()).toBeNull();
    expect(view.querySelector('[data-compaction-readonly]')?.textContent).toContain(
      '需要 Agent 管理权限',
    );
  });

  it('keeps a compare failure as an inline alert', async () => {
    compareCompaction.mockRejectedValue(new ApiError(503, 'Harness 存储不可用'));
    await mount();
    setInput('last-seq', '2');
    await submitCompare();

    expect(view.querySelector('[data-compaction-error]')?.textContent).toContain(
      'Harness 存储不可用',
    );
  });

  it('validates the fixed boundary before calling the API', async () => {
    await mount();
    setInput('last-seq', '');
    await submitCompare();

    expect(compareCompaction).not.toHaveBeenCalled();
    expect(view.querySelector('[data-compaction-error]')?.textContent).toContain(
      '请输入固定的 last_seq',
    );
  });

  it('clears an old matched comparison when boundary validation fails', async () => {
    compareCompaction.mockResolvedValue(response(comparison()));
    await mount();
    setInput('last-seq', '2');
    await submitCompare();
    expect(replaceButton()).not.toBeNull();

    setInput('last-seq', '');
    await submitCompare();

    expect(view.querySelector('[data-compaction-error]')?.textContent).toContain(
      '请输入固定的 last_seq',
    );
    expect(replaceButton()).toBeNull();
    expect(view.querySelector('[data-compaction-facts]')).toBeNull();
  });

  it('renders differences from the fresh comparison carried by a 409 conflict', async () => {
    compareCompaction.mockResolvedValue(response(comparison()));
    const fresh = comparison({
      status: 'hash_mismatch',
      matched: false,
      canReplace: false,
      snapshotStateVersion: 4,
      differences: [
        { path: '/events/0/payload/text', rebuilt: 'one', snapshot: 'tampered' },
      ],
    });
    replaceSnapshot.mockRejectedValue(
      new ApiError(409, '投影快照已被更新，保留最新快照', {
        code: 409,
        msg: '投影快照已被更新，保留最新快照',
        result: fresh,
      }),
    );
    await mount();
    setInput('last-seq', '2');
    await submitCompare();
    replaceButton()?.click();
    await settle();

    expect(view.querySelector('[data-differences]')?.textContent).toContain(
      '/events/0/payload/text',
    );
    expect(view.textContent).toContain('Hash 不匹配');
    expect(replaceButton()).toBeNull();
    expect(recompareButton()).not.toBeNull();
  });

  it('ignores a stale replace response after a workspace switch', async () => {
    compareCompaction.mockResolvedValue(response(comparison()));
    let release: (value: {
      code: number;
      msg: string;
      result: HarnessProjectionSnapshot;
    }) => void = () => {};
    replaceSnapshot.mockImplementation(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    await mount();
    setInput('last-seq', '2');
    await submitCompare();
    replaceButton()?.click();
    await settle();

    view.workspaceId = 'ws_2';
    await settle();
    release(response(snapshot({ stateVersion: 4 })));
    await settle();

    expect(view.textContent).not.toContain('已替换投影快照');
    expect(view.querySelector('[data-compaction-facts]')).toBeNull();
    expect(view.querySelector('[data-compaction-status]')).toBeNull();
  });
});
