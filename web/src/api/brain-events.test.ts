/**
 * brain-events API client 契约测试（X4）。
 *
 * 锁的是：端点路径/参数、camelCase 契约形状（learning: web-case-converter
 * 类型/运行时分叉教训）、unwrap 空响应报错、approve/reject 按 source 路由。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./client.js", () => ({
  apiGet: vi.fn(),
  apiPost: vi.fn(),
  getWorkspaceId: vi.fn(() => "ws-1"),
}));
vi.mock("./judgments.js", () => ({
  judgmentApi: { approve: vi.fn(), reject: vi.fn() },
}));

import { apiGet, apiPost } from "./client.js";
import { judgmentApi } from "./judgments.js";
import { brainEventApi, type BrainEvent } from "./brain-events.js";

const row: BrainEvent = {
  id: "alarm:j1",
  source: "alarm",
  alarmId: "a1",
  thingId: "t1",
  title: "温度越限",
  verdict: "self_healable",
  reason: "可重连",
  suggestedAction: "重连",
  actionCategory: "connection_recovery",
  risk: "medium",
  runId: "r1",
  ticketId: null,
  status: "awaiting_approval",
  triageMode: "annotate",
  createdAt: "2026-09-28T01:00:00Z",
  judgedAt: "2026-09-28T01:01:00Z",
  stateEnteredAt: "2026-09-28T01:01:00Z",
  resolvedAt: null,
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("brainEventApi.list", () => {
  it("默认 needs_you tab + limit 20，返回 camelCase 行", async () => {
    vi.mocked(apiGet).mockResolvedValue({ code: 0, msg: "ok", result: [row] });
    const out = await brainEventApi.list();
    expect(apiGet).toHaveBeenCalledWith("/brain-events?tab=needs_you&limit=20");
    expect(out).toHaveLength(1);
    // camelCase 契约：这些字段名是多下划线 snake 的高危区（web-case-converter 教训）
    expect(out[0].suggestedAction).toBe("重连");
    expect(out[0].actionCategory).toBe("connection_recovery");
    expect(out[0].stateEnteredAt).toBe("2026-09-28T01:01:00Z");
    expect(out[0].triageMode).toBe("annotate");
  });

  it("before 游标与 tab 透传", async () => {
    vi.mocked(apiGet).mockResolvedValue({ code: 0, msg: "ok", result: [] });
    await brainEventApi.list("patrol", "alarm:j0", 5);
    expect(apiGet).toHaveBeenCalledWith("/brain-events?tab=patrol&limit=5&before=alarm%3Aj0");
  });

  it("空响应抛错（不静默吞成 undefined）", async () => {
    vi.mocked(apiGet).mockResolvedValue({ code: 0, msg: "ok", result: null });
    await expect(brainEventApi.list()).rejects.toThrow("空响应");
  });
});

describe("brainEventApi.summary / detail", () => {
  it("summary 命中 /brain-events/summary", async () => {
    const summary = {
      digestedToday: 3,
      needsYou: 1,
      latencyP50Secs: 42.5,
      feedbackRight: 2,
      feedbackWrong: 1,
    };
    vi.mocked(apiGet).mockResolvedValue({ code: 0, msg: "ok", result: summary });
    const out = await brainEventApi.summary();
    expect(apiGet).toHaveBeenCalledWith("/brain-events/summary");
    expect(out.latencyP50Secs).toBe(42.5);
  });

  it("detail 按 id 取（含 evidence），id 含冒号正确转义", async () => {
    vi.mocked(apiGet).mockResolvedValue({ code: 0, msg: "ok", result: { ...row, evidence: { summary: "原文" } } });
    const out = await brainEventApi.detail("alarm:j1");
    expect(apiGet).toHaveBeenCalledWith("/brain-events/alarm%3Aj1");
    expect(out.evidence).toEqual({ summary: "原文" });
  });
});

describe("approve / reject 按 source 路由", () => {
  it("alarm 源走 judgmentApi", async () => {
    await brainEventApi.approve(row);
    expect(judgmentApi.approve).toHaveBeenCalledWith("j1");
    await brainEventApi.reject(row, "现在不能重启");
    expect(judgmentApi.reject).toHaveBeenCalledWith("j1", "现在不能重启");
  });

  it("patrol 源走 heartbeat approvals（reject 带原因 body，X2）", async () => {
    const patrol: BrainEvent = { ...row, id: "patrol:p-9", source: "patrol" };
    vi.mocked(apiPost).mockResolvedValue({ code: 0, msg: "ok", result: {} });
    await brainEventApi.approve(patrol);
    expect(apiPost).toHaveBeenCalledWith("/workspaces/ws-1/heartbeat/approvals/p-9/approve", {});
    await brainEventApi.reject(patrol, "误报，无需处理");
    expect(apiPost).toHaveBeenCalledWith("/workspaces/ws-1/heartbeat/approvals/p-9/reject", {
      reason: "误报，无需处理",
    });
  });
});
