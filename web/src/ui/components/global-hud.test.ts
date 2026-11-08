import { afterEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({ fetchAssistantHud: vi.fn() }));
vi.mock('../../api/assistant.js', () => ({ fetchAssistantHud: hoisted.fetchAssistantHud }));

import './global-hud.js';
import type { GlobalHud } from './global-hud.js';

const snapshot = {
  assistant: { status: 'waiting_for_user', lastOutcome: 'completed' },
  devices: { total: 12, online: 10, offline: 2 },
  alarms: { highPriority: 3 },
  pending: { questions: 1, approvals: 0 },
  activity: { activeSessions: 1 },
  schedule: {
    enabled: true,
    intervalSeconds: 300,
    prompt: '',
    nextWakeAt: '2026-11-08T10:10:00Z',
    lastWakeAt: null,
    lastOutcome: null,
  },
  recentEvents: [],
};

describe('global-hud', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    hoisted.fetchAssistantHud.mockReset();
  });

  it('renders the global status and counters', async () => {
    hoisted.fetchAssistantHud.mockResolvedValue(snapshot);
    const el = document.createElement('global-hud') as GlobalHud;
    el.workspaceId = 'ws-1';
    el.pollMs = 0;
    document.body.appendChild(el);
    await el.refresh();
    await el.updateComplete;

    const text = el.shadowRoot!.textContent!;
    expect(text).toContain('等待你');
    expect(text).toContain('12');
    expect(text).toContain('3');
    expect(text).toContain('1 待你回答');
    el.remove();
  });

  it('expands the detail panel', async () => {
    hoisted.fetchAssistantHud.mockResolvedValue(snapshot);
    const el = document.createElement('global-hud') as GlobalHud;
    el.workspaceId = 'ws-1';
    el.pollMs = 0;
    document.body.appendChild(el);
    await el.refresh();
    await el.updateComplete;
    (el.shadowRoot!.querySelector('.hud-card') as HTMLButtonElement).click();
    await el.updateComplete;
    expect(el.shadowRoot!.textContent).toContain('下次唤醒');
    el.remove();
  });
});
