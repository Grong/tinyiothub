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

/** Frontend presentation; the backend event's viewKey is authoritative. */
export function presentTool(input: ToolViewInput): ToolPresentation {
  const view = input.viewKey ? VIEWS[input.viewKey] : undefined;
  const presentation = view?.present(input);
  return presentation ?? genericView.present(input)!;
}
