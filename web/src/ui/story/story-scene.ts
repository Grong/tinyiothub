/**
 * 星云流水页的场景投影：StoryView → 技能行 + 卡片 + 待确认 + 总结。
 *
 * 纯函数，不碰 DOM。Story page 仍然是唯一的数据入口（buildStory），这里只负责
 * 把同一份投影重新编排成「一次技能一行、卡片按序连线」的结构，供 view-story-map 渲染。
 */

import type { PendingApproval, PendingQuestion } from '../chat/timeline-model.js';
import { cardFamilyOf } from '../tool-views/registry.js';
import { MAP_COPY } from './story-map-copy.js';
import type { StorySection, StoryStep, StoryView, StoryVision } from './story-model.js';

export type SceneCardKind =
  | 'skill'
  | 'device'
  | 'list'
  | 'doc'
  | 'vision'
  | 'think'
  | 'finding'
  | 'action'
  | 'plan'
  | 'watch'
  | 'subagent'
  | 'generic'
  | 'conclusion'
  | 'error';

export type SceneStatus = 'running' | 'done' | 'failed';

export interface SceneMetric {
  label: string;
  value: string;
  warn?: boolean;
}

export interface SceneCard {
  id: string;
  kind: SceneCardKind;
  at: string;
  title: string;
  sub: string;
  status: SceneStatus;
  metrics: SceneMetric[];
  /** 调用参数（筛选条件/时间范围），用于区分看起来一样的调用。 */
  params?: SceneMetric[];
  /** 参数指纹，连续调用是否完全一致。 */
  argsKey?: string;
  tool?: string;
  detail?: string;
  count?: number;
  /** 叠卡展开的子卡片（同批设备/画面/思考）。 */
  items?: SceneCard[];
  /** 连续重复调用合并后的次数（>1 时显示 ×N）。 */
  repeatCount?: number;
  images?: StoryVision[];
}

export interface SceneNote {
  at: string;
  text: string;
  /** 插在第几张卡片之前；等于 cards.length 表示行尾。 */
  index: number;
  /** 这条判断之后紧接着做的动作（卡片标题）。 */
  nextCardTitle?: string;
}

export interface SceneLane {
  id: string;
  /** 技能标识（inspection / video-inspection…），用于合并重复标记。 */
  skillKey?: string;
  skill: string;
  at: string;
  status: SceneStatus;
  cards: SceneCard[];
  /** 思考不占卡片：收在这一行的「判断依据」里。 */
  notes: SceneNote[];
  steps: number;
  /** 还没加载技能、只有开场思考的行。 */
  prologue?: boolean;
}

export interface ScenePending {
  id: string;
  kind: 'ask' | 'approval';
  title: string;
  text: string;
  at: string;
  options: string[];
}

export interface SceneSummary {
  text: string;
  status: string;
  tone: 'ok' | 'warn';
  devices: number;
  vision: number;
  actions: number;
  /** 执行失败的动作数，单独统计，不混进「处置」。 */
  failedActions: number;
  findings: number;
  steps: number;
  skills: string[];
}

export interface SceneFinal {
  at: string;
  status: string;
  tone: 'ok' | 'warn';
  text: string;
  /** 完整结论（清理 markdown 后），用于就地展开。 */
  fullText: string;
  cols: Array<[string, string, string]>;
  meta: string;
}

export interface StoryScene {
  lanes: SceneLane[];
  pending: ScenePending[];
  summary: SceneSummary;
  final: SceneFinal | null;
  totalSteps: number;
  /** 更早的轮次数（当前只展示最近一轮）。 */
  earlierTurns: number;
  /** 超过 MAX_LANES 被折叠掉的技能数。 */
  hiddenSkills: number;
}



const TONE_FAMILY: Record<string, SceneCardKind> = {
  read: 'doc',
  write: 'action',
  alarm: 'finding',
  plan: 'plan',
  skill: 'doc',
  analysis: 'subagent',
  think: 'think',
};

