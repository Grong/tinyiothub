import type { ToolPresentation, ToolView, ToolViewInput } from './types.js';
import { clampText, deviceFacts } from './parse.js';

const HEALTH_LABEL: Record<string, string> = {
  ok: '健康正常',
  attention: '需要关注',
  bad: '健康异常',
  unknown: '健康未知',
};

function statusOf(input: ToolViewInput): ToolPresentation['status'] {
  if (input.status === 'failed') return 'failed';
  if (input.status === 'running' || input.result == null) return 'running';
  return 'ok';
}

function label(name: string): string {
  return name ? `「${name}」` : '该设备';
}

export const deviceReadView: ToolView = {
  key: 'device.read',
  present(input) {
    const facts = deviceFacts(input.result, input.args);
    if (!facts && !input.status) return null;
    const name = facts?.name ?? '';
    const profile = input.name === 'get_thing' || input.name === 'get_thing_profile';
    const title = profile
      ? `查看${label(name)}的档案`
      : `查看${label(name)}的实时状态`;
    if (input.status === 'failed') {
      const detail = clampText(input.error || '设备读取失败', 120);
      return { viewKey: 'device.read', title: `${title}失败`, summary: detail, status: 'failed', details: [{ label: '错误', value: detail }] };
    }
    if (!facts) return null;
    const metrics = facts.metrics;
    const factsLine = [
      facts.online === undefined ? '' : facts.online ? '在线' : '离线',
      HEALTH_LABEL[facts.health] ?? facts.health,
      metrics.length > 0 ? `${metrics.length} 项运行参数` : '',
    ].filter(Boolean);
    return {
      viewKey: 'device.read',
      title,
      summary: factsLine.join(' · ') || (profile ? '设备档案已读取' : '设备状态已读取'),
      status: statusOf(input),
      metrics,
      details: [
        ...(facts.zone ? [{ label: '区域', value: facts.zone }] : []),
        ...(facts.id ? [{ label: '设备 ID', value: facts.id }] : []),
        ...facts.sources.map((source, index) => ({
          label: `画面 ${index + 1}`,
          value: [source.label, source.finding].filter(Boolean).join(' · '),
        })),
        ...facts.actions.slice(0, 5).map((action, index) => ({
          label: `动作 ${index + 1}`,
          value: [action.name, action.description].filter(Boolean).join(' · '),
        })),
        ...(input.evidence?.length
          ? [{ label: '证据', value: `${input.evidence.length} 条` }]
          : []),
      ],
    };
  },
};

export const deviceListView: ToolView = {
  key: 'device.list',
  present(input) {
    const facts = (() => {
      try {
        const value = input.result ? JSON.parse(input.result) : null;
        if (!value || typeof value !== 'object') return null;
        const record = value as Record<string, unknown>;
        const raw = Array.isArray(record.items)
          ? record.items
          : Array.isArray(record.things)
            ? record.things
            : Array.isArray(record.devices)
              ? record.devices
              : null;
        if (!raw) return null;
        const items = raw
          .filter((item): item is Record<string, unknown> => typeof item === 'object' && item !== null)
          .map((item) => ({
            name: typeof item.name === 'string' ? item.name : '',
            online: typeof item.online === 'boolean' ? item.online : undefined,
          }));
        return { total: typeof record.total === 'number' ? record.total : items.length, items };
      } catch {
        return null;
      }
    })();
    if (!facts) return null;
    const active = facts.items.filter((item) => item.online !== false).length;
    return {
      viewKey: 'device.list',
      title: input.name === 'search_things' ? '查找设备' : '梳理需要关注的设备',
      summary: `共 ${facts.total} 台`,
      status: statusOf(input),
      metrics: [
        { label: '设备总数', value: String(facts.total) },
        { label: '在线', value: String(active) },
        { label: '离线', value: String(facts.items.length - active) },
      ],
      details: facts.items.slice(0, 5).map((item, index) => ({
        label: `设备 ${index + 1}`,
        value: [item.name, item.online === false ? '离线' : item.online ? '在线' : ''].filter(Boolean).join(' · '),
      })),
    };
  },
};

export const propertyReadView: ToolView = {
  key: 'property.read',
  present(input) {
    if (input.status === 'failed') {
      return {
        viewKey: 'property.read',
        title: '读取运行参数失败',
        summary: clampText(input.error || '读取失败', 120),
        status: 'failed',
      };
    }
    let record: Record<string, unknown> | null = null;
    try {
      const parsed = input.result ? JSON.parse(input.result) : null;
      record = typeof parsed === 'object' && parsed !== null ? (parsed as Record<string, unknown>) : null;
    } catch {
      record = null;
    }
    if (!record) return null;
    const property = typeof record.propertyName === 'string' ? record.propertyName : '';
    const value = record.currentValue === undefined ? '—' : String(record.currentValue);
    const unit = typeof record.unit === 'string' ? ` ${record.unit}` : '';
    return {
      viewKey: 'property.read',
      title: `读取${property ? `「${property}」` : ''}的运行参数`,
      summary: `${property || '属性'}：${value}${unit}`,
      status: statusOf(input),
      details: [
        ...(property ? [{ label: '属性', value: property }] : []),
        { label: '当前值', value: `${value}${unit}` },
      ],
    };
  },
};

export const deviceWriteView: ToolView = {
  key: 'device.write',
  present(input) {
    const failed = input.status === 'failed';
    const args = (input.args && typeof input.args === 'object' ? input.args : {}) as Record<string, unknown>;
    const target = typeof args.thingId === 'string' ? args.thingId : '';
    const detail = failed ? clampText(input.error || '写入失败', 120) : '设置已提交';
    return {
      viewKey: 'device.write',
      title: target ? `调整「${target}」的设置` : '调整设备设置',
      summary: detail,
      status: failed ? 'failed' : statusOf(input),
      details: target ? [{ label: '设备', value: target }] : [],
    };
  },
};

export const deviceActionView: ToolView = {
  key: 'device.action',
  present(input) {
    const failed = input.status === 'failed';
    const args = (input.args && typeof input.args === 'object' ? input.args : {}) as Record<string, unknown>;
    const target = typeof args.thingId === 'string' ? args.thingId : typeof args.deviceId === 'string' ? args.deviceId : '';
    const action = typeof args.action === 'string' ? args.action : '';
    return {
      viewKey: 'device.action',
      title: target
        ? `让「${target}」执行${action ? `「${action}」` : '动作'}`
        : action ? `执行「${action}」` : '执行设备操作',
      summary: failed ? clampText(input.error || '执行失败', 120) : input.result ? '已提交执行' : '执行中…',
      status: failed ? 'failed' : statusOf(input),
      details: [
        ...(target ? [{ label: '设备', value: target }] : []),
        ...(action ? [{ label: '动作', value: action }] : []),
        ...(args.params && typeof args.params === 'object'
          ? [{ label: '参数', value: JSON.stringify(args.params).slice(0, 160) }]
          : []),
      ],
    };
  },
};
