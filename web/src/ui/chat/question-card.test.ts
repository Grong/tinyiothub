import { afterEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({ answerQuestion: vi.fn() }));
vi.mock('../../api/assistant.js', () => ({ answerQuestion: hoisted.answerQuestion }));

import './question-card.js';
import { QuestionCard } from './question-card.js';

async function mount(): Promise<QuestionCard> {
  const el = document.createElement('question-card') as QuestionCard;
  el.sessionKey = 'agent:ws-1:default/s1';
  el.pending = {
    questionId: 'q1',
    questions: [
      {
        question: '检查哪些设备？',
        header: '范围',
        options: [
          { label: '全部', description: '全部设备' },
          { label: '仅离线', description: '只检查离线' },
        ],
        multiSelect: false,
      },
    ],
  };
  document.body.appendChild(el);
  await el.updateComplete;
  return el;
}

describe('question-card', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    hoisted.answerQuestion.mockReset();
  });

  it('submits the selected option', async () => {
    hoisted.answerQuestion.mockResolvedValue(undefined);
    const el = await mount();
    (el.shadowRoot!.querySelectorAll('.question-option')[1] as HTMLButtonElement).click();
    await el.updateComplete;
    (el.shadowRoot!.querySelector('.question-submit') as HTMLButtonElement).click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await el.updateComplete;
    expect(hoisted.answerQuestion).toHaveBeenCalledWith('agent:ws-1:default/s1', 'q1', {
      answers: [{ questionIndex: 0, selected: ['仅离线'] }],
    });
    expect(el.shadowRoot!.textContent).toContain('已提交');
  });

  it('disables submit until every question has a selection', async () => {
    const el = await mount();
    const submit = el.shadowRoot!.querySelector('.question-submit') as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
  });

  it('renders the question header', async () => {
    const el = await mount();
    expect(el.shadowRoot!.textContent).toContain('范围');
  });

  it('uses left-aligned full-width option rows', async () => {
    await mount();
    const styles = QuestionCard.styles.toString();
    expect(styles).toContain('flex-direction: column');
    expect(styles).toContain('text-align: left');
  });

  it('dismisses an already-closed question after a 404 response', async () => {
    const { ApiError } = await import('../../api/client.js');
    hoisted.answerQuestion.mockRejectedValue(new ApiError(404, '问题不存在或会话已结束'));
    const el = await mount();
    let answered = 0;
    el.addEventListener('answered', () => { answered += 1; });
    (el.shadowRoot!.querySelectorAll('.question-option')[0] as HTMLButtonElement).click();
    await el.updateComplete;
    (el.shadowRoot!.querySelector('.question-submit') as HTMLButtonElement).click();
    await vi.waitFor(() => expect(answered).toBe(1));
    await el.updateComplete;
    expect(el.shadowRoot!.textContent).toContain('问题已关闭');
  });
});
