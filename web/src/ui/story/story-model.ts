/**
 * 业务视角的工作纪实投影。
 *
 * 输入是 `/chat/sessions/{key}/events` 的 `session_events`（经 TimelineModel
 * 归一化后的 block 序列），输出是领导/普通用户能直接读懂的一班岗：
 *
 * - 每一步是什么业务动作（看设备、调画面、查告警、下指令……）
 * - 这一步涉及哪台设备、看到什么结果、有没有发现问题
 * - 每个画面证据（摄像头抓拍）属于哪台设备、什么时间
 * - 本轮结论、计划进度、整班统计
 *
 * 纯函数，不碰 DOM，不读全局状态；设备/画面/问题都按“用户可见的事实”合并
 * 去重，不重新解释工具协议。
 */

import type { TodoItem } from '../../api/assistant.js';
import { toolFailure, type PendingQuestion, type TimelineBlock } from '../chat/timeline-model.js';
import { presentTool } from '../tool-views/registry.js';
import type { ToolDetail } from '../tool-views/types.js';

export type StoryTone =
  | 'read'
  | 'write'
  | 'alarm'
  | 'plan'
  | 'ask'
  | 'analysis'
  | 'skill'
  | 'think';

export type StoryStepStatus = 'running' | 'done' | 'failed';

export interface StoryDeviceMetric {
  name: string;
  value: string;
}

export interface StoryDevice {
  id: string;
  name: string;
  type: string;
  zone: string;
  online?: boolean;
  health?: string;
  metrics: StoryDeviceMetric[];
  /** 最近一次被 AI 关注时做了什么（业务动词）。 */
  lastAction: string;
  lastAt: string;
  /** 被查看/操作的次数（卡牌排序与“关注热度”）。 */
  touches: number;
  lastOrder: number;
  /** 是否被真正查看过（清单里扫到但没读的设备不算巡检）。 */
  inspected: boolean;
}

export interface StoryVision {
  evidenceId: string;
  deviceId: string;
  deviceName: string;
  at: string;
  /** 例如「现场画面 · 通道 1」。 */
  caption: string;
  /** AI 对这张画面的结论（工具没有给出时为空）。 */
  finding: string;
  mime: string;
}

export interface StoryStep {
  id: string;
  title: string;
  detail: string;
  at: string;
  status: StoryStepStatus;
  tone: StoryTone;
  /** 原始工具名（展开时可看到，折叠行保持业务语言）。 */
  tool?: string;
  deviceId?: string;
  visionIds: string[];
  /** 这一步发现的问题；无事发生则为空。 */
  finding?: string;
  /** 设备步骤展开后的具体运行参数。 */
  metrics?: StoryDeviceMetric[];
  /** 工具 presentation 提供的结构化展开字段。 */
  details?: ToolDetail[];
}

export type StorySectionStatus = 'running' | 'done' | 'failed' | 'cancelled';

export interface StorySection {
  id: string;
  status: StorySectionStatus;
  startedAt: string;
  completedAt?: string;
  /** 触发来源（定时巡检/告警/事件/手动）。 */
  origin?: string;
  steps: StoryStep[];
  /** AI 的完整结论文本（markdown）。 */
  conclusion: string;
  /** AI 私有推理，只在“判断依据”里展开。 */
  reasoning: string;
  findingCount: number;
  /** Turn-level failure/cancel detail, when the turn did not complete. */
  error?: string;
  retries: number;
  lastRetryAt?: string;
  lastRetryError?: string;
}

export interface StoryPrompt {
  text: string;
  at: string;
}

export interface StoryFinding {
  deviceId: string;
  deviceName: string;
  label: string;
  at: string;
}

export interface StoryStats {
  /** 本班涉及的设备（含清单里扫到的）。 */
  devices: number;
  /** 真正查看过状态/参数/事件的设备。 */
  inspected: number;
  vision: number;
  findings: number;
  actions: number;
  steps: number;
}

export interface StoryRoundStats {
  devices: number;
  inspected: number;
  vision: number;
  findings: number;
  actions: number;
}

export interface StoryView {
  sections: StorySection[];
  prompts: StoryPrompt[];
  plans: TodoItem[];
  devices: StoryDevice[];
  vision: StoryVision[];
  /** 本班去重后的关键发现（按发现时间倒序）。 */
  findings: StoryFinding[];
  stats: StoryStats;
  /** 最近一轮真正看了设备/画面的章节；右栏卡牌只展示这一轮。 */
  latestSectionId: string;
  latestDevices: StoryDevice[];
  latestVision: StoryVision[];
  latestFindings: StoryFinding[];
  latestStats: StoryRoundStats;
  running?: StoryStep;
  /** 第一个触发来源，用于页头叙事。 */
  origin?: string;
}

