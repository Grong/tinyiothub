import type { ToolPresentation, ToolView, ToolViewInput } from './types.js';
import { asString, clampText, parseJson, recordOf } from './parse.js';

function statusOf(input: ToolViewInput): ToolPresentation['status'] {
  if (input.status === 'failed') return 'failed';
  if (input.status === 'running' || input.result == null) return 'running';
  return 'ok';
}

function arrayFrom(record: Record<string, unknown>, keys: string[]): unknown[] {
  for (const key of keys) {
    const value = record[key];
    if (Array.isArray(value)) return value;
  }
  return [];
}

export const alarmActionView: ToolView = {
  key: 'alarm.action',
  card: 'action',
  tone: 'write',
  verb: '确认告警',
  present(input) {
    const record = recordOf(parseJson(input.result));
    const failed = input.status === 'failed' || record?.success === false;
    const status = asString(record?.status || record?.outcome);
    const detail = asString(record?.message || record?.error);
    return {
      viewKey: 'alarm.action',
      title: input.name.includes('ack') ? '确认告警' : '处理告警',
      summary: failed
        ? clampText(input.error || detail || '操作失败', 120)
        : status
          ? `操作结果：${status}`
          : input.result
            ? '操作已提交'
            : '执行中…',
      status: failed ? 'failed' : statusOf(input),
      details: [
        ...(status ? [{ label: '结果', value: status }] : []),
        ...(detail ? [{ label: '说明', value: detail }] : []),
      ],
    };
  },
};

export const deviceStopView: ToolView = {
  key: 'device.stop',
  card: 'action',
  tone: 'write',
  verb: '停止设备',
  present(input) {
    return {
      viewKey: 'device.stop',
      title: '停止设备操作',
      summary: input.result ? '已停止' : '停止中…',
      status: statusOf(input),
    };
  },
};

export const thingTreeView: ToolView = {
  key: 'thing.tree',
  card: 'list',
  tone: 'read',
  verb: '浏览设备层级',
  present(input) {
    const record = recordOf(parseJson(input.result));
    const nodes = record ? arrayFrom(record, ['nodes', 'items', 'things']) : [];
    return {
      viewKey: 'thing.tree',
      title: '查看空间与设备层级',
      summary: nodes.length > 0 ? `共 ${nodes.length} 个节点` : input.result ? '层级已读取' : '读取中…',
      status: statusOf(input),
    };
  },
};

const ALARM_LEVEL_LABEL: Record<string, string> = {
  critical: '严重',
  error: '错误',
  high: '严重',
  warning: '警告',
  info: '提示',
};

function alarmLevelLabel(value: unknown): string {
  const level = asString(value).toLowerCase();
  return ALARM_LEVEL_LABEL[level] ?? asString(value);
}

function alarmThingLabel(alarm: Record<string, unknown>): string {
  const thing = recordOf(alarm.thing);
  if (thing) {
    return asString(thing.displayName) || asString(thing.name) || asString(thing.id);
  }
  return asString(alarm.thingDisplayName) || asString(alarm.thingName) || asString(alarm.deviceName) || asString(alarm.thingId);
}

const ALARM_STATUS_LABEL: Record<string, string> = {
  active: '活跃',
  acknowledged: '已确认',
  resolved: '已恢复',
  closed: '已关闭',
};

function shortTime(value: string): string {
  return value ? value.slice(5, 16).replace('T', ' ') : '';
}

