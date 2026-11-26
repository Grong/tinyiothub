import { describe, expect, it } from 'vitest';
import { skillDisplay } from './skill-copy.js';

describe('skillDisplay', () => {
  it('prefers backend title/description and falls back to known skill copy', () => {
    expect(skillDisplay('job-management')).toMatchObject({
      title: '定时任务管理',
      description: '管理定时任务和自动化规则，让设备操作按计划执行。',
    });
    expect(skillDisplay('job-management', '任务管理', '后端描述')).toEqual({
      title: '任务管理',
      description: '后端描述',
    });
    expect(skillDisplay('unknown-skill', '未知技能')).toEqual({
      title: '未知技能',
      description: '',
    });
  });
});
