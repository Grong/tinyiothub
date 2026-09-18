import { describe, expect, it } from "vitest";
import {
  BADGE_MAP,
  evidenceRawText,
  segmentEvidence,
  approvalCountdown,
  emptyStateCopy,
  fmtJudgmentTime,
  needsYou,
  nextExpandedId,
  tabCount,
  validateWrongReason,
} from "./dispositions.js";
import type { BrainEvent, BrainEventsSummary } from "../../api/brain-events.js";

function ev(over: Partial<BrainEvent>): BrainEvent {
  return {
    id: "alarm:j1",
    workspaceId: "ws1",
    source: "alarm",
    alarmId: null,
    thingId: "t1",
    title: "",
    verdict: null,
    reason: "",
    suggestedAction: null,
    actionCategory: null,
    risk: null,
    runId: null,
    ticketId: null,
    status: "investigating",
    triageMode: "annotate",
    createdAt: new Date().toISOString(),
    judgedAt: null,
    stateEnteredAt: null,
    resolvedAt: null,
    ...over,
  };
}

describe("dispositions view helpers", () => {
  it("「需要你」= 待审批 + 需人工（F3/F15①）", () => {
    expect(needsYou(ev({ status: "awaiting_approval" }))).toBe(true);
    expect(needsYou(ev({ status: "escalated" }))).toBe(true);
    expect(needsYou(ev({ status: "investigating" }))).toBe(false);
    expect(needsYou(ev({ status: "resolved" }))).toBe(false);
    expect(needsYou(ev({ status: "self_closed" }))).toBe(false);
    expect(needsYou(ev({ status: "patrol_ok" }))).toBe(false);
  });

  it("badge map covers all 11 states（含 patrol_tick 行的 patrol_ok）", () => {
    const all = [
      "investigating",
      "self_closed",
      "awaiting_approval",
      "executing",
      "resolved",
      "escalated",
      "investigation_failed",
      "dismissed",
      "budget_skipped",
      "dispatch_suppressed",
      "patrol_ok",
    ] as const;
    for (const s of all) {
      expect(BADGE_MAP[s], `状态 ${s} 缺徽章`).toBeTruthy();
    }
    expect(Object.keys(BADGE_MAP).sort()).toEqual([...all].sort());
    // 语义抽查（brief 指定措辞）
    expect(BADGE_MAP.investigating).toBe("分析中");
    expect(BADGE_MAP.self_closed).toBe("自行闭环");
    expect(BADGE_MAP.awaiting_approval).toBe("需要你");
    expect(BADGE_MAP.executing).toBe("执行中");
    expect(BADGE_MAP.resolved).toBe("闭环");
    expect(BADGE_MAP.escalated).toBe("转工单");
    expect(BADGE_MAP.investigation_failed).toBe("转工单");
    expect(BADGE_MAP.dismissed).toBe("已拒绝");
    expect(BADGE_MAP.budget_skipped).toBe("未受理");
    expect(BADGE_MAP.dispatch_suppressed).toBe("未受理");
    expect(BADGE_MAP.patrol_ok).toBe("巡检正常");
  });

  it("tab counts from summary：needsYou 计数驱动 tab 角标", () => {
    const summary: BrainEventsSummary = {
      digestedToday: 12,
      needsYou: 3,
      latencyP50Secs: null,
      feedbackRight: 5,
      feedbackWrong: 2,
    };
    expect(tabCount(summary, "needs_you")).toBe(3);
    expect(tabCount(summary, "all")).toBeNull();
    expect(tabCount(summary, "patrol")).toBeNull();
    expect(tabCount(summary, "alarm")).toBeNull();
    expect(tabCount(summary, "directive")).toBeNull();
    expect(tabCount(null, "needs_you")).toBeNull();
  });

  it("时间戳规则（F12⑤）", () => {
    const now = new Date("2026-09-12T15:00:00").getTime();
    expect(fmtJudgmentTime(new Date(now - 30_000).toISOString(), now)).toBe("刚刚");
    expect(fmtJudgmentTime(new Date(now - 25 * 60_000).toISOString(), now)).toBe("25 分钟前");
    expect(fmtJudgmentTime(new Date(now - 3 * 3600_000).toISOString(), now)).toBe("12:00");
    expect(fmtJudgmentTime("2026-09-10T10:00:00", now)).toBe("9月10日");
  });

  it("审批倒计时（F6）：非待审批 → null；待审批 → 剩余小时", () => {
    expect(approvalCountdown(ev({ status: "escalated" }))).toBeNull();
    expect(approvalCountdown(ev({ status: "awaiting_approval" }))).toBeNull(); // 无 judgedAt
    const now = Date.now();
    const judged = ev({
      status: "awaiting_approval",
      judgedAt: new Date(now - 6 * 3600_000).toISOString(),
    });
    expect(approvalCountdown(judged, now)).toBe("18 小时后自动转工单");
    // 超时后钳到 0
    const overdue = ev({
      status: "awaiting_approval",
      judgedAt: new Date(now - 26 * 3600_000).toISOString(),
    });
    expect(approvalCountdown(overdue, now)).toBe("0 小时后自动转工单");
  });

  it("点错必填校验（F13）", () => {
    expect(validateWrongReason("")).toContain("必须填写原因");
    expect(validateWrongReason("  ")).toContain("必须填写原因");
    expect(validateWrongReason("太短")).toContain("必须填写原因");
    expect(validateWrongReason("这个传感器梅雨季会越限")).toBeNull();
  });
});

