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
  present(input) {
    return {
      viewKey: 'alarm.action',
      title: input.name.includes('ack') ? '确认告警' : '处理告警',
      summary: input.result ? '操作已提交' : '执行中…',
      status: statusOf(input),
    };
  },
};

export const deviceStopView: ToolView = {
  key: 'device.stop',
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

export const alarmListView: ToolView = {
  key: 'alarm.list',
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
    const alarms = arrayFrom(record, ['alarms', 'items'])
      .map(recordOf)
      .filter((item): item is Record<string, unknown> => item !== null);
    const total = typeof record.total === 'number' ? record.total : alarms.length;
    const critical = alarms.filter((alarm) =>
      ['critical', 'error', 'high'].includes(asString(alarm.level || alarm.severity)),
    ).length;
    const unacknowledged = alarms.filter((alarm) => alarm.isAcknowledged !== true).length;
    const details = alarms.slice(0, 5).map((alarm, index) => ({
      label: `告警 ${index + 1}`,
      value: [
        asString(alarm.level || alarm.severity),
        asString(alarm.thingName || alarm.deviceName || alarm.thingId),
        asString(alarm.message || alarm.ruleName || alarm.name),
      ]
        .filter(Boolean)
        .join(' · '),
    }));
    return {
      viewKey: 'alarm.list',
      title: '检查当前告警',
      summary: total > 0 ? `${total} 条待关注` : '当前没有告警',
      status: statusOf(input),
      metrics: [
        { label: '告警数', value: String(total) },
        { label: '严重', value: String(critical) },
        { label: '未确认', value: String(unacknowledged) },
      ],
      details,
    };
  },
};

export const eventHistoryView: ToolView = {
  key: 'event.history',
  present(input) {
    const record = recordOf(parseJson(input.result));
    if (!record) return null;
    const events = arrayFrom(record, ['events', 'items', 'history'])
      .map(recordOf)
      .filter((item): item is Record<string, unknown> => item !== null);
    const total = typeof record.total === 'number' ? record.total : events.length;
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
      metrics: [{ label: '事件数', value: String(total) }],
      details,
    };
  },
};

export const knowledgeView: ToolView = {
  key: 'knowledge.search',
  present(input) {
    const record = recordOf(parseJson(input.result));
    const items = record ? arrayFrom(record, ['results', 'items', 'documents', 'docs']) : [];
    const total = record && typeof record.total === 'number' ? record.total : items.length;
    return {
      viewKey: 'knowledge.search',
      title: input.name === 'read_document' ? '查阅运行手册' : '查找知识资料',
      summary: record ? `共 ${total} 条资料` : input.result ? '已读取' : '执行中…',
      status: statusOf(input),
    };
  },
};

export const knowledgeDocumentView: ToolView = {
  ...knowledgeView,
  key: 'knowledge.document',
};

export const skillView: ToolView = {
  key: 'skill.load',
  present(input) {
    const args = recordOf(input.args);
    const name = asString(args?.skill_name) || asString(args?.skillId) || '作业规范';
    return {
      viewKey: 'skill.load',
      title: `查阅《${name}》作业规范`,
      summary: input.result ? '已载入' : '加载中…',
      status: statusOf(input),
    };
  },
};

export const todoView: ToolView = {
  key: 'todo.write',
  present(input) {
    const record = recordOf(parseJson(input.result));
    const args = recordOf(input.args);
    const todos = arrayFrom(record ?? args ?? {}, ['todos']);
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
    };
  },
};

export const questionView: ToolView = {
  key: 'interaction.ask',
  present(input) {
    const args = recordOf(input.args);
    const questions = Array.isArray(args?.questions) ? args.questions : [];
    const first = recordOf(questions[0]);
    return {
      viewKey: 'interaction.ask',
      title: '需要你确认一件事',
      summary: asString(first?.header) || asString(first?.question) || '等待你的回答',
      status: input.result ? 'ok' : 'running',
    };
  },
};

export const subagentView: ToolView = {
  key: 'subagent.spawn',
  present(input) {
    const args = recordOf(input.args);
    return {
      viewKey: 'subagent.spawn',
      title: '安排专项分析',
      summary: asString(args?.description) || asString(args?.prompt) || '子代理处理中',
      status: statusOf(input),
    };
  },
};

export const watchView: ToolView = {
  key: 'watch.create',
  present(input) {
    return {
      viewKey: 'watch.create',
      title: input.name === 'unwatch' ? '取消条件等待' : '建立条件等待',
      summary: input.result ? '已生效' : '执行中…',
      status: statusOf(input),
    };
  },
};

export const compositeView: ToolView = {
  key: 'composite.list',
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