interface ActionCopy {
  verb: string;
  tone: StoryTone;
  icon: string;
}

/** 工具名 → 业务动作。只做“翻译”，不改变任何执行语义。 */
const STORY_ACTION_COPY: Record<string, ActionCopy> = {
  list_things: { verb: '梳理设备清单', tone: 'read', icon: '☷' },
  list_devices: { verb: '梳理设备清单', tone: 'read', icon: '☷' },
  search_things: { verb: '查找设备', tone: 'read', icon: '⌕' },
  get_thing: { verb: '查看设备档案', tone: 'read', icon: '◎' },
  get_thing_profile: { verb: '查看设备档案', tone: 'read', icon: '◎' },
  read_device: { verb: '查看设备状态', tone: 'read', icon: '◎' },
  read_property: { verb: '读取运行参数', tone: 'read', icon: '◫' },
  read_properties: { verb: '读取运行参数', tone: 'read', icon: '◫' },
  query_events: { verb: '回看设备事件', tone: 'read', icon: '◷' },
  get_device_history: { verb: '回看历史数据', tone: 'read', icon: '◷' },
  query_data: { verb: '查询运行数据', tone: 'read', icon: '⌕' },
  aggregate_data: { verb: '汇总运行数据', tone: 'read', icon: 'Σ' },
  alarm_list: { verb: '检查告警', tone: 'alarm', icon: '⚠' },
  list_alarms: { verb: '检查告警', tone: 'alarm', icon: '⚠' },
  acknowledge_alarm: { verb: '确认告警', tone: 'write', icon: '✓' },
  resolve_alarm: { verb: '解除告警', tone: 'write', icon: '✓' },
  invoke_action: { verb: '执行设备操作', tone: 'write', icon: '▸' },
  call_device: { verb: '执行设备操作', tone: 'write', icon: '▸' },
  send_command: { verb: '下发设备命令', tone: 'write', icon: '▸' },
  set_device: { verb: '调整设备设置', tone: 'write', icon: '✎' },
  write_properties: { verb: '调整设备设置', tone: 'write', icon: '✎' },
  todo_write: { verb: '更新工作计划', tone: 'plan', icon: '☑' },
  update_todo: { verb: '更新工作计划', tone: 'plan', icon: '☑' },
  ask_user: { verb: '请你决定', tone: 'ask', icon: '?' },
  get_skill: { verb: '查阅作业规范', tone: 'skill', icon: '◆' },
  read_document: { verb: '查阅运行手册', tone: 'skill', icon: '▤' },
  search_knowledge: { verb: '查找知识资料', tone: 'skill', icon: '⌕' },
  spawn_subagent: { verb: '安排专项分析', tone: 'analysis', icon: '⧉' },
};

/** 触发来源的业务文案。 */
export const STORY_ORIGIN_LABEL: Record<string, string> = {
  schedule: '定时巡检',
  scheduled: '定时巡检',
  alarm: '告警触发',
  watch: '值守规则触发',
  thing_event: '设备事件触发',
  directive: '指令触发',
  event: '事件触发',
  manual: '手动发起',
};

const HEALTH_LABEL: Record<string, string> = {
  ok: '健康正常',
  attention: '需要关注',
  bad: '健康异常',
  unknown: '健康未知',
};

const TYPE_LABEL: Record<string, string> = {
  device: '设备',
  space: '空间',
  line: '产线',
  building: '建筑',
};

const DEVICE_TRIGGER_KEYS = ['thingId', 'thing_id', 'deviceId', 'device_id', 'id'];
const DEVICE_NAME_KEYS = ['displayName', 'display_name', 'name', 'title'];