describe("行交互与空态（Task 5：点行展开 + 空态产品时刻）", () => {
  it("row click toggles evidence expand：点同一行收起，点他行切换", () => {
    expect(nextExpandedId(null, "a")).toBe("a");
    expect(nextExpandedId("a", "a")).toBeNull();
    expect(nextExpandedId("a", "b")).toBe("b");
    expect(nextExpandedId("b", "b")).toBeNull();
  });

  it("empty states carry product copy（needs_you / all 两案）", () => {
    const summary: BrainEventsSummary = {
      digestedToday: 7,
      needsYou: 0,
      latencyP50Secs: null,
      feedbackRight: 0,
      feedbackWrong: 0,
    };
    const ny = emptyStateCopy("needs_you", summary);
    expect(ny.title).toContain("一切正常");
    expect(ny.body).toContain("今日 7 条已消化");
    expect(ny.body).toContain("没有需要你处理的事");
    // summary 未落地时兜底仍成立
    expect(emptyStateCopy("needs_you", null).body).toContain("没有需要你处理的事");

    const all = emptyStateCopy("all", summary);
    expect(all.title).toContain("还没有事件");
    expect(all.body).toContain("大脑还没开始干活");
  });
});

describe("evidenceRawText（新行 summary / 存量老行 excerpt）", () => {
  it("prefers full summary over excerpt", () => {
    expect(evidenceRawText({ summary: "完整记录", excerpt: "旧摘录" })).toBe("完整记录");
    expect(evidenceRawText({ excerpt: "旧摘录" })).toBe("旧摘录");
  });

  it("returns empty for missing evidence", () => {
    expect(evidenceRawText(null)).toBe("");
    expect(evidenceRawText({})).toBe("");
    expect(evidenceRawText({ summary: 42 })).toBe("");
  });
});

describe("segmentEvidence（证据分段：think/verdict 折叠成段，正文保留）", () => {
  it("plain text stays one text segment", () => {
    expect(segmentEvidence("查了设备状态，温度 40 秒回落")).toEqual([
      { kind: "text", text: "查了设备状态，温度 40 秒回落" },
    ]);
  });

  it("splits think block and trailing verdict fence, order preserved", () => {
    const raw =
      '<think>让我分析一下历史报警…</think>温度 10°C 超限，door_open=true 是根因。```json {"verdict":"needs_human"}```';
    expect(segmentEvidence(raw)).toEqual([
      { kind: "think", text: "让我分析一下历史报警…" },
      { kind: "text", text: "温度 10°C 超限，door_open=true 是根因。" },
      { kind: "verdict", text: '{"verdict":"needs_human"}' },
    ]);
  });

  it("handles unclosed think block (输出截断)", () => {
    expect(segmentEvidence("结论。<think>分析到一半")).toEqual([
      { kind: "text", text: "结论。" },
      { kind: "think", text: "分析到一半" },
    ]);
  });

  it("returns no segments for empty input", () => {
    expect(segmentEvidence("")).toEqual([]);
    expect(segmentEvidence("  ")).toEqual([]);
  });
});
