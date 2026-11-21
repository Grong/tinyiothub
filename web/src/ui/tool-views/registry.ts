import type { ToolPresentation, ToolView, ToolViewInput } from './types.js';
import {
  deviceActionView,
  deviceListView,
  deviceReadView,
  deviceWriteView,
  propertyReadView,
} from './device-view.js';
import {
  alarmActionView,
  alarmListView,
  compositeView,
  deviceStopView,
  eventHistoryView,
  knowledgeDocumentView,
  knowledgeView,
  questionView,
  skillView,
  subagentView,
  thingTreeView,
  todoView,
  watchView,
} from './misc-views.js';

const NAME_TO_VIEW: Record<string, string> = {
  read_device: 'device.read',
  get_thing: 'device.read',
  get_thing_profile: 'device.read',
  list_devices: 'device.list',
  list_things: 'device.list',
  search_things: 'device.list',
  read_property: 'property.read',
  read_properties: 'property.read',
  set_device: 'device.write',
  write_properties: 'device.write',
  call_device: 'device.action',
  invoke_action: 'device.action',
  send_command: 'device.action',
  query_events: 'event.history',
  get_device_history: 'event.history',
  alarm_list: 'alarm.list',
  list_alarms: 'alarm.list',
  todo_write: 'todo.write',
  update_todo: 'todo.write',
  ask_user: 'interaction.ask',
  spawn_subagent: 'subagent.spawn',
  get_skill: 'skill.load',
  search_knowledge: 'knowledge.search',
  read_document: 'knowledge.document',
  stop: 'device.stop',
  get_thing_tree: 'thing.tree',
  alarm_acknowledge: 'alarm.action',
  alarm_resolve: 'alarm.action',
  watch: 'watch.create',
  unwatch: 'watch.create',
  list_composites: 'composite.list',
  run_composite: 'composite.run',
};

const VIEWS: Record<string, ToolView> = {
  [deviceReadView.key]: deviceReadView,
  [deviceListView.key]: deviceListView,
  [propertyReadView.key]: propertyReadView,
  [deviceWriteView.key]: deviceWriteView,
  [deviceActionView.key]: deviceActionView,
  [alarmListView.key]: alarmListView,
  [alarmActionView.key]: alarmActionView,
  [deviceStopView.key]: deviceStopView,
  [thingTreeView.key]: thingTreeView,
  [eventHistoryView.key]: eventHistoryView,
  [knowledgeView.key]: knowledgeView,
  [knowledgeDocumentView.key]: knowledgeDocumentView,
  [skillView.key]: skillView,
  [todoView.key]: todoView,
  [questionView.key]: questionView,
  [subagentView.key]: subagentView,
  [watchView.key]: watchView,
  [compositeView.key]: compositeView,
  'composite.run': compositeView,
};

export const genericView: ToolView = {
  key: 'generic',
  present(input) {
    const failed = input.status === 'failed';
    const name = input.name || '工具';
    if (failed) {
      return {
        viewKey: 'generic',
        title: name,
        summary: input.error || '执行失败',
        status: 'failed',
      };
    }
    if (!input.result) {
      return { viewKey: 'generic', title: name, summary: '执行中…', status: 'running' };
    }
    let parsed: unknown = null;
    try {
      parsed = JSON.parse(input.result);
    } catch {
      parsed = null;
    }
    const record = parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : null;
    return {
      viewKey: 'generic',
      title: name,
      summary: !parsed && /^[[{]/.test(input.result)
        ? '已完成'
        : typeof parsed === 'string' || !parsed
          ? input.result.replace(/\s+/g, ' ').slice(0, 120)
          : '已完成',
      status: 'ok',
      details: record
        ? Object.entries(record)
            .slice(0, 6)
            .map(([label, value]) => ({
              label,
              value: typeof value === 'string' ? value.slice(0, 120) : JSON.stringify(value).slice(0, 120),
            }))
        : undefined,
    };
  },
};

export function toolViewKey(name: string): string {
  return NAME_TO_VIEW[name] ?? 'generic';
}

/** Frontend presentation; unknown tools always get a generic fallback. */
export function presentTool(input: ToolViewInput): ToolPresentation {
  const key = input.viewKey && VIEWS[input.viewKey] ? input.viewKey : toolViewKey(input.name);
  const view = VIEWS[key];
  const presentation = view?.present(input);
  return presentation ?? genericView.present(input)!;
}

export function hasDedicatedToolView(name: string): boolean {
  return VIEWS[toolViewKey(name)] !== undefined;
}