function asString(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function recordOf(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function parseJsonValue(text?: string): unknown {
  if (!text) return undefined;
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return undefined;
  }
}

function firstString(source: Record<string, unknown> | null | undefined, keys: string[]): string {
  if (!source) return '';
  for (const key of keys) {
    const value = source[key];
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  return '';
}

function displayValue(value: unknown): string {
  if (value === null || value === undefined) return '—';
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return JSON.stringify(value);
}

function clampText(value: string, max = 120): string {
  const text = value.replace(/\s+/g, ' ').trim();
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

function quoteDevice(name: string): string {
  return `「${name}」`;
}

function deviceLabel(device: StoryDevice | null | undefined): string {
  return device ? quoteDevice(device.name) : '设备';
}

function actionName(part: Extract<TimelineBlock, { kind: 'tool' }>): string {
  return firstString(recordOf(part.args), [
    'actionName',
    'action_name',
    'action',
    'command',
    'name',
  ]);
}

function metricEntries(record: Record<string, unknown>): StoryDeviceMetric[] {
  const state = recordOf(record.state);
  if (!state) return [];
  return Object.entries(state)
    .slice(0, 4)
    .map(([name, value]) => ({ name, value: clampText(displayValue(value), 24) }));
}

function deviceFromRecord(record: Record<string, unknown>): StoryDevice | null {
  const id = firstString(record, DEVICE_TRIGGER_KEYS);
  if (!id) return null;
  const name = firstString(record, DEVICE_NAME_KEYS) || id;
  return {
    id,
    name,
    type: firstString(record, ['thingType', 'thing_type', 'type']),
    zone: firstString(record, ['zone', 'position', 'address']),
    online: typeof record.online === 'boolean' ? record.online : undefined,
    health: firstString(record, ['health']),
    metrics: metricEntries(record),
    lastAction: '',
    lastAt: '',
    touches: 0,
    lastOrder: -1,
    inspected: false,
  };
}

function deviceRecordsFromValue(value: unknown): Record<string, unknown>[] {
  if (Array.isArray(value)) {
    return value.map(recordOf).filter((item): item is Record<string, unknown> => item !== null);
  }
  const record = recordOf(value);
  if (!record) return [];
  for (const key of ['items', 'things', 'devices']) {
    const items = record[key];
    if (Array.isArray(items)) {
      return items.map(recordOf).filter((item): item is Record<string, unknown> => item !== null);
    }
  }
  return [record];
}

function upsertDevice(
  devices: Map<string, StoryDevice>,
  record: Record<string, unknown>,
  part: Extract<TimelineBlock, { kind: 'tool' }>,
  order: number,
  inspected: boolean,
): StoryDevice | null {
  const next = deviceFromRecord(record);
  if (!next) return null;
  const existing = devices.get(next.id);
  if (!existing) {
    next.touches = 1;
    next.lastOrder = order;
    next.lastAt = part.at;
    next.inspected = inspected;
    devices.set(next.id, next);
    return next;
  }
  existing.touches += 1;
  existing.inspected = existing.inspected || inspected;
  existing.lastOrder = Math.max(existing.lastOrder, order);
  existing.lastAt = part.at || existing.lastAt;
  if (next.name && next.name !== next.id) existing.name = next.name;
  if (next.type) existing.type = next.type;
  if (next.zone) existing.zone = next.zone;
  if (next.online !== undefined) existing.online = next.online;
  if (next.health) existing.health = next.health;
  if (next.metrics.length > 0) existing.metrics = next.metrics;
  return existing;
}

/** 从工具结果里回收设备，并把它们记进设备卡牌。 */
function collectDevices(
  devices: Map<string, StoryDevice>,
  part: Extract<TimelineBlock, { kind: 'tool' }>,
  order: number,
): StoryDevice | null {
  const parsed = parseJsonValue(part.result);
  const inspected = !['list_things', 'list_devices', 'search_things'].includes(part.name);
  let primary: StoryDevice | null = null;
  for (const record of deviceRecordsFromValue(parsed)) {
    const touched = upsertDevice(devices, record, part, order, inspected);
    if (touched && !primary) primary = touched;
  }

  const argId = firstString(recordOf(part.args), DEVICE_TRIGGER_KEYS);
  if (argId && !devices.has(argId)) {
    primary =
      upsertDevice(
        devices,
        {
          thingId: argId,
          name: firstString(recordOf(part.args), ['deviceName', 'thingName']) || argId,
        },
        part,
        order,
        inspected,
      ) ?? primary;
  }
  if (!primary && argId) primary = devices.get(argId) ?? null;
  return primary;
}

function findingOf(
  part: Extract<TimelineBlock, { kind: 'tool' }>,
  device: StoryDevice | null,
): string | undefined {
  // 清单/搜索只是在“圈定范围”，不能把某台设备的状态算成这一步的发现。
  if (['list_things', 'list_devices', 'search_things'].includes(part.name)) return undefined;
  if (device?.online === false) return '设备离线';
  if (device?.health === 'bad') return '健康异常';
  if (device?.health === 'attention') return '需要关注';

  const parsed = recordOf(parseJsonValue(part.result));
  if (parsed) {
    const alarms = parsed.alarms ?? parsed.items;
    if (Array.isArray(alarms) && alarms.length > 0 && part.name.includes('alarm')) {
      return `发现 ${alarms.length} 条告警`;
    }
    const sources = Array.isArray(parsed.sources) ? parsed.sources.map(recordOf) : [];
    const detection = sources.find((source) => source?.dataKind === 'detections');
    if (detection) {
      const text = firstString(detection, ['text']);
      if (text) return `画面：${clampText(text, 40)}`;
      if (Array.isArray(detection.data) && detection.data.length > 0) {
        return `画面识别 ${detection.data.length} 个目标`;
      }
    }
    const sourceErrors = parsed.sourceErrors;
    if (Array.isArray(sourceErrors) && sourceErrors.length > 0) return '现场画面读取失败';
    const success = parsed.success;
    if (success === false) return '操作未成功';
  }
  if (part.status === 'failed') return '操作未成功';
  return undefined;
}

function visionOf(
  part: Extract<TimelineBlock, { kind: 'tool' }>,
  device: StoryDevice | null,
): StoryVision[] {
  const refs = Array.isArray(part.evidence) ? part.evidence : [];
  if (refs.length === 0) return [];
  const parsed = recordOf(parseJsonValue(part.result));
  const sources = Array.isArray(parsed?.sources) ? parsed.sources : [];
  const analysis = firstString(parsed, ['analysis', 'verdict', 'finding', 'description']) || '';
  return refs.flatMap((raw) => {
    const reference = recordOf(raw);
    if (!reference) return [];
    const evidenceId = firstString(reference, ['evidenceId', 'evidence_id']);
    if (!evidenceId) return [];
    const sourceEntry = sources
      .map(recordOf)
      .find((entry) => entry && recordOf(entry.evidence)?.evidenceId === evidenceId);
    const sourceName =
      firstString(recordOf(sourceEntry?.evidence), ['source']) ||
      firstString(sourceEntry, ['source']);
    return [
      {
        evidenceId,
        deviceId: device?.id ?? '',
        deviceName: device?.name ?? '现场设备',
        at: firstString(reference, ['mediaTime', 'media_time']) || part.at,
        caption: sourceName ? `现场画面 · ${sourceName}` : '现场画面',
        finding: analysis ? clampText(analysis, 100) : '',
        mime: firstString(reference, ['mime']) || 'image/jpeg',
      },
    ];
  });
}

function detailOf(
  part: Extract<TimelineBlock, { kind: 'tool' }>,
  device: StoryDevice | null,
): string {
  if (part.status === 'failed')
    return clampText(part.error ?? toolFailure(part.result) ?? '执行未成功');
  if (!part.result) return '执行中…';

  const parsed = parseJsonValue(part.result);
  const record = recordOf(parsed);
  const name = part.name;

  if (device && ['read_device', 'get_thing', 'get_thing_profile'].includes(name)) {
    const facts = [
      device.online === undefined ? '' : device.online ? '在线' : '离线',
      HEALTH_LABEL[device.health ?? ''] ?? '',
      device.metrics.length > 0 ? `${device.metrics.length} 项运行参数` : '',
    ].filter(Boolean);
    if (facts.length > 0) return facts.join(' · ');
  }

  if (!record) {
    // 事件里的 result 被截断时，宁可说“已完成”，也不要编造“0 台/没有告警”。
    const text = asString(part.result).replace(/\s+/g, ' ').trim();
    if (text && !/^[[{]/.test(text)) return clampText(text, 90);
    return '已完成';
  }

  if (name === 'read_property' || name === 'read_properties') {
    const property = firstString(record, ['propertyName', 'property_name']) || '属性';
    const value = displayValue(record?.currentValue);
    const unit = asString(record?.unit);
    return `${property}：${value}${unit ? ` ${unit}` : ''}`;
  }

  if (['list_things', 'list_devices', 'search_things'].includes(name)) {
    const items = Array.isArray(record?.items) ? record.items.length : 0;
    const total = typeof record?.total === 'number' ? record.total : items;
    return total > items ? `共 ${total} 台，已载入 ${items} 台` : `共 ${total} 台`;
  }

  if (name === 'query_events') {
    const total = typeof record?.total === 'number' ? record.total : 0;
    return `回看 ${total} 条事件`;
  }

  if (name === 'alarm_list' || name === 'list_alarms') {
    const alarms = record?.alarms ?? record?.items;
    const count = Array.isArray(alarms)
      ? alarms.length
      : typeof record?.total === 'number'
        ? record.total
        : 0;
    return count > 0 ? `${count} 条待关注` : '当前没有告警';
  }

  if (name === 'set_device' || name === 'write_properties') {
    const updated = record?.updated;
    return Array.isArray(updated) && updated.length > 0
      ? `已调整 ${updated.length} 项设置`
      : '设置已生效';
  }

  if (name === 'invoke_action' || name === 'call_device' || name === 'send_command') {
    const status = firstString(record, ['status']);
    if (status === 'ok' || status === 'accepted' || status === 'submitted') return '已提交执行';
    return '已下发，等待设备回读';
  }

  if (name === 'todo_write' || name === 'update_todo') {
    const todos = Array.isArray(record?.todos) ? record.todos : [];
    const done = todos.filter((item) => recordOf(item)?.status === 'completed').length;
    return `计划 ${done}/${todos.length} 项完成`;
  }

  if (name === 'get_skill') {
    const skill = firstString(recordOf(part.args), ['skillId', 'skill', 'name']);
    return skill ? `规范 ${skill}` : '已载入作业规范';
  }

  if (name === 'ask_user') return '等待你的回答';

  const text = asString(part.result).replace(/\s+/g, ' ').trim();
  if (!text) return '';
  // 证据列只保留 2000 字符，长 JSON 可能被截断而无法解析；业务页不展示半截协议。
  if (/^[[{]/.test(text)) return '已完成';
  return clampText(text, 90);
}

function titleOf(
  part: Extract<TimelineBlock, { kind: 'tool' }>,
  device: StoryDevice | null,
): string {
  const copy = STORY_ACTION_COPY[part.name] ?? {
    verb: part.name,
    tone: 'read' as StoryTone,
    icon: '⚙',
  };
  const action = actionName(part);
  switch (part.name) {
    case 'read_device':
      return `查看${deviceLabel(device)}的实时状态`;
    case 'get_thing':
    case 'get_thing_profile':
      return `查看${deviceLabel(device)}的档案`;
    case 'read_property':
    case 'read_properties':
      return `读取${deviceLabel(device)}的运行参数`;
    case 'set_device':
    case 'write_properties':
      return `调整${deviceLabel(device)}的设置`;
    case 'invoke_action':
    case 'call_device':
    case 'send_command':
      return action ? `让${deviceLabel(device)}执行「${action}」` : `操作${deviceLabel(device)}`;
    case 'query_events':
    case 'get_device_history':
      return `回看${deviceLabel(device)}最近的事件`;
    case 'alarm_list':
    case 'list_alarms':
      return '检查当前告警';
    case 'list_things':
    case 'list_devices':
      return '梳理需要关注的设备';
    case 'ask_user':
      return '需要你确认一件事';
    case 'todo_write':
    case 'update_todo':
      return '更新工作计划';
    case 'get_skill':
      return '查阅作业规范';
    case 'spawn_subagent':
      return '安排专项分析';
    default:
      return device ? `${copy.verb}${deviceLabel(device)}` : copy.verb;
  }
}

function stepFromTool(
  part: Extract<TimelineBlock, { kind: 'tool' }>,
  device: StoryDevice | null,
): StoryStep {
  const copy = STORY_ACTION_COPY[part.name] ?? {
    verb: part.name,
    tone: 'read' as StoryTone,
    icon: '⚙',
  };
  const failed = part.status === 'failed';
  const status: StoryStepStatus = failed ? 'failed' : part.result ? 'done' : 'running';
  const presentation = presentTool({
    name: part.name,
    viewKey: part.viewKey,
    args: part.args,
    result: part.result ?? null,
    status: failed ? 'failed' : part.result ? 'ok' : 'running',
    error: part.error,
  });
  const dedicated = presentation.viewKey !== 'generic';
  return {
    id: part.id,
    title: dedicated ? presentation.title : titleOf(part, device),
    detail: dedicated ? presentation.summary : detailOf(part, device),
    at: part.at,
    status,
    tone: copy.tone,
    tool: part.name,
    deviceId: device?.id,
    visionIds: visionOf(part, device).map((item) => item.evidenceId),
    finding: findingOf(part, device),
    metrics: presentation.metrics?.map((metric) => ({ name: metric.label, value: metric.value })) ?? device?.metrics,
    details: presentation.details,
  };
}

function statusOf(status: string): StorySectionStatus {
  if (status === 'completed') return 'done';
  if (status === 'failed') return 'failed';
  if (status === 'cancelled') return 'cancelled';
  return 'running';
}

function uniqueFindingKey(step: StoryStep): string | null {
  if (!step.finding) return null;
  return `${step.deviceId ?? 'global'}:${step.finding}`;
}

export interface FailureCopy {
  detail: string;
  category: 'quota' | 'timeout' | 'auth' | 'network' | 'unknown';
}

/** Translate provider/turn errors into operator-facing copy. */
export function failureCopy(error: string): FailureCopy {
  const text = error.replace(/\s+/g, ' ').trim();
  if (/429|rate[ _-]?limit|token plan|用量上限|quota/i.test(text)) {
    return {
      category: 'quota',
      detail: '模型额度不足或已达到用量上限。请检查 Token Plan / 模型额度，下一轮会自动重试。',
    };
  }
  if (/timeout|timed out/i.test(text)) {
    return { category: 'timeout', detail: '模型响应超时，请稍后重试。' };
  }
  if (/401|403|unauthorized|api key|invalid key/i.test(text)) {
    return { category: 'auth', detail: '模型鉴权失败，请检查 API Key / 模型配置。' };
  }
  if (/connect|network|dns|socket/i.test(text)) {
    return { category: 'network', detail: '模型网络连接失败，请检查网络后重试。' };
  }
  return {
    category: 'unknown',
    detail: text ? clampText(text, 180) : '本轮执行失败，请查看工程师视图中的详细轨迹。',
  };
}

export function buildStory(
  blocks: TimelineBlock[],
  todos: TodoItem[] = [],
  pendingQuestion: PendingQuestion | null = null,
): StoryView {
  const devices = new Map<string, StoryDevice>();
  const vision: StoryVision[] = [];
  const sections: StorySection[] = [];
  const prompts: StoryPrompt[] = [];
  let current: StorySection | null = null;
  let pendingOrigin: string | undefined;
  let order = 0;

  const ensureSection = (at: string): StorySection => {
    if (!current) {
      current = {
        id: 'story-live',
        status: 'running',
        startedAt: at,
        origin: pendingOrigin,
        steps: [],
        conclusion: '',
        reasoning: '',
        findingCount: 0,
        retries: 0,
      };
      pendingOrigin = undefined;
      sections.push(current);
    }
    return current;
  };

  for (const part of blocks) {
    switch (part.kind) {
      case 'origin':
        pendingOrigin = part.source;
        break;
      case 'turn':
        current = {
          id: part.id,
          status: statusOf(part.status),
          startedAt: part.at,
          completedAt: part.completedAt,
          origin: pendingOrigin,
          steps: [],
          conclusion: '',
          reasoning: '',
          findingCount: 0,
          error: part.error,
          retries: 0,
        };
        pendingOrigin = undefined;
        sections.push(current);
        break;
      case 'user':
        if (part.text.trim()) prompts.push({ text: part.text, at: part.at });
        break;
      case 'assistant': {
        if (!part.text.trim()) break;
        const section = current ?? ensureSection(part.at);
        // Only the latest assistant settlement is the turn conclusion;
        // intermediate step narration must not become a premature summary.
        section.conclusion = part.text;
        break;
      }
      case 'reasoning': {
        if (!part.text.trim()) break;
        const section = current ?? ensureSection(part.at);
        section.reasoning = section.reasoning ? `${section.reasoning}${part.text}` : part.text;
        section.steps.push({
          id: part.id,
          title: 'AI 在思考',
          detail: part.text,
          at: part.at,
          status: part.streaming ? 'running' : 'done',
          tone: 'think',
          visionIds: [],
        });
        break;
      }
      case 'question': {
        const first = part.questions[0];
        const pending = part.outcome === 'pending';
        const answered = part.outcome === 'answered';
        const section = current ?? ensureSection(part.at);
        section.steps.push({
          id: `question:${part.questionId}`,
          title: pending
            ? first?.header
              ? `等待你确认：${first.header}`
              : '等待你确认'
            : answered
              ? first?.header
                ? `已确认：${first.header}`
                : '已确认'
              : first?.header
                ? `确认未完成：${first.header}`
                : '确认未完成',
          detail: first?.question ?? '',
          at: part.at,
          status: pending ? 'running' : answered ? 'done' : 'failed',
          tone: pending ? 'ask' : answered ? 'read' : 'alarm',
          visionIds: [],
        });
        break;
      }
      case 'approval': {
        const pending = part.outcome === 'pending';
        const section = current ?? ensureSection(part.at);
        section.steps.push({
          id: `approval:${part.approvalId}`,
          title: pending ? `等待审批：${part.tool || '设备操作'}` : `审批已处理：${part.tool || '设备操作'}`,
          detail: part.summary,
          at: part.at,
          status: pending ? 'running' : 'done',
          tone: pending ? 'ask' : 'read',
          visionIds: [],
        });
        break;
      }
      case 'retry': {
        const section = current ?? ensureSection(part.at);
        section.retries += 1;
        section.lastRetryAt = part.at;
        section.lastRetryError = part.error;
        break;
      }
      case 'tool': {
        if (part.name === 'canvas') break;
        const device = collectDevices(devices, part, order);
        order += 1;
        const step = stepFromTool(part, device);
        const section = current ?? ensureSection(part.at);
        section.steps.push(step);
        if (device) {
          device.lastAction = step.title;
          device.lastAt = part.at || device.lastAt;
        }
        for (const item of visionOf(part, device)) vision.push(item);
        break;
      }
      case 'skill': {
        const section = current ?? ensureSection(part.at);
        section.steps.push({
          id: part.id,
          title: part.name ? `按照《${part.name}》作业` : '查阅作业规范',
          detail: part.version ? `版本 ${part.version}` : '已载入',
          at: part.at,
          status: 'done',
          tone: 'skill',
          tool: 'get_skill',
          visionIds: [],
        });
        break;
      }
      case 'subagent': {
        const section = current ?? ensureSection(part.at);
        section.steps.push({
          id: part.id,
          title: part.description
            ? `安排专项分析：${clampText(part.description, 40)}`
            : '安排专项分析',
          detail: part.summary ? clampText(part.summary, 90) : '',
          at: part.at,
          status:
            part.status === 'failed' ? 'failed' : part.status === 'completed' ? 'done' : 'running',
          tone: 'analysis',
          visionIds: [],
        });
        break;
      }
      case 'notice': {
        // The turn header/status and the failure card explain terminal state;
        // a generic "会话结束：failed/cancelled" row is duplicate noise.
        if (part.text.startsWith('会话结束：')) break;
        const section = current ?? ensureSection(part.at);
        section.steps.push({
          id: part.id,
          title: part.text,
          detail: '',
          at: part.at,
          status: 'done',
          tone: 'ask',
          visionIds: [],
        });
        break;
      }
      default:
        break;
    }
  }

  if (
    pendingQuestion &&
    !blocks.some(
      (block) => block.kind === 'question' && block.questionId === pendingQuestion.questionId,
    )
  ) {
    const section = sections[sections.length - 1];
    const first = pendingQuestion.questions[0];
    if (section) {
      section.steps.push({
        id: `question:${pendingQuestion.questionId}`,
        title: first?.header ? `等待你确认：${first.header}` : '等待你确认',
        detail: first?.question ?? 'AI 正在等待你的回答。',
        at: pendingQuestion.at ?? section.startedAt,
        status: 'running',
        tone: 'ask',
        visionIds: [],
      });
    }
  }

  for (const section of sections) {
    if (section.retries > 0) {
      section.steps.push({
        id: `${section.id}:retry`,
        title: '模型调用重试',
        detail: `共 ${section.retries} 次`,
        at: section.lastRetryAt ?? section.startedAt,
        status: 'done',
        tone: 'analysis',
        visionIds: [],
      });
    }
    if (section.status === 'failed' || section.status === 'cancelled') {
      const copy = failureCopy(section.error ?? section.lastRetryError ?? '');
      const retryNote = section.retries > 0 ? ` 已重试 ${section.retries} 次。` : '';
      section.steps.push({
        id: `${section.id}:failure`,
        title: section.status === 'failed' ? '本轮未完成' : '本轮已取消',
        detail: `${copy.detail}${retryNote}`,
        at: section.completedAt ?? section.startedAt,
        status: section.status === 'failed' ? 'failed' : 'done',
        tone: 'alarm',
        visionIds: [],
      });
    }
    const seenInSection = new Set<string>();
    section.findingCount = section.steps.filter((step) => {
      const key = uniqueFindingKey(step);
      if (!key || seenInSection.has(key)) return false;
      seenInSection.add(key);
      return true;
    }).length;
  }

  const findingKeys = new Set<string>();
  const findings: StoryFinding[] = [];
  let actions = 0;
  let steps = 0;
  for (const section of sections) {
    for (const step of section.steps) {
      steps += 1;
      const key = uniqueFindingKey(step);
      if (key && step.finding && !findingKeys.has(key)) {
        findingKeys.add(key);
        findings.push({
          deviceId: step.deviceId ?? '',
          deviceName: step.deviceId
            ? (devices.get(step.deviceId)?.name ?? step.deviceId)
            : step.finding?.includes('告警')
              ? '告警'
              : '现场',
          label: step.finding,
          at: step.at,
        });
      }
      if (step.tone === 'write' && step.status !== 'failed') actions += 1;
    }
  }
  findings.reverse();

  const sortedDevices = [...devices.values()]
    .filter((device) => device.touches > 0)
    .sort((a, b) => b.lastOrder - a.lastOrder);
  const sortedVision = [...vision].sort((a, b) => {
    const left = new Date(a.at).getTime();
    const right = new Date(b.at).getTime();
    return (Number.isFinite(right) ? right : 0) - (Number.isFinite(left) ? left : 0);
  });
  const running =
    [...sections]
      .reverse()
      .flatMap((section) => [...section.steps].reverse())
      .find((step) => step.status === 'running') ?? undefined;

  // 右栏只回答“最近一轮发生了什么”。若最新一轮只有文字（例如追问），
  // 回退到最近一轮真正查看了设备/画面的章节。
  const latestSection =
    [...sections]
      .reverse()
      .find((section) =>
        section.steps.some((step) => step.deviceId || step.visionIds.length > 0),
      ) ?? sections[sections.length - 1];
  const latestDeviceIds = new Set<string>();
  const latestVisionIds = new Set<string>();
  const latestFindingKeys = new Set<string>();
  const latestFindings: StoryFinding[] = [];
  let latestActions = 0;
  for (const step of latestSection?.steps ?? []) {
    if (step.deviceId) latestDeviceIds.add(step.deviceId);
    for (const evidenceId of step.visionIds) latestVisionIds.add(evidenceId);
    const key = uniqueFindingKey(step);
    if (key && step.finding && !latestFindingKeys.has(key)) {
      latestFindingKeys.add(key);
      latestFindings.push({
        deviceId: step.deviceId ?? '',
        deviceName: step.deviceId
          ? (devices.get(step.deviceId)?.name ?? step.deviceId)
          : step.finding.includes('告警')
            ? '告警'
            : '现场',
        label: step.finding,
        at: step.at,
      });
    }
    if (step.tone === 'write' && step.status !== 'failed') latestActions += 1;
  }
  latestFindings.reverse();
  const latestDevices = sortedDevices.filter((device) => latestDeviceIds.has(device.id));
  const latestVision = sortedVision.filter((item) => latestVisionIds.has(item.evidenceId));

  return {
    sections,
    prompts,
    plans: todos,
    devices: sortedDevices,
    vision: sortedVision,
    findings,
    stats: {
      devices: sortedDevices.length,
      inspected: sortedDevices.filter((device) => device.inspected).length,
      vision: sortedVision.length,
      findings: findings.length,
      actions,
      steps,
    },
    latestSectionId: latestSection?.id ?? '',
    latestDevices,
    latestVision,
    latestFindings,
    latestStats: {
      devices: latestDevices.length,
      inspected: latestDevices.filter((device) => device.inspected).length,
      vision: latestVision.length,
      findings: latestFindings.length,
      actions: latestActions,
    },
    running,
    origin: sections.find((section) => section.origin)?.origin ?? pendingOrigin,
  };
}

export const EMPTY_STORY: StoryView = buildStory([], []);

/** 业务译文对外暴露，测试与页面标题共用同一份。 */
export function storyActionCopy(name: string): ActionCopy {
  return STORY_ACTION_COPY[name] ?? { verb: name, tone: 'read', icon: '⚙' };
}

/** 设备类型/健康的短标签（卡片与测试共用）。 */
export function deviceTypeLabel(type: string): string {
  return TYPE_LABEL[type] ?? type;
}

export function deviceHealthLabel(health: string | undefined): string {
  return health ? (HEALTH_LABEL[health] ?? health) : '';
}

/** 统一时间文案：无效/空值返回 —，同日只显示时:分。 */
export function formatStoryClock(iso: string, now = new Date()): string {
  if (!iso) return '—';
  const date = new Date(iso);
  if (!Number.isFinite(date.getTime())) return '—';
  const clock = date.toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });
  if (date.toDateString() === now.toDateString()) return clock;
  return `${date.getMonth() + 1}月${date.getDate()}日 ${clock}`;
}
