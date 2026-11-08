import { afterEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({ answerApproval: vi.fn() }));
vi.mock('../../api/assistant.js', () => ({ answerApproval: hoisted.answerApproval }));

import './approval-card.js';
import type { ApprovalCard } from './approval-card.js';

describe('approval-card', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    hoisted.answerApproval.mockReset();
  });

  it('approves and emits answered', async () => {
    hoisted.answerApproval.mockResolvedValue(undefined);
    const el = document.createElement('approval-card') as ApprovalCard;
    el.sessionKey = 'agent:ws:a/s1';
    el.pending = { approvalId: 'a1', tool: 'set_device', summary: '设 24℃' };
    const answered: string[] = [];
    el.addEventListener('answered', (event) => answered.push((event as CustomEvent<string>).detail));
    document.body.appendChild(el);
    await el.updateComplete;
    (el.shadowRoot!.querySelector('.approval-approve') as HTMLButtonElement).click();
    await el.updateComplete;
    expect(hoisted.answerApproval).toHaveBeenCalledWith('agent:ws:a/s1', 'a1', 'approved');
    expect(answered).toEqual(['a1']);
  });
});
