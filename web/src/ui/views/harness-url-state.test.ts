import { describe, expect, it } from 'vitest';
import {
  parseHarnessHash,
  runtimeToHarnessUrl,
  toHarnessDryRunUrl,
  toHarnessTaskUrl,
} from './harness-url-state.js';

describe('Harness canonical URL state', () => {
  it('parses exactly the canonical Harness routes', () => {
    expect(parseHarnessHash('#/harness')).toEqual({ name: 'overview' });
    expect(parseHarnessHash('#/harness#main-main%3A1')).toEqual({
      name: 'overview',
      anchor: 'main-main:1',
    });
    expect(parseHarnessHash('#/harness#harness-attention-5')).toEqual({
      name: 'overview',
      anchor: 'harness-attention-5',
    });
    expect(parseHarnessHash('#/harness/tasks')).toEqual({ name: 'tasks' });
    expect(parseHarnessHash('#/harness/tasks/new')).toEqual({ name: 'new' });
    expect(parseHarnessHash('#/harness/workflows')).toEqual({
      name: 'workflows',
      workflowId: null,
      creating: false,
    });
    expect(parseHarnessHash('#/harness/workflows/wf%3A1')).toEqual({
      name: 'workflows',
      workflowId: 'wf:1',
      creating: false,
    });
    expect(parseHarnessHash('#/harness/workflows/new')).toEqual({
      name: 'workflows',
      workflowId: null,
      creating: true,
    });
    expect(parseHarnessHash('#/harness/workflows/templates')).toEqual({ name: 'templates' });
    expect(parseHarnessHash('#/harness/tasks/task%3A1')).toMatchObject({
      name: 'task',
      taskId: 'task:1',
      view: 'loop',
      device: null,
      history: null,
      from: null,
    });
  });

  it('pins the dry-run route to a revision, snapshot and optional simulation task', () => {
    expect(parseHarnessHash('#/harness/workflows/wf%3A1/dry-run?version=3')).toEqual({
      name: 'dry-run',
      workflowId: 'wf:1',
      version: 3,
      snapshotSeq: null,
      taskId: null,
    });
    expect(
      parseHarnessHash('#/harness/workflows/wf%3A1/dry-run?version=3&snapshot=12&task=task%3A9'),
    ).toEqual({
      name: 'dry-run',
      workflowId: 'wf:1',
      version: 3,
      snapshotSeq: 12,
      taskId: 'task:9',
    });
    expect(toHarnessDryRunUrl('wf:1', 3)).toBe('#/harness/workflows/wf%3A1/dry-run?version=3');
    expect(toHarnessDryRunUrl('wf:1', 3, { snapshotSeq: 12, taskId: 'task:9' })).toBe(
      '#/harness/workflows/wf%3A1/dry-run?version=3&snapshot=12&task=task%3A9',
    );
  });

  it('returns null for invalid Harness routes', () => {
    expect(parseHarnessHash('#/harness/unknown')).toBeNull();
    expect(parseHarnessHash('#/harness/')).toBeNull();
    expect(parseHarnessHash('/harness')).toBeNull();
    expect(parseHarnessHash('#/runtime')).toBeNull();
    expect(parseHarnessHash('#/harness/tasks/')).toBeNull();
    expect(parseHarnessHash('#/harness/workflows/')).toBeNull();
    expect(parseHarnessHash('#/harness/workflows/wf%3A1/dry-run')).toBeNull();
    expect(parseHarnessHash('#/harness/workflows/wf%3A1/dry-run?version=0')).toBeNull();
    expect(parseHarnessHash('#/harness/workflows/wf%3A1/dry-run?version=nope')).toBeNull();
  });

  it('round-trips encoded Task IDs, view, device and history state', () => {
    const url = toHarnessTaskUrl('task:1', {
      view: 'flow',
      device: 'dev/1',
      at: '2026-10-27T10:00:00Z',
      event: 'event:2',
    });
    expect(url).toBe(
      '#/harness/tasks/task%3A1?view=flow&device=dev%2F1&at=2026-10-27T10%3A00%3A00Z&event=event%3A2',
    );
    expect(parseHarnessHash(url)).toMatchObject({
      taskId: 'task:1',
      view: 'flow',
      device: 'dev/1',
      history: { at: '2026-10-27T10:00:00Z', event: 'event:2' },
    });
    expect(toHarnessTaskUrl('t1')).toBe('#/harness/tasks/t1');
  });

  it('preserves a safe parent return path and rejects unsafe ones', () => {
    const url = toHarnessTaskUrl('child:1', { from: '#/harness#main-main:1' });
    expect(url).toBe('#/harness/tasks/child%3A1?from=%23%2Fharness%23main-main%3A1');
    expect(parseHarnessHash(url)).toMatchObject({ from: '#/harness#main-main:1' });
    expect(parseHarnessHash('#/harness/tasks/t1?from=javascript%3Aalert(1)')).toMatchObject({
      from: null,
    });
  });
});

describe('legacy Runtime to canonical Harness mapping', () => {
  it('maps list and new to canonical Task routes', () => {
    expect(runtimeToHarnessUrl('#/runtime')).toBe('#/harness/tasks');
    expect(runtimeToHarnessUrl('#/runtime/new')).toBe('#/harness/tasks/new');
  });

  it('maps a Task without losing the encoded ID or replay/device state', () => {
    expect(
      runtimeToHarnessUrl(
        '#/runtime/tasks/task%3A1?view=flow&device=dev%2F1&at=2026-10-27T10%3A00%3A00Z',
      ),
    ).toBe('#/harness/tasks/task%3A1?view=flow&device=dev%2F1&at=2026-10-27T10%3A00%3A00Z');
    expect(runtimeToHarnessUrl('#/runtime/tasks/task%3A1?device=dev%2F1&event=event%3A2')).toBe(
      '#/harness/tasks/task%3A1?device=dev%2F1&event=event%3A2',
    );
    expect(runtimeToHarnessUrl('#/runtime/tasks/task%2F1')).toBe('#/harness/tasks/task%2F1');
  });

  it('returns null for routes that are not valid legacy Runtime entries', () => {
    expect(runtimeToHarnessUrl('#/runtime/other')).toBeNull();
    expect(runtimeToHarnessUrl('#/harness')).toBeNull();
    expect(runtimeToHarnessUrl('#/runtime/tasks/')).toBeNull();
    expect(runtimeToHarnessUrl('/runtime')).toBeNull();
  });
});