export const alarmListView: ToolView = {
  key: 'alarm.list',
  card: 'finding',
  tone: 'alarm',
  verb: '检查告警',
  present(input) {
    const record = recordOf(parseJson(input.result));
    if (!record) {
      return input.status === 'failed'
        ? {
            viewKey: 'alarm.list',
            title: '检查当前告警失败',
            summary: clampText(input.error || '读取失败', 120),
            status: 'failed',
          }
        : null;
    }
    const alarms = arrayFrom(record, ['alarms', 'items', 'data'])
      .map(recordOf)
      .filter((item): item is Record<string, unknown> => item !== null);
    const active = alarms.filter((alarm) => {
      const status = asString(alarm.status).toLowerCase();
      return alarm.isResolved !== true && status !== 'resolved' && status !== 'closed';
    });
    const critical = active.filter((alarm) =>
      ['critical', 'error', 'high'].includes(asString(alarm.alarmLevel || alarm.level || alarm.severity).toLowerCase()),
    ).length;
    const unacknowledged = active.filter((alarm) => alarm.isAcknowledged !== true).length;
    const args = recordOf(input.args);
    const params: ToolPresentation['params'] = [];
    if (args) {
      const levels = Array.isArray(args.levels) ? args.levels.map(asString).filter(Boolean) : [];
      if (levels.length > 0) params.push({ label: '级别', value: levels.join('/') });
      const statuses = Array.isArray(args.statuses) ? args.statuses.map(asString).filter(Boolean) : [];
      if (statuses.length > 0) {
        params.push({ label: '状态', value: statuses.map((item) => ALARM_STATUS_LABEL[item] ?? item).join('/') });
      }
      const start = asString(args.startTime || args.start_time);
      const end = asString(args.endTime || args.end_time);
      if (start || end) {
        params.push({ label: '时间', value: `${shortTime(start) || '—'}~${shortTime(end) || '现在'}` });
      }
      const pageSize = Number(args.pageSize ?? args.page_size);
      if (Number.isFinite(pageSize) && pageSize > 0) {
        params.push({ label: '条数', value: String(pageSize) });
      }
    }
    const details = active.slice(0, 5).map((alarm, index) => ({
      label: `${alarmLevelLabel(alarm.alarmLevel || alarm.level || alarm.severity)}告警 ${index + 1}`,
      value: [
        alarmThingLabel(alarm),
        asString(alarm.message || alarm.ruleName || alarm.name),
        asString(alarm.alarmTime || alarm.createdAt),
        alarm.isAcknowledged === true ? '已确认' : '未确认',
      ]
        .filter(Boolean)
        .join(' · '),
    }));
    return {
      viewKey: 'alarm.list',
      title: '检查当前告警',
      summary: active.length > 0 ? `${active.length} 条待处理 · ${critical} 条严重` : '当前没有待处理告警',
      status: statusOf(input),
      params,
      metrics: [
        { label: '待处理', value: String(active.length) },
        { label: '严重', value: String(critical) },
        { label: '未确认', value: String(unacknowledged) },
      ],
      details,
    };
  },
};

export const eventHistoryView: ToolView = {
  key: 'event.history',
  card: 'doc',
  tone: 'read',
  verb: '回看设备事件',
  present(input) {
    const record = recordOf(parseJson(input.result));
    if (!record) return null;
    const events = arrayFrom(record, ['events', 'items', 'history'])
      .map(recordOf)
      .filter((item): item is Record<string, unknown> => item !== null);
    const total = typeof record.total === 'number' ? record.total : events.length;
    const args = recordOf(input.args);
    const params: ToolPresentation['params'] = [];
    if (args) {
      const start = asString(args.startTime || args.start_time);
      const end = asString(args.endTime || args.end_time);
      if (start || end) {
        params.push({ label: '时间', value: `${shortTime(start) || '—'}~${shortTime(end) || '现在'}` });
      }
      const thingId = asString(args.thingId || args.thing_id || args.deviceId);
      if (thingId) params.push({ label: '设备', value: thingId });
      const pageSize = Number(args.pageSize ?? args.page_size ?? args.limit);
      if (Number.isFinite(pageSize) && pageSize > 0) params.push({ label: '条数', value: String(pageSize) });
    }
    const details = events.slice(0, 5).map((event, index) => ({
      label: `事件 ${index + 1}`,
      value: [
        asString(event.occurredAt || event.time || event.at),
        asString(event.type || event.eventType),
        asString(event.message || event.text),
      ]
        .filter(Boolean)
        .join(' · '),
    }));
    return {
      viewKey: 'event.history',
      title: '回看设备最近的事件',
      summary: `回看 ${total} 条事件`,
      status: statusOf(input),
      params,
      metrics: [{ label: '事件数', value: String(total) }],
      details,
    };
  },
};

