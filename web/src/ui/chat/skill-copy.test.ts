import { describe, expect, it } from 'vitest';
import { skillDisplay } from './skill-copy.js';

describe('skillDisplay', () => {
  it('uses backend title/description and falls back only to the id', () => {
    expect(skillDisplay('job-management', '定时任务管理', '管理定时任务和自动化规则。')).toEqual({
      title: '定时任务管理',
      description: '管理定时任务和自动化规则。',
    });
    expect(skillDisplay('job-management')).toEqual({
      title: 'job-management',
      description: '',
    });
  });
});
