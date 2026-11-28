import type { CardFamily, ToolImage, ToolPresentation, ToolTone, ToolView, ToolViewInput } from './types.js';
import { recordOf } from './parse.js';
import {
  deviceActionView,
  deviceListView,
  deviceProfileView,
  deviceKnowledgeView,
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
  skillCompleteView,
  skillView,
  subagentView,
  thingTreeView,
  todoView,
  watchView,
} from './misc-views.js';

const VIEWS: Record<string, ToolView> = {
  [deviceReadView.key]: deviceReadView,
  [deviceProfileView.key]: deviceProfileView,
  [deviceListView.key]: deviceListView,
  [propertyReadView.key]: propertyReadView,
  [deviceWriteView.key]: deviceWriteView,
  [deviceActionView.key]: deviceActionView,
  [deviceKnowledgeView.key]: deviceKnowledgeView,
  [alarmListView.key]: alarmListView,
  [alarmActionView.key]: alarmActionView,
  [deviceStopView.key]: deviceStopView,
  [thingTreeView.key]: thingTreeView,
  [eventHistoryView.key]: eventHistoryView,
  [knowledgeView.key]: knowledgeView,
  [knowledgeDocumentView.key]: knowledgeDocumentView,
  [skillView.key]: skillView,
  [skillCompleteView.key]: skillCompleteView,
  [todoView.key]: todoView,
  [questionView.key]: questionView,
  [subagentView.key]: subagentView,
  [watchView.key]: watchView,
  [compositeView.key]: compositeView,
  'composite.run': compositeView,
};

/** viewKey → 卡片视觉族。注册表里声明优先，其次按命名空间前缀推断。 */
const PREFIX_FAMILY: Record<string, CardFamily> = {
  device: 'device',
  property: 'device',
  alarm: 'finding',
  event: 'doc',
  knowledge: 'doc',
  todo: 'plan',
  subagent: 'subagent',
  watch: 'watch',
  composite: 'list',
  thing: 'list',
};

export function cardFamilyOf(viewKey?: string): CardFamily | null {
  if (!viewKey || viewKey === 'generic') return null;
  const view = VIEWS[viewKey];
  if (view?.card) return view.card;
  return PREFIX_FAMILY[viewKey.split('.')[0]] ?? null;
}

/** 工具的业务语气；未登记视图返回 null，由调用方决定兜底。 */
export function toolToneOf(viewKey?: string): ToolTone | null {
  if (!viewKey) return null;
  return VIEWS[viewKey]?.tone ?? null;
}

/** 工具的业务动词；未登记视图返回 null。 */
export function toolVerbOf(viewKey?: string): string | null {
  if (!viewKey) return null;
  return VIEWS[viewKey]?.verb ?? null;
}

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

function evidenceImages(input: ToolViewInput): ToolImage[] {
  const refs = Array.isArray(input.evidence) ? input.evidence : [];
  const parsed = (() => {
    try {
      return input.result ? JSON.parse(input.result) : null;
    } catch {
      return null;
    }
  })();
  const result = recordOf(parsed);
  const snapshot = recordOf(result?.snapshot);
  const rawSource =
    (typeof snapshot?.source === 'string' && snapshot.source) ||
    (typeof result?.source === 'string' && result.source) ||
    '';
  const imageCaption = rawSource === 'snapshot' ? '抓拍画面' : rawSource ? `现场画面 · ${rawSource}` : '现场画面';
  return refs
    .map(recordOf)
    .filter((ref): ref is Record<string, unknown> => ref !== null)
    .filter((ref) => {
      const mime = typeof ref.mime === 'string' ? ref.mime : '';
      return !mime || mime.startsWith('image/');
    })
    .map((ref) => ({
      evidenceId: String(ref.evidenceId ?? ref.evidence_id ?? ''),
      alt: imageCaption,
      caption: imageCaption,
    }))
    .filter((image) => image.evidenceId.length > 0);
}

/** Frontend presentation; the backend event's viewKey is authoritative. */
export function presentTool(input: ToolViewInput): ToolPresentation {
  const view = input.viewKey ? VIEWS[input.viewKey] : undefined;
  const presentation = view?.present(input) ?? genericView.present(input)!;
  const images = presentation.images ?? evidenceImages(input);
  if (images.length === 0) return presentation;
  const summary = presentation.summary.includes('张画面')
    ? presentation.summary
    : `${presentation.summary}${presentation.summary ? ' · ' : ''}${images.length} 张画面`;
  return { ...presentation, images, summary };
}