export const knowledgeView: ToolView = {
  key: 'knowledge.search',
  card: 'doc',
  tone: 'skill',
  verb: '查找知识资料',
  present(input) {
    const record = recordOf(parseJson(input.result));
    const items = record ? arrayFrom(record, ['results', 'items', 'documents', 'docs']).map(recordOf).filter((item): item is Record<string, unknown> => item !== null) : [];
    const total = record && typeof record.total === 'number' ? record.total : items.length;
    const args = recordOf(input.args);
    const params: ToolPresentation['params'] = [];
    if (args) {
      const query = asString(args.query || args.keyword || args.q || args.documentId || args.document_id);
      if (query) params.push({ label: input.name === 'read_document' ? '文档' : '关键词', value: clampText(query, 24) });
    }
    return {
      viewKey: 'knowledge.search',
      title: input.name === 'read_document' ? '查阅运行手册' : '查找知识资料',
      summary: record ? `共 ${total} 条资料` : input.result ? '已读取' : '执行中…',
      status: statusOf(input),
      params,
      details: items.slice(0, 5).map((item, index) => ({
        label: `资料 ${index + 1}`,
        value: [
          asString(item.title || item.name || item.documentId),
          asString(item.snippet || item.summary || item.content).slice(0, 100),
        ].filter(Boolean).join(' · '),
      })),
    };
  },
};

export const knowledgeDocumentView: ToolView = {
  ...knowledgeView,
  key: 'knowledge.document',
  card: 'doc',
  tone: 'skill',
  verb: '查阅运行手册',
};