const GROUPABLE: ReadonlySet<SceneCardKind> = new Set(['device', 'vision', 'think']);
/** 一叠最多展开 5 张，超过就拆成下一叠，避免悬停时把整行撑爆。 */
const MAX_STACK = 5;

const MAX_METRICS = 12;

function clean(text: string): string {
  return text
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[*_`#>|]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

function firstSentence(text: string, max: number): string {
  const cleanText = clean(text);
  if (cleanText.length <= max) return cleanText;
  const cut = cleanText.search(/[。！？!?]/);
  if (cut > 8 && cut < max) return cleanText.slice(0, cut + 1);
  return `${cleanText.slice(0, max).trimEnd()}…`;
}

function kindOf(step: StoryStep): SceneCardKind | 'skip' {
  // 优先用工具展示键决定视觉族；漏配 viewKey 时退回 tone，再不行按通用卡渲染。
  if (step.viewKey === 'skill.load') return step.skill ? 'skill' : 'doc';
  if (step.viewKey === 'interaction.ask' || step.viewKey === 'approval.request') return 'skip';
  const family = cardFamilyOf(step.viewKey);
  if (family) return family;
  // 有 viewKey 但没登记视觉族：用通用卡，而不是猜成某个业务族。
  if (step.viewKey && step.viewKey !== 'generic') return 'generic';
  if (step.skill) return 'skill';
  return TONE_FAMILY[step.tone] ?? 'doc';
}

function stepCard(step: StoryStep): SceneCard {
  const kind = kindOf(step);
  const metrics: SceneMetric[] = (step.metrics ?? [])
    .slice(0, MAX_METRICS)
    .map((metric) => ({ label: metric.name, value: metric.value }));

  return {
    id: step.id,
    kind: kind === 'skip' ? 'think' : kind,
    at: step.at,
    title: step.title,
    sub: step.detail,
    status: step.status,
    metrics,
    params: (step.params ?? []).slice(0, 3).map((param) => ({
      label: param.label,
      value: param.value,
    })),
    argsKey: step.argsKey,
    tool: step.tool,
    detail: step.finding || undefined,
  };
}

function visionCard(vision: StoryVision): SceneCard {
  return {
    id: `vision-${vision.evidenceId}`,
    kind: 'vision',
    at: vision.at,
    title: vision.deviceName,
    sub: vision.caption,
    status: 'done',
    metrics: [],
    images: [vision],
    detail: vision.finding || undefined,
  };
}

function stack(cards: SceneCard[]): SceneCard {
  const [first] = cards;
  return {
    ...first,
    id: `${first.id}-stack`,
    items: cards,
    count: cards.length,
    title: first.title,
    sub: `${cards.length} 项`,
    status: cards.some((card) => card.status === 'running')
      ? 'running'
      : cards.some((card) => card.status === 'failed')
        ? 'failed'
        : 'done',
  };
}

function groupCardsMapped(cards: SceneCard[]): {
  groups: SceneCard[];
  positions: Map<number, number>;
} {
  const groups: SceneCard[] = [];
  const positions = new Map<number, number>();
  let run: SceneCard[] = [];
  let runStart = 0;

  const flushRun = () => {
    for (let offset = 0; offset < run.length; offset += MAX_STACK) {
      const chunk = run.slice(offset, offset + MAX_STACK);
      const groupIndex = groups.length;
      groups.push(chunk.length > 1 ? stack(chunk) : chunk[0]);
      chunk.forEach((_, index) => positions.set(runStart + offset + index, groupIndex));
    }
    run = [];
  };

  cards.forEach((card, index) => {
    if (!GROUPABLE.has(card.kind)) {
      flushRun();
      positions.set(index, groups.length);
      groups.push(card);
      return;
    }
    if (run.length > 0 && run[0].kind !== card.kind) flushRun();
    if (run.length === 0) runStart = index;
    run.push(card);
  });
  flushRun();
  positions.set(cards.length, groups.length);
  return { groups, positions };
}

export function groupCards(cards: SceneCard[]): SceneCard[] {
  return groupCardsMapped(cards).groups;
}

const NON_REPEAT: ReadonlySet<SceneCardKind> = new Set(['skill', 'conclusion', 'error']);

function cardKey(card: SceneCard): string {
  return [
    card.kind,
    card.title,
    card.sub,
    card.tool ?? '',
    card.status,
    card.argsKey ?? '',
    card.metrics.map((metric) => `${metric.label}=${metric.value}`).join('|'),
  ].join('~');
}

/** 连续且完全相同的调用合成一张卡，用 ×N 表示重复次数。 */
function mergeRepeats(cards: SceneCard[]): {
  cards: SceneCard[];
  positions: Map<number, number>;
} {
  const out: SceneCard[] = [];
  const positions = new Map<number, number>();
  cards.forEach((card, index) => {
    const last = out[out.length - 1];
    if (
      last &&
      !last.items &&
      last.kind === card.kind &&
      !NON_REPEAT.has(card.kind) &&
      cardKey(last) === cardKey(card)
    ) {
      out[out.length - 1] = {
        ...last,
        repeatCount: (last.repeatCount ?? 1) + 1,
        at: card.at,
      };
      positions.set(index, out.length - 1);
      return;
    }
    positions.set(index, out.length);
    out.push(card);
  });
  positions.set(cards.length, out.length);
  return { cards: out, positions };
}


function sectionStats(section: StorySection) {
  let vision = 0;
  let actions = 0;
  let failedActions = 0;
  let findings = 0;
  const devices = new Set<string>();
  let steps = 0;

  for (const step of section.steps) {
    steps += 1;
    vision += step.visionIds.length;
    findings += step.finding ? 1 : 0;
    const kind = kindOf(step);
    if (kind === 'action') {
      if (step.status === 'failed') failedActions += 1;
      else if (step.status === 'done') actions += 1;
    }
    if (step.deviceId) devices.add(step.deviceId);
  }
  return { devices: devices.size, vision, actions, failedActions, findings, steps };
}

function laneStatus(section: StorySection, cards: SceneCard[]): SceneStatus {
  if (section.status === 'failed') return 'failed';
  if (section.status === 'running' || cards.some((card) => card.status === 'running')) {
    return 'running';
  }
  return 'done';
}

/** 合并相邻的同名技能行（前一行只有技能卡时），兼容旧数据里的重复标记。 */
export function mergeDuplicateLanes(lanes: SceneLane[]): SceneLane[] {
  const out: SceneLane[] = [];
  for (const lane of lanes) {
    const last = out[out.length - 1];
    if (
      last &&
      lane.skillKey &&
      last.skillKey === lane.skillKey &&
      last.notes.length === 0 &&
      last.cards.length === 1
    ) {
      last.cards.push(...lane.cards.slice(1));
      last.notes.push(
        ...lane.notes.map((note) => ({ ...note, index: Math.max(1, note.index) })),
      );
      last.steps += Math.max(0, lane.steps - 1);
      if (lane.status === 'failed') last.status = 'failed';
      continue;
    }
    out.push(lane);
  }
  return out;
}

function buildLanes(section: StorySection, story: StoryView): SceneLane[] {
  const visionById = new Map(story.vision.map((item) => [item.evidenceId, item]));
  const lanes: SceneLane[] = [];
  let lane: SceneLane | null = null;
  let cards: SceneCard[] = [];

  const flush = () => {
    if (!lane) return;
    const repeats = mergeRepeats(cards);
    const { groups, positions } = groupCardsMapped(repeats.cards);
    // 重复合并 + 叠卡都会改变卡片数量，思考标记的位置要跟着重映射。
    for (const note of lane.notes) {
      const merged = repeats.positions.get(note.index) ?? note.index;
      note.index = positions.get(merged) ?? merged;
      note.nextCardTitle = groups[note.index]?.title;
    }
    lane.cards = groups;
    lanes.push(lane);
  };

  const steps = section.steps;
  for (let index = 0; index < steps.length; index += 1) {
    const step = steps[index];
    // 一次技能加载会产生两条步骤：get_skill 工具调用 + skill/loaded。
    // 只保留后者（带 skill 元数据），避免同一件事出现两张卡。
    if (step.viewKey === 'skill.load' && !step.skill && step.status !== 'failed') continue;
    const kind = kindOf(step);
    if (kind === 'skip') continue;
    // 计划更新已经有左侧统计和总结里的「接下来」，不再占卡片。
    if (step.viewKey === 'todo.write') continue;
    if (kind === 'think') {
      if (!lane) {
        lane = {
          id: `${section.id}-lane`,
          skill: MAP_COPY.initialLane,
          at: section.startedAt,
          status: 'done',
          prologue: true,
          cards: [],
          notes: [],
          steps: 0,
        };
      }
      lane.notes.push({
        at: step.at,
        text: step.detail || step.title,
        index: cards.length,
      });
      lane.steps += 1;
      continue;
    }

    if (kind === 'skill' && step.status === 'failed') {
      // 被拒绝/失败的技能加载不是新的一行，留在当前行里显示失败原因。
      if (!lane) {
        lane = {
          id: `${section.id}-lane`,
          skill: MAP_COPY.initialLane,
          at: section.startedAt,
          status: 'done',
          prologue: true,
          cards: [],
          notes: [],
          steps: 0,
        };
      }
      cards.push(stepCard(step));
      lane.steps += 1;
      continue;
    }

    if (kind === 'skill') {
      const title = step.skill?.title || step.title;
      // 自动接续和显式 get_skill 可能对同一个技能各发一次 skill/loaded：
      // 当前行还只有技能卡时直接合并，避免出现两条同名空行。
      if (
        lane &&
        step.skill?.name &&
        lane.skillKey === step.skill.name &&
        cards.length === 1 &&
        lane.notes.length === 0
      ) {
        lane.steps += 1;
        continue;
      }
      flush();
      lane = {
        id: step.id,
        skillKey: step.skill?.name,
        skill: title,
        at: step.at,
        status: 'done',
        cards: [],
        notes: [],
        steps: 0,
      };
      cards = [
        {
          id: `${step.id}-skill`,
          kind: 'skill',
          at: step.at,
          title,
          sub: step.skill?.description || step.detail,
          status: step.status,
          metrics: [],
          detail: step.skill?.description,
        },
      ];
      lane.steps += 1;
      continue;
    }

    if (!lane) {
      lane = {
        id: `${section.id}-lane`,
        skill: MAP_COPY.initialLane,
        at: section.startedAt,
        status: 'done',
        prologue: true,
        cards: [],
        notes: [],
        steps: 0,
      };
      cards = [];
    }

    // 结算技能是记账动作，不占卡片。
    if (step.viewKey === 'skill.complete') continue;

    const vision = step.visionIds
      .map((id) => visionById.get(id))
      .filter((item): item is StoryVision => Boolean(item));

    if (vision.length > 0) {
      for (const item of vision) cards.push(visionCard(item));
    } else {
      cards.push(stepCard(step));
    }
    lane.steps += 1;

  }
  flush();

  for (const item of lanes) {
    if (section.status === 'failed') {
      item.cards.push({
        id: `${item.id}-error`,
        kind: 'error',
        at: section.completedAt || item.at,
        title: '执行中断',
        sub: section.error || '步骤执行失败',
        status: 'failed',
        metrics: [],
      });
    }
    if (item === lanes[lanes.length - 1] && section.conclusion) {
      item.cards.push({
        id: `${item.id}-conclusion`,
        kind: 'conclusion',
        at: section.completedAt || item.at,
        title: finalTitle(section),
        sub: firstSentence(section.conclusion, 90),
        status: section.status === 'failed' ? 'failed' : section.status === 'running' ? 'running' : 'done',
        metrics: [],
        detail: section.conclusion,
      });
    }
    item.status = laneStatus(section, item.cards);
  }

  return lanes;
}

function finalTitle(section: StorySection): string {
  if (section.status === 'failed') return MAP_COPY.finalTitle.failed;
  if (section.status === 'running') return MAP_COPY.finalTitle.running;
  return MAP_COPY.finalTitle.done;
}

function planNext(plans: Array<{ content: string; status: string; activeForm?: string }>): string {
  const next = plans.find((item) => item.status !== 'completed');
  if (!next) return plans.length > 0 ? '计划已全部完成' : '暂无待办';
  return next.activeForm || next.content;
}

const MAX_LANES = 12;

export function buildScene(
  story: StoryView,
  options: {
    question?: PendingQuestion | null;
    approvals?: PendingApproval[];
    plans?: Array<{ content: string; status: string; activeForm?: string }>;
    /** 指定要展示的轮次；缺省用最新一轮。 */
    sectionId?: string;
  } = {},
): StoryScene {
  const section =
    (options.sectionId
      ? story.sections.find((item) => item.id === options.sectionId)
      : undefined) ??
    story.sections.find((item) => item.id === story.latestSectionId) ??
    story.sections[story.sections.length - 1];
  const earlierTurns = Math.max(0, story.sections.length - (section ? 1 : 0));

  if (!section) {
    return {
      lanes: [],
      pending: [],
      summary: {
        text: MAP_COPY.empty,
        status: '待开始',
        tone: 'warn',
        devices: 0,
        vision: 0,
        actions: 0,
        failedActions: 0,
        findings: 0,
        steps: 0,
        skills: [],
      },
      final: null,
      totalSteps: 0,
      earlierTurns: 0,
      hiddenSkills: 0,
    };
  }

  const allLanes = mergeDuplicateLanes(buildLanes(section, story));
  const lanes = allLanes.slice(0, MAX_LANES);
  const stats = sectionStats(section);
  const skills = lanes
    .map((lane) => lane.skill)
    .filter((skill) => skill !== MAP_COPY.initialLane);
  const failed = section.status === 'failed';
  const running = section.status === 'running';
  const statusText = MAP_COPY.turnStatus[section.status] ?? MAP_COPY.turnStatus.done;
  const text = section.conclusion
    ? firstSentence(section.conclusion, 120)
    : failed
      ? section.error || MAP_COPY.failedText
      : running
        ? MAP_COPY.runningText
        : MAP_COPY.noConclusion;

  const pending: ScenePending[] = [];
  if (options.question) {
    const first = options.question.questions[0];
    pending.push({
      id: options.question.questionId,
      kind: 'ask',
      title: first?.header || '需要你决定',
      text: first?.question || '',
      at: section.startedAt,
      options: first?.options.map((option) => option.label) ?? [],
    });
  }
  for (const approval of options.approvals ?? []) {
    pending.push({
      id: approval.approvalId,
      kind: 'approval',
      title: '需要审批',
      text: approval.summary || approval.tool,
      at: section.startedAt,
      options: [],
    });
  }

  const plans = options.plans ?? [];
  const next = planNext(plans);
  const final: SceneFinal = {
    at: section.completedAt || section.startedAt,
    status: statusText,
    tone: failed ? 'warn' : 'ok',
    text: text,
    fullText: clean(section.conclusion || text),
    cols: [
      [
        MAP_COPY.cols.did,
        `读取 ${stats.devices} 台设备`,
        `查看 ${stats.vision} 张画面 · ${stats.actions} 次处置${
          stats.failedActions > 0 ? ` · ${stats.failedActions} 次失败` : ''
        }`,
      ],
      [
        MAP_COPY.cols.result,
        failed ? '执行中断' : `${stats.findings} 项发现`,
        failed ? section.error || '需要人工介入' : `共 ${stats.steps} 步`,
      ],
      [MAP_COPY.cols.next, next, pending.length > 0 ? `${pending.length} 项待确认` : '无需确认'],
    ],
    meta: `${skills.length} 个技能 · ${stats.steps} 步`,
  };

  return {
    lanes,
    pending,
    summary: {
      text,
      status: statusText,
      tone: failed ? 'warn' : 'ok',
      ...stats,
      skills,
      steps: stats.steps,
    },
    final,
    totalSteps: stats.steps,
    earlierTurns,
    hiddenSkills: Math.max(0, allLanes.length - lanes.length),
  };
}
