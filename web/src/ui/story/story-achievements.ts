/**
 * AI 大脑功绩汇总（今天 / 本周 / 本月）。
 *
 * 数据全部来自既有事实：brain_events（闭环/转人工/巡检正常）+ 最近一轮
 * StoryView 计数。不估算“省了多少人工”，不把一次工具调用包装成多个成果。
 */

import type { BrainEvent, BrainEventsSummary } from '../../api/brain-events.js';
import type { StoryView } from './story-model.js';

export type AchievementTab = 'today' | 'week' | 'month';

export interface AchievementBucket {
  key: string;
  label: string;
  closed: number;
  patrol: number;
  needsYou: number;
  total: number;
}

export interface StoryAchievement {
  devices: number;
  inspected: number;
  vision: number;
  findings: number;
  actions: number;
}

export interface AchievementStats {
  /** 最近一轮的设备/画面/发现，只用于「今天」页签的“本轮”块。 */
  latest: StoryAchievement;
  today: {
    closed: number;
    patrol: number;
    needsYou: number;
    digested: number;
    total: number;
  };
  week: {
    closed: number;
    patrol: number;
    needsYou: number;
    total: number;
    days: AchievementBucket[];
  };
  month: {
    closed: number;
    patrol: number;
    needsYou: number;
    total: number;
    buckets: AchievementBucket[];
  };
}

export interface AchievementInput {
  story: StoryView;
  events: BrainEvent[];
  summary: BrainEventsSummary | null;
  /** 当前会话最后更新时间（ms）；不是今天的记录时，本轮状态不计入今天。 */
  storyAt?: number;
  now?: Date;
}

const CLOSED_STATUS = new Set(['self_closed', 'resolved']);
const NEEDS_STATUS = new Set(['awaiting_approval', 'escalated']);
const PATROL_STATUS = new Set(['patrol_ok']);
const WEEKDAY = ['日', '一', '二', '三', '四', '五', '六'];

function startOfDay(value: Date): Date {
  return new Date(value.getFullYear(), value.getMonth(), value.getDate());
}

export function dayKey(value: Date): string {
  return `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, '0')}-${String(
    value.getDate(),
  ).padStart(2, '0')}`;
}

function eventTime(event: BrainEvent): Date {
  const candidates = [event.judgedAt, event.resolvedAt, event.stateEnteredAt, event.createdAt];
  for (const candidate of candidates) {
    if (!candidate) continue;
    const date = new Date(candidate);
    if (Number.isFinite(date.getTime())) return date;
  }
  return new Date(0);
}

/** 最近 7 天（含今天，按本地时区）的每日成果桶。 */
export function buildWeekBuckets(events: BrainEvent[], now = new Date()): AchievementBucket[] {
  const today = startOfDay(now);
  const buckets: AchievementBucket[] = [];
  for (let offset = 6; offset >= 0; offset -= 1) {
    const date = new Date(today);
    date.setDate(today.getDate() - offset);
    buckets.push({
      key: dayKey(date),
      label: WEEKDAY[date.getDay()] ?? '',
      closed: 0,
      patrol: 0,
      needsYou: 0,
      total: 0,
    });
  }
  const byKey = new Map(buckets.map((bucket) => [bucket.key, bucket]));
  for (const event of events) {
    const bucket = byKey.get(dayKey(eventTime(event)));
    if (!bucket) continue;
    bucket.total += 1;
    if (CLOSED_STATUS.has(event.status)) bucket.closed += 1;
    else if (PATROL_STATUS.has(event.status)) bucket.patrol += 1;
    else if (NEEDS_STATUS.has(event.status)) bucket.needsYou += 1;
  }
  return buckets;
}