function skillMetadata(result: string | null | undefined): { title: string; description: string } {
  if (!result || !result.startsWith('---')) return { title: '', description: '' };
  const end = result.indexOf('---', 3);
  if (end < 0) return { title: '', description: '' };
  const frontmatter = result.slice(3, end);
  const read = (key: string): string =>
    frontmatter
      .split(/\r?\n/)
      .find((line) => line.trimStart().startsWith(`${key}:`))
      ?.split(':')
      .slice(1)
      .join(':')
      .trim()
      .replace(/^["']|["']$/g, '') ?? '';
  const heading = result
    .slice(end + 3)
    .split(/\r?\n/)
    .find((line) => line.startsWith('# '))
    ?.slice(2)
    .trim() ?? '';
  return { title: read('title') || heading, description: read('description') };
}

export const skillView: ToolView = {
  key: 'skill.load',
  card: 'skill',
  tone: 'skill',
  verb: '查阅作业规范',
  present(input) {
    const args = recordOf(input.args);
    const name = asString(args?.skill_name) || asString(args?.skillId) || '';
    const meta = skillMetadata(input.result);
    const title = meta.title || name || '作业规范';
    return {
      viewKey: 'skill.load',
      title: `查阅「${title}」作业规范`,
      summary: meta.description || (input.result ? '已载入' : '加载中…'),
      status: statusOf(input),
      details: [
        { label: '技能', value: title },
        ...(meta.description ? [{ label: '说明', value: meta.description }] : []),
      ],
    };
  },
};

export const todoView: ToolView = {
  key: 'todo.write',
  card: 'plan',
  tone: 'plan',
  verb: '更新工作计划',
  present(input) {
    const record = recordOf(parseJson(input.result));
    const args = recordOf(input.args);
    const source = record && Array.isArray(record.todos) ? record : (args ?? {});
    const todos = arrayFrom(source, ['todos']);
    const done = todos.filter((item) => recordOf(item)?.status === 'completed').length;
    return {
      viewKey: 'todo.write',
      title: '更新工作计划',
      summary: todos.length > 0 ? `计划 ${done}/${todos.length} 项完成` : '计划已更新',
      status: statusOf(input),
      metrics: [
        { label: '已计划', value: String(todos.length) },
        { label: '已完成', value: String(done) },
      ],
      details: todos.slice(0, 8).map((item, index) => {
        const todo = recordOf(item);
        return {
          label: `任务 ${index + 1}`,
          value: `${asString(todo?.status)} · ${asString(todo?.content)}`,
        };
      }),
    };
  },
};

export const questionView: ToolView = {
  key: 'interaction.ask',
  card: 'generic',
  tone: 'ask',
  verb: '请你决定',
  present(input) {
    const args = recordOf(input.args);
    const questions = Array.isArray(args?.questions) ? args.questions : [];
    const first = recordOf(questions[0]);
    return {
      viewKey: 'interaction.ask',
      title: '需要你确认一件事',
      summary: asString(first?.header) || asString(first?.question) || '等待你的回答',
      status: input.result ? 'ok' : 'running',
      details: questions.slice(0, 4).map((item, index) => {
        const question = recordOf(item);
        return {
          label: `选项组 ${index + 1}`,
          value: asString(question?.header) || asString(question?.question),
        };
      }),
    };
  },
};

export const subagentView: ToolView = {
  key: 'subagent.spawn',
  card: 'subagent',
  tone: 'analysis',
  verb: '安排专项分析',
  present(input) {
    const args = recordOf(input.args);
    return {
      viewKey: 'subagent.spawn',
      title: '安排专项分析',
      summary: asString(args?.description) || asString(args?.prompt) || '子代理处理中',
      status: statusOf(input),
      details: [
        ...(asString(args?.description) ? [{ label: '任务', value: asString(args?.description) }] : []),
        ...(asString(args?.prompt) ? [{ label: '指令', value: asString(args?.prompt).slice(0, 120) }] : []),
      ],
    };
  },
};

export const watchView: ToolView = {
  key: 'watch.create',
  card: 'watch',
  tone: 'plan',
  verb: '创建监听',
  present(input) {
    return {
      viewKey: 'watch.create',
      title: input.name === 'unwatch' ? '取消条件等待' : '建立条件等待',
      summary: input.result ? '已生效' : '执行中…',
      status: statusOf(input),
      details: [
        ...(asString((recordOf(input.args) ?? {}).condition) ? [{ label: '条件', value: asString((recordOf(input.args) ?? {}).condition) }] : []),
        ...(asString((recordOf(input.args) ?? {}).thingId) ? [{ label: '设备', value: asString((recordOf(input.args) ?? {}).thingId) }] : []),
      ],
    };
  },
};

export const compositeView: ToolView = {
  key: 'composite.list',
  card: 'list',
  tone: 'read',
  verb: '组合查询',
  present(input) {
    const record = recordOf(parseJson(input.result));
    const items = record ? arrayFrom(record, ['composites', 'items']) : [];
    return {
      viewKey: input.name === 'run_composite' ? 'composite.run' : 'composite.list',
      title: input.name === 'run_composite' ? '执行动作链' : '列出动作链',
      summary: input.name === 'run_composite'
        ? input.result ? '已执行' : '执行中…'
        : `共 ${items.length} 条动作链`,
      status: statusOf(input),
    };
  },
};

export const skillCompleteView: ToolView = {
  key: 'skill.complete',
  card: 'generic',
  tone: 'plan',
  verb: '结算技能',
  present(input) {
    const lines = (input.result ?? '')
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean);
    const first = lines[0] ?? '';
    const failed = input.status === 'failed';
    return {
      viewKey: 'skill.complete',
      title: failed ? '结算技能失败' : '结算技能',
      summary: failed
        ? '技能结算失败'
        : first.replace(/^已结算技能：/, '').slice(0, 80) || '技能已结算',
      status: statusOf(input),
    };
  },
};
