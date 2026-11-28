import type { ToolPresentation, ToolView, ToolViewInput } from './types.js';
import { clampText, parseJson, recordOf } from './parse.js';

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

interface ThingFacts {
  id: string;
  name: string;
  displayName: string;
}

function thingFacts(value: unknown): ThingFacts | null {
  const record = recordOf(parseJson(value)) ?? recordOf(value);
  const thing = recordOf(record?.thing) ?? record;
  if (!thing) return null;
  const id = typeof thing.id === 'string' ? thing.id : '';
  const name = typeof thing.name === 'string' ? thing.name : '';
  const displayName = typeof thing.displayName === 'string' ? thing.displayName : '';
  return id || name || displayName ? { id, name, displayName } : null;
}

function thingLabel(thing: ThingFacts | null): string {
  return thing ? thing.displayName || thing.name || thing.id : '';
}

function propertyLabel(property: Record<string, unknown>): string {
  const displayName = typeof property.displayName === 'string' ? property.displayName : '';
  const name = typeof property.name === 'string' ? property.name : '';
  return displayName || name || '属性';
}

function propertyValue(property: Record<string, unknown>): string {
  const value = property.currentValue;
  if (value === null || value === undefined || value === '') return '—';
  const unit = typeof property.unit === 'string' && property.unit ? ` ${property.unit}` : '';
  return `${String(value)}${unit}`;
}

export const deviceReadView: ToolView = {
  key: 'device.read',
  card: 'device',
  tone: 'read',
  verb: '查看设备状态',
  present(input) {
    const failed = input.status === 'failed';
    const record = recordOf(parseJson(input.result)) ?? recordOf(input.result);
    const thing = thingFacts(record) ?? thingFacts(input.args);
    const properties = Array.isArray(record?.properties)
      ? record.properties.map(recordOf).filter((item): item is Record<string, unknown> => item !== null)
      : [];
    const commands = Array.isArray(record?.commands)
      ? record.commands.map(recordOf).filter((item): item is Record<string, unknown> => item !== null)
      : [];
    const profile = input.name === 'get_thing' || input.name === 'get_thing_profile' || input.name === 'get_device_profile';
    const name = thingLabel(thing);
    const title = `${profile ? '查看' : '读取'}${label(name)}${profile ? '的档案' : '的实时状态'}`;
    if (failed) {
      const detail = clampText(input.error || '设备读取失败', 120);
      return { viewKey: 'device.read', title: `${title}失败`, summary: detail, status: 'failed', details: [{ label: '错误', value: detail }] };
    }
    const online = typeof record?.online === 'boolean'
      ? record.online
      : typeof record?.thing === 'object' && record.thing !== null && typeof (record.thing as Record<string, unknown>).online === 'boolean'
        ? ((record.thing as Record<string, unknown>).online as boolean)
        : undefined;
    const health = typeof record?.health === 'string'
      ? record.health
      : typeof record?.thing === 'object' && record.thing !== null && typeof (record.thing as Record<string, unknown>).health === 'string'
        ? ((record.thing as Record<string, unknown>).health as string)
        : '';
    const metrics = properties.slice(0, 6).map((property) => ({
      label: propertyLabel(property),
      value: propertyValue(property),
    }));
    const summary = [
      online === undefined ? '' : online ? '在线' : '离线',
      health && health !== 'unknown' ? HEALTH_LABEL[health] ?? health : '',
      properties.length > 0 ? `${properties.length} 项属性` : '',
    ].filter(Boolean).join(' · ');
    const zone = typeof record?.zone === 'string' ? record.zone : '';
    const sources = Array.isArray(record?.sources)
      ? record.sources.map(recordOf).filter((item): item is Record<string, unknown> => item !== null)
      : [];
    const details = [
      ...(zone ? [{ label: '区域', value: zone }] : []),
      ...(thing?.id ? [{ label: '设备 ID', value: thing.id }] : []),
      ...sources.slice(0, 5).map((source, index) => ({
        label: `画面 ${index + 1}`,
        value: [source.source, source.label, source.text, source.finding].filter((value): value is string => typeof value === 'string' && Boolean(value)).join(' · '),
      })),
      ...commands.slice(0, 5).map((command, index) => ({
        label: `动作 ${index + 1}`,
        value: [
          typeof command.displayName === 'string' ? command.displayName : '',
          typeof command.name === 'string' ? command.name : '',
          typeof command.description === 'string' ? command.description : '',
        ].filter(Boolean).join(' · '),
      })),
      ...(input.evidence?.length ? [{ label: '证据', value: `${input.evidence.length} 条` }] : []),
    ];
    if (!thing && properties.length === 0 && commands.length === 0) return null;
    return {
      viewKey: 'device.read',
      title,
      summary: summary || '设备状态已读取',
      status: statusOf(input),
      metrics,
      details,
    };
  },
};

