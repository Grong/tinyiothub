import { afterEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({ answerQuestion: vi.fn() }));
vi.mock('../../api/assistant.js', () => ({ answerQuestion: hoisted.answerQuestion }));

import './question-card.js';
import type { QuestionCard } from './question-card.js';

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
});
