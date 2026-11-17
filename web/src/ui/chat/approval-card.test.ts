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
    el.addEventListener('answered', (event) =>
      answered.push((event as CustomEvent<string>).detail),
    );
    document.body.appendChild(el);
    await el.updateComplete;
    (el.shadowRoot!.querySelector('.approval-approve') as HTMLButtonElement).click();
    await el.updateComplete;
    expect(hoisted.answerApproval).toHaveBeenCalledWith('agent:ws:a/s1', 'a1', 'approved');
    expect(answered).toEqual(['a1']);
  });

  it('dismisses an already-closed approval after a 404 or 409 response', async () => {
    const { ApiError } = await import('../../api/client.js');
    hoisted.answerApproval.mockRejectedValue(new ApiError(404, '审批不存在或已处理'));
    const el = document.createElement('approval-card') as ApprovalCard;
    el.sessionKey = 'agent:ws:a/s1';
    el.pending = { approvalId: 'a1', tool: 'call_device', summary: '重连设备' };
    const answered: string[] = [];
    el.addEventListener('answered', (event) =>
      answered.push((event as CustomEvent<string>).detail),
    );
    document.body.appendChild(el);
    await el.updateComplete;
    (el.shadowRoot!.querySelector('.approval-deny') as HTMLButtonElement).click();
    await vi.waitFor(() => expect(answered).toEqual(['a1']));
    await el.updateComplete;
    expect(el.shadowRoot!.textContent).toContain('已处理');
  });
});