/** 本月的周桶（1-7 / 8-14 / …），按周给领导看月度趋势。 */
export function buildMonthBuckets(events: BrainEvent[], now = new Date()): AchievementBucket[] {
  const year = now.getFullYear();
  const month = now.getMonth();
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const count = Math.ceil(daysInMonth / 7);
  const buckets: AchievementBucket[] = Array.from({ length: count }, (_, index) => {
    const startDay = index * 7 + 1;
    const endDay = Math.min(startDay + 6, daysInMonth);
    return {
      key: `${year}-${month + 1}-${startDay}`,
      label: `${startDay}-${endDay}`,
      closed: 0,
      patrol: 0,
      needsYou: 0,
      total: 0,
    };
  });
  for (const event of events) {
    const at = eventTime(event);
    if (at.getFullYear() !== year || at.getMonth() !== month) continue;
    const bucket = buckets[Math.floor((at.getDate() - 1) / 7)];
    if (!bucket) continue;
    bucket.total += 1;
    if (CLOSED_STATUS.has(event.status)) bucket.closed += 1;
    else if (PATROL_STATUS.has(event.status)) bucket.patrol += 1;
    else if (NEEDS_STATUS.has(event.status)) bucket.needsYou += 1;
  }
  return buckets;
}

export function computeAchievements(input: AchievementInput): AchievementStats {
  const now = input.now ?? new Date();
  const weekStart = startOfDay(now);
  weekStart.setDate(weekStart.getDate() - 6);
  const weekStartMs = weekStart.getTime();
  const monthStart = new Date(now.getFullYear(), now.getMonth(), 1);
  const monthStartMs = monthStart.getTime();
  const todayKey = dayKey(now);

  const storyIncluded =
    input.storyAt === undefined ? true : dayKey(new Date(input.storyAt)) === todayKey;

  let closedToday = 0;
  let patrolToday = 0;
  let totalToday = 0;
  let closedWeek = 0;
  let patrolWeek = 0;
  let totalWeek = 0;
  let closedMonth = 0;
  let patrolMonth = 0;
  let totalMonth = 0;
  let needsYou = 0;

  for (const event of input.events) {
    const at = eventTime(event);
    const atMs = at.getTime();
    const isToday = dayKey(at) === todayKey;
    if (NEEDS_STATUS.has(event.status)) needsYou += 1;

    if (isToday) {
      totalToday += 1;
      if (CLOSED_STATUS.has(event.status)) closedToday += 1;
      else if (PATROL_STATUS.has(event.status)) patrolToday += 1;
    }
    if (atMs >= weekStartMs) {
      totalWeek += 1;
      if (CLOSED_STATUS.has(event.status)) closedWeek += 1;
      else if (PATROL_STATUS.has(event.status)) patrolWeek += 1;
    }
    if (atMs >= monthStartMs) {
      totalMonth += 1;
      if (CLOSED_STATUS.has(event.status)) closedMonth += 1;
      else if (PATROL_STATUS.has(event.status)) patrolMonth += 1;
    }
  }

  const latest = input.story.latestStats;
  return {
    latest: {
      devices: storyIncluded ? latest.devices : 0,
      inspected: storyIncluded ? latest.inspected : 0,
      vision: storyIncluded ? latest.vision : 0,
      findings: storyIncluded ? latest.findings : 0,
      actions: storyIncluded ? latest.actions : 0,
    },
    today: {
      closed: closedToday,
      patrol: patrolToday,
      needsYou,
      digested: input.summary?.digestedToday ?? 0,
      total: totalToday,
    },
    week: {
      closed: closedWeek,
      patrol: patrolWeek,
      needsYou,
      total: totalWeek,
      days: buildWeekBuckets(input.events, now),
    },
    month: {
      closed: closedMonth,
      patrol: patrolMonth,
      needsYou,
      total: totalMonth,
      buckets: buildMonthBuckets(input.events, now),
    },
  };
}

export const EMPTY_ACHIEVEMENTS: AchievementStats = {
  latest: { devices: 0, inspected: 0, vision: 0, findings: 0, actions: 0 },
  today: { closed: 0, patrol: 0, needsYou: 0, digested: 0, total: 0 },
  week: { closed: 0, patrol: 0, needsYou: 0, total: 0, days: [] },
  month: { closed: 0, patrol: 0, needsYou: 0, total: 0, buckets: [] },
};