export const deviceProfileView: ToolView = {
  key: 'device.profile',
  card: 'doc',
  tone: 'read',
  verb: '查看设备档案',
  present(input) {
    const base = deviceReadView.present(input);
    if (!base) return null;
    const facts = thingFacts(input.result);
    const record = recordOf(parseJson(input.result)) ?? recordOf(input.result);
    const docs = Array.isArray(record?.knowledgeDocs) ? record.knowledgeDocs : [];
    const properties = Array.isArray(record?.properties) ? record.properties : [];
    const commands = Array.isArray(record?.commands)
      ? record.commands
      : Array.isArray(record?.actions)
        ? record.actions
        : [];
    return {
      ...base,
      viewKey: 'device.profile',
      title: `查看${label(thingLabel(facts))}的完整档案`,
      summary: `${properties.length} 项属性 · ${commands.length} 个动作 · ${docs.length} 份知识文档`,
      metrics: [
        ...(base.metrics ?? []).slice(0, 6),
        { label: '知识文档', value: String(docs.length) },
      ],
      details: [
        ...(base.details ?? []),
        ...docs.slice(0, 3).map((doc, index) => {
          const row = recordOf(doc);
          return {
            label: `文档 ${index + 1}`,
            value: [row?.name, row?.resourceType ?? row?.resource_type, row?.contentTruncated ? '正文已截断' : '']
              .filter(Boolean)
              .join(' · '),
          };
        }),
      ],
    };
  },
};

export const deviceListView: ToolView = {
  key: 'device.list',
  card: 'list',
  tone: 'read',
  verb: '梳理设备清单',
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
            name: typeof item.displayName === 'string' && item.displayName ? item.displayName : typeof item.name === 'string' ? item.name : '',
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
  card: 'device',
  tone: 'read',
  verb: '读取运行参数',
  present(input) {
    if (input.status === 'failed') {
      return {
        viewKey: 'property.read',
        title: '读取属性失败',
        summary: clampText(input.error || '读取失败', 120),
        status: 'failed',
      };
    }
    const record = recordOf(parseJson(input.result)) ?? recordOf(input.result);
    const thing = thingFacts(record);
    const properties = Array.isArray(record?.properties)
      ? record.properties.map(recordOf).filter((item): item is Record<string, unknown> => item !== null)
      : record?.property
        ? [recordOf(record.property)].filter((item): item is Record<string, unknown> => item !== null)
        : [];
    if (properties.length === 0) return null;
    const first = properties[0];
    const title = properties.length === 1
      ? `读取${label(thingLabel(thing))} · ${propertyLabel(first)}`
      : `读取${label(thingLabel(thing))} · ${properties.length} 项属性`;
    return {
      viewKey: 'property.read',
      title,
      summary: properties.length === 1
        ? `${propertyLabel(first)}：${propertyValue(first)}`
        : properties.map((property) => `${propertyLabel(property)} ${propertyValue(property)}`).join(' · '),
      status: statusOf(input),
      metrics: properties.map((property) => ({ label: propertyLabel(property), value: propertyValue(property) })),
      details: [
        ...(thing ? [{ label: '设备', value: thingLabel(thing) }] : []),
        ...properties.map((property, index) => {
          const range = [property.minValue, property.maxValue].some((value) => value !== undefined && value !== null)
            ? `${property.minValue ?? '—'} ~ ${property.maxValue ?? '—'}${typeof property.unit === 'string' && property.unit ? ` ${property.unit}` : ''}`
            : '';
          return {
            label: properties.length === 1 ? '属性' : `属性 ${index + 1}`,
            value: [propertyLabel(property), propertyValue(property), range].filter(Boolean).join(' · '),
          };
        }),
      ],
    };
  },
};

