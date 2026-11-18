import { describe, expect, it } from 'vitest';
import {
  assistantUrl,
  legacyWorkbenchRedirect,
  parseAssistantHash,
} from './assistant-url-state.js';

describe('assistant hash URL state', () => {
  it('parses the canonical assistant route', () => {
    expect(parseAssistantHash('#/assistant')).toEqual({ view: 'assistant' });
  });

  it('parses an encoded session key', () => {
    expect(parseAssistantHash('#/assistant/sessions/agent%3Aws%3Adefault%2Fs1')).toEqual({
      view: 'assistant',
      sessionKey: 'agent:ws:default/s1',
    });
  });

  it('parses timeline focus params', () => {
    expect(
      parseAssistantHash(
        '#/assistant/sessions/agent%3Aws%3Adefault%2Fs1?view=flow&device=dev%2F1&at=2026-10-27T10%3A00%3A00Z&event=event%3A2',
      ),
    ).toEqual({
      view: 'assistant',
      sessionKey: 'agent:ws:default/s1',
      focus: {
        view: 'flow',
        device: 'dev/1',
        at: '2026-10-27T10:00:00Z',
        event: 'event:2',
      },
    });
  });

  it('omits absent focus params', () => {
    expect(parseAssistantHash('#/assistant?view=flow')).toEqual({
      view: 'assistant',
      focus: { view: 'flow' },
    });
  });

  it('rejects non-assistant, malformed and over-long hashes', () => {
    expect(parseAssistantHash('')).toBeNull();
    expect(parseAssistantHash('/assistant')).toBeNull();
    expect(parseAssistantHash('#/assistant/foo')).toBeNull();
    expect(parseAssistantHash('#/assistant/sessions')).toBeNull();
    expect(parseAssistantHash('#/assistant/sessions/a/b')).toBeNull();
    expect(parseAssistantHash('#/assistant/sessions/%E0%A4%A')).toBeNull();
    expect(parseAssistantHash('#/harness/tasks/t1')).toBeNull();
    expect(parseAssistantHash('#/runtime')).toBeNull();
  });

  it('formats and round-trips the base URL, session key and focus state', () => {
    expect(assistantUrl({ view: 'assistant' })).toBe('#/assistant');
    expect(assistantUrl({ view: 'assistant', sessionKey: 'agent:ws:default/s1' })).toBe(
      '#/assistant/sessions/agent%3Aws%3Adefault%2Fs1',
    );

    const state = {
      view: 'assistant' as const,
      sessionKey: 'agent:ws:default/s1',
      focus: {
        view: 'flow',
        device: 'dev/1',
        at: '2026-10-27T10:00:00Z',
        event: 'event:2',
      },
    };
    const url = assistantUrl(state);
    expect(url).toBe(
      '#/assistant/sessions/agent%3Aws%3Adefault%2Fs1?view=flow&device=dev%2F1&at=2026-10-27T10%3A00%3A00Z&event=event%3A2',
    );
    expect(parseAssistantHash(url)).toEqual(state);
  });
});

describe('legacy workbench redirect', () => {
  it('redirects every legacy Runtime and Harness entry to #/assistant', () => {
    for (const hash of [
      '#/runtime',
      '#/runtime/new',
      '#/runtime/tasks/task%3A1?view=flow&device=dev%2F1&event=event%3A2',
      '#/harness',
      '#/harness#main-main%3A1',
      '#/harness/tasks',
      '#/harness/tasks/new',
      '#/harness/tasks/task%3A1?from=%23%2Fharness%23main-main%3A1',
      '#/harness/workflows/wf%3A1',
      '#/harness/supervisor',
      '#/harness/compaction',
    ]) {
      // No Task id becomes a session key: the target is always the plain route.
      expect(legacyWorkbenchRedirect(hash), hash).toBe('#/assistant');
    }
  });

  it('ignores canonical assistant URLs and unknown hashes', () => {
    expect(legacyWorkbenchRedirect('#/assistant')).toBeNull();
    expect(legacyWorkbenchRedirect('#/assistant/sessions/agent%3Aws%3Adefault%2Fs1')).toBeNull();
    expect(legacyWorkbenchRedirect('#/harness/unknown')).toBeNull();
    expect(legacyWorkbenchRedirect('#/runtime/other')).toBeNull();
    expect(legacyWorkbenchRedirect('')).toBeNull();
    expect(legacyWorkbenchRedirect('/runtime')).toBeNull();
  });
});
