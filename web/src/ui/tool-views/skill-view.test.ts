import { describe, expect, it } from 'vitest';
import './skill-view.js';
import type { SkillView } from './skill-view.js';

describe('skill-view', () => {
  it('renders backend title and description', async () => {
    const el = document.createElement('skill-view') as SkillView;
    el.name = 'job-management';
    el.title = '定时任务管理';
    el.description = '管理定时任务和自动化规则。';
    document.body.append(el);
    await el.updateComplete;
    expect(el.shadowRoot?.textContent).toContain('定时任务管理');
    expect(el.shadowRoot?.textContent).toContain('管理定时任务和自动化规则');
    el.remove();
  });
});
