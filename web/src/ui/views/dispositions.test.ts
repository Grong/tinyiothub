import { describe, expect, it } from "vitest";
import {
  approvalCountdown,
  fmtJudgmentTime,
  needsYou,
  validateWrongReason,
  verdictLabel,
} from "./dispositions.js";
import type { Judgment } from "../../api/judgments.js";

function j(over: Partial<Judgment>): Judgment {
  return {
    id: "j1",
    alarmId: null,
    thingId: "t1",
    ticketId: null,
    verdict: null,
    reason: "",
    evidence: null,
    suggestedAction: null,
    actionCategory: null,
    status: "investigating",
    latestFeedback: null,
    createdAt: new Date().toISOString(),
    judgedAt: null,
    resolvedAt: null,
    ...over,
  };
}

describe("dispositions view helpers", () => {
  it("「需要你」= 待审批 + 需人工（F3/F15①）", () => {
    expect(needsYou(j({ status: "awaiting_approval" }))).toBe(true);
    expect(needsYou(j({ status: "escalated" }))).toBe(true);
    expect(needsYou(j({ status: "investigating" }))).toBe(false);
    expect(needsYou(j({ status: "resolved" }))).toBe(false);
    expect(needsYou(j({ status: "noise_archived" }))).toBe(false);
  });

  it("verdict 标签：状态优先于 verdict（调查中/未调查），verdict 其次", () => {
    expect(verdictLabel(j({ status: "investigating" }))).toBe("调查中");
    expect(verdictLabel(j({ status: "budget_skipped" }))).toBe("未调查");
    expect(verdictLabel(j({ status: "noise_archived", verdict: "noise" }))).toBe("噪声");
    expect(verdictLabel(j({ status: "awaiting_approval", verdict: "self_healable" }))).toBe("可自愈");
    expect(verdictLabel(j({ status: "escalated", verdict: "needs_human" }))).toBe("需人工");
    expect(verdictLabel(j({ status: "investigation_failed" }))).toBe("调查失败");
  });

  it("时间戳规则（F12⑤）", () => {
    const now = new Date("2026-09-12T15:00:00").getTime();
    expect(fmtJudgmentTime(new Date(now - 30_000).toISOString(), now)).toBe("刚刚");
    expect(fmtJudgmentTime(new Date(now - 25 * 60_000).toISOString(), now)).toBe("25 分钟前");
    expect(fmtJudgmentTime(new Date(now - 3 * 3600_000).toISOString(), now)).toBe("12:00");
    expect(fmtJudgmentTime("2026-09-10T10:00:00", now)).toBe("9月10日");
  });

  it("审批倒计时（F6）：非待审批 → null；待审批 → 剩余小时", () => {
    expect(approvalCountdown(j({ status: "escalated" }))).toBeNull();
    expect(approvalCountdown(j({ status: "awaiting_approval" }))).toBeNull(); // 无 judgedAt
    const now = Date.now();
    const judged = j({
      status: "awaiting_approval",
      judgedAt: new Date(now - 6 * 3600_000).toISOString(),
    });
    expect(approvalCountdown(judged, now)).toBe("18 小时后自动转工单");
    // 超时后钳到 0
    const overdue = j({
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