export const deviceWriteView: ToolView = {
  key: 'device.write',
  card: 'action',
  tone: 'write',
  verb: '调整设备设置',
  present(input) {
    const failed = input.status === 'failed';
    const args = recordOf(input.args) ?? {};
    const record = recordOf(parseJson(input.result)) ?? recordOf(input.result);
    const thing = thingFacts(record);
    const target = thingLabel(thing) || (typeof args.thingId === 'string' ? args.thingId : '');
    const execution = recordOf(record?.execution);
    const success = execution?.success === true;
    const error = failed
      ? clampText(input.error || '写入失败', 120)
      : typeof execution?.error === 'string' && execution.error
        ? execution.error
        : '';
    const targets = Object.entries(recordOf(args.properties) ?? {})
      .map(([name, value]) => `${name} → ${String(value)}`)
      .join(' · ');
    return {
      viewKey: 'device.write',
      title: target ? `调整「${target}」的设置` : '调整设备设置',
      summary: error || (success ? '执行成功' : input.result ? '已提交' : '执行中…'),
      status: failed || error ? 'failed' : statusOf(input),
      details: [
        ...(target ? [{ label: '设备', value: target }] : []),
        ...(targets ? [{ label: '写入参数', value: targets }] : []),
        ...(execution ? [{ label: '执行结果', value: success ? '成功' : '未成功' }] : []),
      ],
    };
  },
};

export const deviceActionView: ToolView = {
  key: 'device.action',
  card: 'action',
  tone: 'write',
  verb: '执行设备操作',
  present(input) {
    const failed = input.status === 'failed';
    const args = (input.args && typeof input.args === 'object' ? input.args : {}) as Record<string, unknown>;
    const thing = thingFacts(input.result) ?? thingFacts(input.args);
    const target = thingLabel(thing) || (typeof args.thingId === 'string' ? args.thingId : typeof args.deviceId === 'string' ? args.deviceId : '');
    const action = typeof args.action === 'string' ? args.action : typeof args.actionName === 'string' ? args.actionName : '';
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


export const deviceKnowledgeView: ToolView = {
  key: 'device.knowledge',
  card: 'doc',
  tone: 'skill',
  verb: '查阅设备知识',
  present(input) {
    if (input.status === 'failed') {
      return {
        viewKey: 'device.knowledge',
        title: '读取设备知识文档失败',
        summary: clampText(input.error || '读取失败', 120),
        status: 'failed',
      };
    }
    const record = recordOf(parseJson(input.result));
    const docs = Array.isArray(record?.knowledgeDocs) ? record.knowledgeDocs : [];
    const name = typeof record?.name === 'string' ? record.name : '';
    return {
      viewKey: 'device.knowledge',
      title: name ? `查看「${name}」的知识文档` : '读取设备知识文档',
      summary: docs.length > 0 ? `${docs.length} 份文档` : '没有关联知识文档',
      status: statusOf(input),
      details: docs.slice(0, 5).map((item, index) => {
        const doc = recordOf(item);
        return {
          label: `文档 ${index + 1}`,
          value: [doc?.name, doc?.description].filter((value): value is string => typeof value === 'string' && Boolean(value)).join(' · '),
        };
      }),
    };
  },
};
