import { afterEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({
  listChatSessions: vi.fn(),
  renameChatSession: vi.fn(),
  deleteChatSession: vi.fn(),
}));
vi.mock('../../api/assistant.js', () => hoisted);

import './session-drawer.js';
import type { SessionDrawer } from './session-drawer.js';

describe('session-drawer', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    Object.values(hoisted).forEach((mock) => mock.mockReset());
  });

  it('groups sessions by kind and emits select', async () => {
    hoisted.listChatSessions.mockResolvedValue([
      {
        sessionKey: 'agent:ws:a/c1',
        kind: 'chat',
        status: 'active',
        label: '新会话',
        updatedAt: 2,
      },
      {
        sessionKey: 'agent:ws:a/s1',
        kind: 'scheduled',
        status: 'ended',
        label: '巡检',
        updatedAt: 1,
      },
    ]);
    const el = document.createElement('session-drawer') as SessionDrawer;
    el.agentId = 'default';
    el.current = 'agent:ws:a/c1';
    const selected: string[] = [];
    el.addEventListener('session-select', (event) =>
      selected.push((event as CustomEvent<string>).detail),
    );
    document.body.appendChild(el);
    await el.refresh();
    await el.updateComplete;

    expect(el.shadowRoot!.textContent).toContain('对话');
    expect(el.shadowRoot!.textContent).toContain('巡检');
    const items = el.shadowRoot!.querySelectorAll('.drawer-item');
    expect(items).toHaveLength(2);
    const actionLabels = Array.from(el.shadowRoot!.querySelectorAll('.drawer-action')).map(
      (button) => button.getAttribute('aria-label'),
    );
    expect(actionLabels).toContain('改名 新会话');
    (items[1] as HTMLButtonElement).click();
    expect(selected).toEqual(['agent:ws:a/s1']);
  });
});
