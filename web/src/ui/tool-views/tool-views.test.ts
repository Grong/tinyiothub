import { describe, expect, it } from 'vitest';
import { presentTool } from './registry.js';

const device = JSON.stringify({
  thingId: 'dev-1',
  name: '冷却塔 A',
  online: false,
  health: 'bad',
  zone: '3 号楼屋面',
  state: { 出水温度: '38.2', 风机转速: '0' },
  sources: [{ source: '通道 1', mediaTime: '2026-11-21T12:00:00Z', text: '塔顶有积水' }],
});

describe('tool views', () => {
  it('renders concrete device parameters from the device.read viewKey', () => {
    for (const name of ['read_device', 'get_thing', 'get_thing_profile']) {
      const view = presentTool({
        name,
        viewKey: 'device.read',
        args: { thingId: 'dev-1' },
        result: device,
        status: 'ok',
        evidence: [{ evidenceId: 'ev-1' }],
      });
      expect(view.viewKey).toBe('device.read');
      expect(view.title).toContain('冷却塔 A');
      expect(view.metrics).toEqual([
        { label: '出水温度', value: '38.2' },
        { label: '风机转速', value: '0' },
      ]);
      expect(view.summary).toContain('2 项运行参数');
      expect(view.details).toEqual(
        expect.arrayContaining([
          { label: '画面 1', value: '通道 1 · 塔顶有积水' },
          { label: '证据', value: '1 条' },
        ]),
      );
    }
  });

  it('renders a device list with totals', () => {
    const view = presentTool({
      name: 'list_things',
      viewKey: 'device.list',
      args: {},
      result: JSON.stringify({ items: [{ id: 'd1', name: '送风机 1' }, { id: 'd2', name: '送风机 2' }], total: 2 }),
      status: 'ok',
    });
    expect(view.viewKey).toBe('device.list');
    expect(view.title).toBe('梳理需要关注的设备');
    expect(view.summary).toBe('共 2 台');
  });

  it('renders property reads through the property.read viewKey', () => {
    const view = presentTool({
      name: 'read_properties',
      viewKey: 'property.read',
      args: {},
      result: JSON.stringify({ propertyName: '温度', currentValue: '25.5', unit: '°C' }),
      status: 'ok',
    });
    expect(view.viewKey).toBe('property.read');
    expect(view.summary).toBe('温度：25.5 °C');
  });

  it('renders alarm and event details', () => {
    const alarms = presentTool({
      name: 'alarm_list',
      viewKey: 'alarm.list',
      args: {},
      result: JSON.stringify({
        total: 2,
        alarms: [
          { level: 'critical', thingName: '冷却塔 A', message: '设备离线', isAcknowledged: false },
          { level: 'warning', thingName: '送风机 1', message: '温度偏高', isAcknowledged: true },
        ],
      }),
      status: 'ok',
    });
    expect(alarms.viewKey).toBe('alarm.list');
    expect(alarms.metrics).toContainEqual({ label: '严重', value: '1' });
    expect(alarms.details?.[0].value).toContain('冷却塔 A');

    const events = presentTool({
      name: 'query_events',
      viewKey: 'event.history',
      args: {},
      result: JSON.stringify({ total: 1, events: [{ type: 'offline', thingName: '冷却塔 A', time: '12:00' }] }),
      status: 'ok',
    });
    expect(events.viewKey).toBe('event.history');
    expect(events.details?.[0].value).toContain('offline');
  });

  it('reads the paginated alarm API shape', () => {
    const view = presentTool({
      name: 'alarm_list',
      viewKey: 'alarm.list',
      args: {},
      result: JSON.stringify({
        data: [
          { id: 'alarm-005', alarmLevel: 'critical', message: 'A栋车间烟雾浓度异常', isAcknowledged: false },
          { id: 'alarm-003', alarmLevel: 'warning', message: '冷链温度异常', isAcknowledged: true },
        ],
        pagination: { total_count: 2 },
      }),
      status: 'ok',
    });
    expect(view.summary).toBe('2 条待关注');
    expect(view.metrics).toContainEqual({ label: '严重', value: '1' });
    expect(view.details?.[0].value).toContain('烟雾浓度');
  });

  it('renders action target and parameters', () => {
    const view = presentTool({
      name: 'call_device',
      viewKey: 'device.action',
      args: { thingId: 'dev-1', action: 'restart', params: { force: true } },
      result: JSON.stringify({ status: 'ok' }),
      status: 'ok',
    });
    expect(view.viewKey).toBe('device.action');
    expect(view.title).toContain('dev-1');
    expect(view.details).toEqual(
      expect.arrayContaining([
        { label: '设备', value: 'dev-1' },
        { label: '动作', value: 'restart' },
      ]),
    );
  });

  it('uses the backend viewKey, even when the tool name is unknown', () => {
    const view = presentTool({
      name: 'external_read',
      viewKey: 'device.read',
      args: { thingId: 'dev-2' },
      result: JSON.stringify({ thingId: 'dev-2', name: '泵 2', online: true, health: 'ok', state: { 压力: '1.2' } }),
      status: 'ok',
    });
    expect(view.viewKey).toBe('device.read');
    expect(view.title).toContain('泵 2');
  });

  it('renders todo and knowledge details', () => {
    const todo = presentTool({
      name: 'todo_write',
      viewKey: 'todo.write',
      args: { todos: [{ content: '检查告警', status: 'completed' }, { content: '读取设备', status: 'in_progress' }] },
      result: '{"ok":true}',
      status: 'ok',
    });
    expect(todo.viewKey).toBe('todo.write');
    expect(todo.metrics).toContainEqual({ label: '已完成', value: '1' });
    expect(todo.details?.[0].value).toContain('检查告警');

    const knowledge = presentTool({
      name: 'search_knowledge',
      viewKey: 'knowledge.search',
      args: {},
      result: JSON.stringify({ total: 1, results: [{ title: '巡检规范', snippet: '检查设备状态' }] }),
      status: 'ok',
    });
    expect(knowledge.viewKey).toBe('knowledge.search');
    expect(knowledge.details?.[0].value).toContain('巡检规范');
  });

  it('falls back to generic without a viewKey and for failures', () => {
    expect(presentTool({ name: 'external_tool', args: {}, result: 'ok' }).viewKey).toBe('generic');
    const failed = presentTool({
      name: 'read_device',
      viewKey: 'device.read',
      args: {},
      result: null,
      status: 'failed',
      error: '设备不存在',
    });
    expect(failed.viewKey).toBe('device.read');
    expect(failed.status).toBe('failed');
  });
});
