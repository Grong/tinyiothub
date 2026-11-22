import { describe, expect, it } from 'vitest';
import './task-plan-panel.js';
import { TaskPlanPanel } from './task-plan-panel.js';

describe('task-plan-panel', () => {
  it('can start collapsed without hiding the task summary', async () => {
    const panel = document.createElement('task-plan-panel') as TaskPlanPanel;
    panel.startCollapsed = true;
    panel.todos = [{ content: '扫描告警', activeForm: '正在扫描告警', status: 'in_progress' }];
    document.body.append(panel);
    await panel.updateComplete;
    expect(panel.textContent).toContain('1 进行中');
    expect(panel.textContent).not.toContain('正在扫描告警');
    panel.querySelector('button')?.click();
    await panel.updateComplete;
    expect(panel.textContent).toContain('正在扫描告警');
    panel.remove();
  });

  it('shows the active task, remaining count, and completed progress', async () => {
    const panel = document.createElement('task-plan-panel') as TaskPlanPanel;
    panel.todos = [
      { content: '扫描告警', activeForm: '正在扫描告警', status: 'completed' },
      { content: '读取设备', activeForm: '正在读取设备状态', status: 'in_progress' },
      { content: '生成结论', activeForm: '正在生成结论', status: 'pending' },
    ];
    document.body.append(panel);
    await panel.updateComplete;
    expect(panel.textContent).toContain('已完成 1 · 1 进行中 · 1 待处理');
    expect(panel.textContent).toContain('正在读取设备状态');
    expect(panel.textContent).not.toContain('1/3');
    panel.remove();
  });
});
