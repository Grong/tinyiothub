import { describe, expect, it } from "vitest";
import {
  STATE_TABS,
  availableActions,
  briefingSteps,
  failureKindLabel,
  hasSuggestions,
  stateBadgeClass,
  stateLabel,
  validateResolution,
} from "./tickets.js";

describe("tickets view helpers", () => {
  it("状态机迁移矩阵与后端一致", () => {
    expect(availableActions("open")).toEqual(["claim", "close"]);
    expect(availableActions("claimed")).toEqual(["start", "abandon"]);
    expect(availableActions("in_progress")).toEqual(["resolve"]);
    expect(availableActions("resolved")).toEqual(["close"]);
    expect(availableActions("closed")).toEqual([]);
  });

  it("状态徽标与标签齐全", () => {
    for (const s of ["open", "claimed", "in_progress", "resolved", "closed"] as const) {
      expect(stateLabel(s)).toBeTruthy();
      expect(stateBadgeClass(s)).toContain("ticket-badge");
    }
    // 待认领用 alarms 语义橙
    expect(stateBadgeClass("open")).toContain("--open");
  });

  it("解决必填校验", () => {
    expect(validateResolution("")).toContain("必填");
    expect(validateResolution("   ")).toContain("必填");
    expect(validateResolution("更换轴承 NSK-6205")).toBeNull();
  });

  it("failure_kind 标签映射", () => {
    expect(failureKindLabel("policy")).toBe("策略拒绝");
    expect(failureKindLabel("agent_unavailable")).toBe("Agent 不可用");
    expect(failureKindLabel(null)).toBe("未知");
    expect(failureKindLabel("future_kind")).toBe("future_kind");
  });

  it("简报部分字段缺失态：suggested 空则隐藏", () => {
    expect(hasSuggestions({ suggested_next_steps: [] })).toBe(false);
    expect(hasSuggestions({})).toBe(false);
    expect(hasSuggestions({ suggested_next_steps: ["现场检查"] })).toBe(true);
    expect(briefingSteps(undefined)).toEqual([]);
  });

  it("状态 tabs 含全部五个", () => {
    expect(STATE_TABS.map((t) => t.key)).toEqual(["", "open", "in_progress", "resolved", "closed"]);
  });
});
