import { render } from 'lit';
import { describe, expect, it } from 'vitest';
import { renderTimeline } from './timeline-render.js';
import type { ContentPart } from './timeline-model.js';

const at = '2026-11-11T17:00:00Z';

describe('process detail surfaces', () => {
  it('uses one content container for reasoning, tools, subagents, and tasks', () => {
    const parts: ContentPart[] = [
      { kind: 'turn', id: 't1', status: 'running', at },
      { kind: 'reasoning', id: 'r1', text: '分析当前状态', turnId: 't1', streaming: true, at },
      { kind: 'tool', id: 'c1', name: 'read_device', args: {}, result: 'online', at },
      { kind: 'subagent', id: 's1', description: '检查告警', summary: '没有高优先级告警', at },
    ];
    const host = document.createElement('div');
    render(
      renderTimeline(parts, {
        todos: [{ content: '检查设备', status: 'in_progress', activeForm: '检查设备' }],
      }),
      host,
    );
    expect(host.querySelectorAll('.turn-process .process-detail')).toHaveLength(4);
    expect(host.querySelector('.timeline-todos')?.classList.contains('process-detail')).toBe(true);
  });

  it('renders a known device result as status and readable properties', () => {
    const host = document.createElement('div');
    render(
      renderTimeline([
        { kind: 'turn', id: 't1', status: 'completed', at },
        {
          kind: 'tool',
          id: 'c1',
          name: 'read_device',
          viewKey: 'device.read',
          args: {},
          result: JSON.stringify({
            thing: { id: 'dev-1', name: 'temp_sensor', displayName: '3号楼温度传感器', thingType: 'device', online: true, health: 'ok' },
            properties: [
              { name: 'temperature', displayName: '温度', currentValue: '24.5', unit: '°C' },
              { name: 'humidity', displayName: '湿度', currentValue: '60', unit: '%' },
            ],
            zone: '3号楼一层',
          }),
          at,
        },
      ]),
      host,
    );

    expect(host.querySelector('.structured-device__name')?.textContent).toBe('3号楼温度传感器');
    expect(host.querySelector('.structured-device__status')?.textContent).toBe('在线');
    expect(host.querySelector('.structured-device__context')?.textContent).toContain('设备');
    expect(host.querySelector('.structured-device__context')?.textContent).toContain('健康正常');
    const properties = [...host.querySelectorAll('.structured-property')].map((row) => row.textContent);
    expect(properties.some((text) => text?.includes('温度') && text.includes('24.5 °C'))).toBe(true);
    expect(properties.some((text) => text?.includes('湿度') && text.includes('60 %'))).toBe(true);
    expect(host.querySelector('.activity__detail')?.textContent).not.toContain('"online"');
  });

  it('renders known device and task collections without raw JSON', () => {
    const host = document.createElement('div');
    render(
      renderTimeline([
        { kind: 'turn', id: 't1', status: 'completed', at },
        {
          kind: 'tool',
          id: 'devices',
          name: 'list_devices',
          args: {},
          result: JSON.stringify({
            items: [
              { id: 'dev-1', name: '冷却塔', thingType: 'device' },
              { id: 'dev-2', displayName: '冷冻泵', name: 'pump-2', thingType: 'device' },
            ],
            total: 8,
          }),
          at,
        },
        {
          kind: 'tool',
          id: 'todos',
          name: 'todo_write',
          args: {},
          result: JSON.stringify({
            todos: [
              { content: '检查设备', status: 'completed' },
              { content: '生成摘要', status: 'in_progress' },
            ],
          }),
          at,
        },
      ]),
      host,
    );

    expect(host.querySelectorAll('.structured-device-row')).toHaveLength(2);
    expect(host.querySelector('.structured-device-list__count')?.textContent).toBe('共 8 台');
    expect(host.querySelector('.structured-device-list')?.textContent).toContain('冷冻泵');
    expect(host.querySelector('.structured-device-row__type')?.textContent).toBe('设备');
    expect(host.querySelectorAll('.structured-todos .timeline-todo')).toHaveLength(2);
    expect(host.querySelector('.structured-todos .timeline-todo--completed')?.textContent).toContain('检查设备');
    expect(host.querySelector('.structured-todos .timeline-todo--in_progress')?.textContent).toContain('生成摘要');
  });

  it('keeps unrecognized JSON available as raw text', () => {
    const host = document.createElement('div');
    const result = '{"custom":{"value":1}}';
    render(
      renderTimeline([
        { kind: 'turn', id: 't1', status: 'completed', at },
        { kind: 'tool', id: 'c1', name: 'custom_tool', args: {}, result, at },
      ]),
      host,
    );
    expect(host.querySelector('.structured-device')).toBeNull();
    expect(host.querySelector('.structured-todos')).toBeNull();
    expect(host.querySelector('.activity__detail pre:last-of-type')?.textContent).toBe(result);
  });
});
