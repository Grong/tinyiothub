import { describe, expect, it, vi } from "vitest";
import {
  BADGE_MAP,
  evidenceRawText,
  expandMode,
  segmentEvidence,
  approvalCountdown,
  emptyStateCopy,
  fmtJudgmentTime,
  needsYou,
  nextExpandedId,
  tabCount,
  validateWrongReason,
} from "./dispositions.js";
import { brainEventApi, type BrainEvent, type BrainEventsSummary } from "../../api/brain-events.js";
import { fetchRunTrace, TraceNotFoundError, type TraceEntry } from "../../api/agent-runs.js";

// 组件级用例需要挂载真实元素：外围副作用（API/SSE/toast）全部 mock。
vi.mock("../../api/sse-client.js", () => ({
  connectSse: vi.fn(() => ({ close() {}, closed: true })),
}));
vi.mock("../../api/judgments.js", () => ({
  judgmentApi: { feedback: vi.fn(), approve: vi.fn(), reject: vi.fn() },
}));
vi.mock("../components/toast.js", () => ({ success: vi.fn(), error: vi.fn() }));
vi.mock("../../api/brain-events.js", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../api/brain-events.js")>();
  return {
    ...orig,
    brainEventApi: {
      list: vi.fn(),
      summary: vi.fn(),
      detail: vi.fn(),
      approve: vi.fn(),
      reject: vi.fn(),
    },
  };
});
vi.mock("../../api/agent-runs.js", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../api/agent-runs.js")>();
  return { ...orig, fetchRunTrace: vi.fn() };
});

function summaryStub(over: Partial<BrainEventsSummary> = {}): BrainEventsSummary {
  return {
    digestedToday: 1,
    needsYou: 0,
    latencyP50Secs: null,
    feedbackRight: 0,
    feedbackWrong: 0,
    ...over,
  };
}

function traceEntry(over: Partial<TraceEntry> = {}): TraceEntry {
  return { id: "c1", name: "read_property", args: { thing_id: "t1" }, output: "23", ...over };
}

/** 挂载视图并等首屏卡片出现（list/summary mock 需在调用前就位）。 */
async function mountView(events: BrainEvent[]): Promise<HTMLElement> {
  vi.mocked(brainEventApi.list).mockResolvedValue(events);
  vi.mocked(brainEventApi.summary).mockResolvedValue(summaryStub());
  const el = document.createElement("view-dispositions");
  document.body.appendChild(el);
  await vi.waitFor(() => expect(el.querySelector(".j-item")).toBeTruthy());
  return el;
}

function ev(over: Partial<BrainEvent>): BrainEvent {
  return {
    id: "alarm:j1",
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
    tickId: null,
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

describe("expandMode（T12：展开区统一为时间线的模式分派）", () => {
  it("investigating+runId → live；runId NULL → queue；终态 → detail；patrol_tick/patrol_ok → none", () => {
    expect(expandMode(ev({ status: "investigating", runId: "r1" }))).toBe("live");
    expect(expandMode(ev({ status: "investigating", runId: null }))).toBe("queue");
    expect(expandMode(ev({ status: "resolved", runId: "r1" }))).toBe("detail");
    expect(expandMode(ev({ status: "self_closed", runId: null }))).toBe("detail");
    // D10：patrol 提案卡片（有 tickId）走 detail.trace 内嵌完成时间线
    expect(expandMode(ev({ source: "patrol", status: "awaiting_approval", tickId: "t1" }))).toBe("detail");
    // tick 聚合行维持跳 #/ai-ops 不展开
    expect(expandMode(ev({ source: "patrol_tick", status: "patrol_ok" }))).toBe("none");
    expect(expandMode(ev({ status: "patrol_ok" }))).toBe("none");
  });
});

describe("组件级：investigating 卡片 live 时间线（T12）", () => {
  it("可展开：3s 轮询 + 预算头 n / budgetMax；收起即停轮询", async () => {
    vi.useFakeTimers();
    const el = await mountView([ev({ id: "alarm:i1", status: "investigating", runId: "run-1" })]);
    try {
      vi.mocked(fetchRunTrace).mockClear();
      vi.mocked(fetchRunTrace).mockResolvedValue({
        live: true,
        budgetMax: 12,
        summary: null,
        entries: [traceEntry({})],
      });
      const row = el.querySelector(".j-item") as HTMLElement;
      // investigating 卡片现在可点（T12 前被 canEvidence 排除）
      expect(row.getAttribute("tabindex")).toBe("0");
      row.click();
      await vi.waitFor(() => expect(fetchRunTrace).toHaveBeenCalledTimes(1));
      expect(vi.mocked(fetchRunTrace).mock.calls[0][0]).toBe("run-1");
      await vi.waitFor(() => expect(el.querySelector("run-timeline")).toBeTruthy());
      // 预算头（D11：中性色，不断言变色）
      expect(el.querySelector(".j-expand")?.textContent).toContain("1 / 12");
      // D5：展开区限高内滚由 CSS 承担（类名存在即可，布局不在 jsdom 断言）
      // 3s 轮询节奏
      await vi.advanceTimersByTimeAsync(3000);
      expect(fetchRunTrace).toHaveBeenCalledTimes(2);
      await vi.advanceTimersByTimeAsync(3000);
      expect(fetchRunTrace).toHaveBeenCalledTimes(3);
      // 收起 → 停止轮询
      row.click();
      await vi.waitFor(() => expect(el.querySelector("run-timeline")).toBeNull());
      await vi.advanceTimersByTimeAsync(9000);
      expect(fetchRunTrace).toHaveBeenCalledTimes(3);
    } finally {
      el.remove();
      vi.useRealTimers();
    }
  });

  it("代际守卫：快速切换展开卡，迟到的旧 run 响应不串数据", async () => {
    vi.useFakeTimers();
    const evA = ev({ id: "alarm:a", status: "investigating", runId: "run-a" });
    const evB = ev({ id: "alarm:b", status: "investigating", runId: "run-b" });
    const el = await mountView([evA, evB]);
    try {
      vi.mocked(fetchRunTrace).mockClear();
      let resolveA!: (t: { live: boolean; budgetMax: number; entries: TraceEntry[]; summary: string | null; }) => void;
      vi.mocked(fetchRunTrace).mockImplementation((runId: string) =>
        runId === "run-a"
          ? new Promise((r) => {
              resolveA = r;
            })
          : Promise.resolve({
              live: true,
              budgetMax: 12,
              summary: null,
              entries: [traceEntry({ id: "cb", name: "read_property" })],
            }),
      );
      const rows = el.querySelectorAll(".j-item");
      (rows[0] as HTMLElement).click(); // 展开 A（请求挂起）
      await vi.waitFor(() => expect(fetchRunTrace).toHaveBeenCalledTimes(1));
      (rows[1] as HTMLElement).click(); // 切到 B
      await vi.waitFor(() =>
        expect(el.querySelector("run-timeline")?.textContent).toContain("读取设备属性"),
      );
      // A 的迟到响应：不得覆盖 B 的时间线
      resolveA({
        live: true,
        budgetMax: 12,
        summary: null,
        entries: [traceEntry({ id: "ca", name: "invoke_action", output: null })],
      });
      await vi.advanceTimersByTimeAsync(0);
      await Promise.resolve();
      expect(el.querySelector("run-timeline")?.textContent).not.toContain("执行处置动作");
      expect(el.querySelector("run-timeline")?.textContent).toContain("读取设备属性");
    } finally {
      el.remove();
      vi.useRealTimers();
    }
  });

  it("404 有限重试 3 次（3s 间隔）后显示「轨迹不可用」且不再请求", async () => {
    vi.useFakeTimers();
    const el = await mountView([ev({ id: "alarm:nf", status: "investigating", runId: "run-x" })]);
    try {
      vi.mocked(fetchRunTrace).mockClear();
      vi.mocked(fetchRunTrace).mockRejectedValue(new TraceNotFoundError("run-x"));
      (el.querySelector(".j-item") as HTMLElement).click();
      await vi.waitFor(() => expect(fetchRunTrace).toHaveBeenCalledTimes(1));
      await vi.advanceTimersByTimeAsync(3000); // 重试 1
      expect(fetchRunTrace).toHaveBeenCalledTimes(2);
      await vi.advanceTimersByTimeAsync(3000); // 重试 2
      expect(fetchRunTrace).toHaveBeenCalledTimes(3);
      await vi.advanceTimersByTimeAsync(3000); // 重试 3 → 用尽
      await vi.waitFor(() => expect(el.textContent).toContain("轨迹不可用"));
      expect(fetchRunTrace).toHaveBeenCalledTimes(4); // 首次 + 3 次重试
      await vi.advanceTimersByTimeAsync(12000); // 不再请求
      expect(fetchRunTrace).toHaveBeenCalledTimes(4);
    } finally {
      el.remove();
      vi.useRealTimers();
    }
  });

  it("runId NULL 的 investigating 卡片显示「排队中」，无展开入口、不发请求", async () => {
    vi.mocked(fetchRunTrace).mockClear();
    vi.mocked(brainEventApi.detail).mockClear();
    const el = await mountView([ev({ id: "alarm:q1", status: "investigating", runId: null })]);
    try {
      const row = el.querySelector(".j-item") as HTMLElement;
      expect(row.getAttribute("tabindex")).toBeNull();
      expect(el.textContent).toContain("排队中");
      row.click();
      await new Promise((r) => setTimeout(r, 20));
      expect(el.querySelector(".j-expand")).toBeNull();
      expect(fetchRunTrace).not.toHaveBeenCalled();
      expect(brainEventApi.detail).not.toHaveBeenCalled();
    } finally {
      el.remove();
    }
  });

  it("aria-live：只在步数变化时更新；run 完成播报「分析完成」一次并停轮询（D9）", async () => {
    vi.useFakeTimers();
    const el = await mountView([ev({ id: "alarm:a1", status: "investigating", runId: "run-1" })]);
    try {
      vi.mocked(fetchRunTrace).mockClear();
      let call = 0;
      vi.mocked(fetchRunTrace).mockImplementation(() => {
        call += 1;
        return Promise.resolve(
          call === 1
            ? { live: true, budgetMax: 12, entries: [traceEntry({ id: "c1" })], summary: null }
            : {
                live: false,
                budgetMax: 12,
                summary: null,
                entries: [traceEntry({ id: "c1" }), traceEntry({ id: "c2", output: null })],
              },
        );
      });
      (el.querySelector(".j-item") as HTMLElement).click();
      await vi.waitFor(() =>
        expect(el.querySelector("[aria-live='polite']")?.textContent).toContain("已完成 1 步"),
      );
      await vi.advanceTimersByTimeAsync(3000);
      await vi.waitFor(() =>
        expect(el.querySelector("[aria-live='polite']")?.textContent).toContain("分析完成"),
      );
      // 完成即停轮询
      await vi.advanceTimersByTimeAsync(9000);
      expect(fetchRunTrace).toHaveBeenCalledTimes(2);
      // 完成后同位置转完成态时间线：未配对步 ✗ 而非 spinner
      expect(el.querySelector(".rt-spinner")).toBeNull();
      expect(el.querySelector(".rt-mark-failed")).toBeTruthy();
    } finally {
      el.remove();
      vi.useRealTimers();
    }
  });

  it("整表刷新翻终态（reconcileExpanded live→detail）：停 live 轮询、改取 detail、换完成时间线", async () => {
    vi.useFakeTimers();
    const investigating = ev({ id: "alarm:sw", status: "investigating", runId: "run-sw" });
    const resolvedRow = ev({ id: "alarm:sw", status: "resolved", runId: "run-sw", reason: "已闭环" });
    // 首屏 investigating；60s 整表轮询刷新后同一事件翻终态（SSE 路径同走 load(true)）
    vi.mocked(brainEventApi.list)
      .mockResolvedValueOnce([investigating])
      .mockResolvedValue([resolvedRow]);
    vi.mocked(brainEventApi.summary).mockResolvedValue(summaryStub());
    const el = document.createElement("view-dispositions");
    document.body.appendChild(el);
    try {
      await vi.waitFor(() => expect(el.querySelector(".j-item")).toBeTruthy());
      vi.mocked(fetchRunTrace).mockClear();
      vi.mocked(fetchRunTrace).mockResolvedValue({
        live: true,
        budgetMax: 12,
        summary: null,
        entries: [traceEntry({})],
      });
      vi.mocked(brainEventApi.detail).mockClear();
      vi.mocked(brainEventApi.detail).mockResolvedValue({
        ...resolvedRow,
        evidence: {},
        trace: [traceEntry({ id: "cd" })],
        narrative: "刷新后叙述",
      });
      (el.querySelector(".j-item") as HTMLElement).click();
      await vi.waitFor(() => expect(fetchRunTrace).toHaveBeenCalledTimes(1));

      // 60s 整表刷新：列表返回终态行 → reconcileExpanded 停轮询 + 取 detail
      await vi.advanceTimersByTimeAsync(60_000);
      await vi.waitFor(() => expect(brainEventApi.detail).toHaveBeenCalledWith("alarm:sw"));
      // live 轮询已停：继续推进时间不再打 trace 端点
      const traceCalls = vi.mocked(fetchRunTrace).mock.calls.length;
      await vi.advanceTimersByTimeAsync(12_000);
      expect(fetchRunTrace).toHaveBeenCalledTimes(traceCalls);
      // 展开区换成完成态：narrative 来自 detail（不再渲染 live 快照）
      await vi.waitFor(() =>
        expect(el.querySelector(".j-narrative")?.textContent).toContain("刷新后叙述"),
      );
    } finally {
      el.remove();
      vi.useRealTimers();
    }
  });

  it("404 用尽后翻终态：reconcileExpanded 仍切 detail（终审 I-2——liveTraceFailed 也触发换道）", async () => {
    vi.useFakeTimers();
    const investigating = ev({ id: "alarm:fp", status: "investigating", runId: "run-fp" });
    const resolvedRow = ev({ id: "alarm:fp", status: "resolved", runId: "run-fp", reason: "已闭环" });
    vi.mocked(brainEventApi.list)
      .mockResolvedValueOnce([investigating])
      .mockResolvedValue([resolvedRow]);
    vi.mocked(brainEventApi.summary).mockResolvedValue(summaryStub());
    const el = document.createElement("view-dispositions");
    document.body.appendChild(el);
    try {
      await vi.waitFor(() => expect(el.querySelector(".j-item")).toBeTruthy());
      // trace 端点全程 404（registry 注销与 report 落库间的异步窗口）
      vi.mocked(fetchRunTrace).mockClear();
      vi.mocked(fetchRunTrace).mockRejectedValue(new TraceNotFoundError("run-fp"));
      vi.mocked(brainEventApi.detail).mockClear();
      vi.mocked(brainEventApi.detail).mockResolvedValue({
        ...resolvedRow,
        evidence: {},
        trace: [traceEntry({ id: "cd" })],
        narrative: "窗口期后的叙述",
      });
      (el.querySelector(".j-item") as HTMLElement).click();
      await vi.waitFor(() => expect(fetchRunTrace).toHaveBeenCalledTimes(1));
      // 3 次重试用尽 → 「轨迹不可用」
      await vi.advanceTimersByTimeAsync(9000);
      await vi.waitFor(() => expect(el.textContent).toContain("轨迹不可用"));

      // 此刻整表刷新把卡片翻终态——I-2 前：三条件全空不切 detail，永卡「加载中…」
      await vi.advanceTimersByTimeAsync(60_000);
      await vi.waitFor(() => expect(brainEventApi.detail).toHaveBeenCalledWith("alarm:fp"));
      await vi.waitFor(() =>
        expect(el.querySelector(".j-narrative")?.textContent).toContain("窗口期后的叙述"),
      );
      expect(el.textContent).not.toContain("加载中…");
    } finally {
      el.remove();
      vi.useRealTimers();
    }
  });
});

describe("组件级：完成态卡片展开区（T12：时间线 + narrative + 完整轨迹链接）", () => {
  it("完成时间线 + narrative + 「查看完整轨迹」链接；不走 trace 轮询端点", async () => {
    vi.mocked(fetchRunTrace).mockClear();
    vi.mocked(brainEventApi.detail).mockClear();
    const evR = ev({ id: "alarm:r1", status: "resolved", runId: "run-9", reason: "温度超限" });
    vi.mocked(brainEventApi.detail).mockResolvedValue({
      ...evR,
      evidence: {},
      trace: [traceEntry({})],
      narrative: "AI 已确认温度回落并闭环",
    });
    const el = await mountView([evR]);
    try {
      (el.querySelector(".j-item") as HTMLElement).click();
      await vi.waitFor(() => expect(el.querySelector("run-timeline")).toBeTruthy());
      expect(fetchRunTrace).not.toHaveBeenCalled(); // 完成态只走 detail
      expect(el.querySelector(".j-narrative")?.textContent).toContain("AI 已确认温度回落并闭环");
      const link = el.querySelector(".j-trace-link") as HTMLAnchorElement;
      expect(link.getAttribute("href")).toBe("#/ai-ops/runs/run-9");
      expect(link.textContent).toContain("查看完整轨迹");
    } finally {
      el.remove();
    }
  });

  it("narrative 缺失降级为 reason", async () => {
    vi.mocked(brainEventApi.detail).mockClear();
    const evR = ev({ id: "alarm:r2", status: "self_closed", runId: "run-2", reason: "温度回落自行闭环" });
    vi.mocked(brainEventApi.detail).mockResolvedValue({
      ...evR,
      evidence: {},
      trace: [traceEntry({})],
      narrative: null,
    });
    const el = await mountView([evR]);
    try {
      (el.querySelector(".j-item") as HTMLElement).click();
      await vi.waitFor(() =>
        expect(el.querySelector(".j-narrative")?.textContent).toContain("温度回落自行闭环"),
      );
    } finally {
      el.remove();
    }
  });

  it("D10：patrol 提案卡片点击展开内嵌完成时间线（detail.trace），tick 聚合行维持跳 #/ai-ops", async () => {
    window.location.hash = "";
    vi.mocked(fetchRunTrace).mockClear();
    vi.mocked(brainEventApi.detail).mockClear();
    const prop = ev({
      id: "patrol:p1",
      source: "patrol",
      status: "awaiting_approval",
      tickId: "tick-1",
      runId: null,
      reason: "建议重启传感器",
    });
    const tickRow = ev({ id: "patrol_tick:t1", source: "patrol_tick", status: "patrol_ok" });
    vi.mocked(brainEventApi.detail).mockResolvedValue({
      ...prop,
      evidence: {},
      trace: [traceEntry({})],
      narrative: null,
    });
    const el = await mountView([prop, tickRow]);
    try {
      const rows = el.querySelectorAll(".j-item");
      // 提案行：点击展开内嵌时间线，不跳转
      (rows[0] as HTMLElement).click();
      await vi.waitFor(() => expect(el.querySelector("run-timeline")).toBeTruthy());
      expect(brainEventApi.detail).toHaveBeenCalledTimes(1);
      expect(fetchRunTrace).not.toHaveBeenCalled();
      expect(window.location.hash).toBe("");
      // runId NULL 的提案：完整轨迹链接用 brain event id（T13 detail 兜底 /brain-events/{id}）
      const link = el.querySelector(".j-trace-link") as HTMLAnchorElement;
      expect(link.getAttribute("href")).toBe("#/ai-ops/runs/patrol:p1");
      // tick 聚合行：跳巡检历史，不展开
      (rows[1] as HTMLElement).click();
      expect(window.location.hash).toBe("#/ai-ops");
      expect(brainEventApi.detail).toHaveBeenCalledTimes(1);
    } finally {
      el.remove();
      window.location.hash = "";
    }
  });

  it("Esc 收起展开区并把焦点交还卡片行（D8）", async () => {
    const evR = ev({ id: "alarm:e1", status: "resolved", runId: "run-1", reason: "已闭环" });
    vi.mocked(brainEventApi.detail).mockResolvedValue({
      ...evR,
      evidence: {},
      trace: [],
      narrative: "叙述",
    });
    const el = await mountView([evR]);
    try {
      const row = el.querySelector(".j-item") as HTMLElement;
      row.click();
      await vi.waitFor(() => expect(el.querySelector(".j-expand")).toBeTruthy());
      row.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      await vi.waitFor(() => expect(el.querySelector(".j-expand")).toBeNull());
      expect(document.activeElement).toBe(row);
    } finally {
      el.remove();
    }
  });
});

describe("组件级：展开区加载失败重试（「重试」不得收起面板）", () => {
  it("detail 失败内联重试，成功后渲染完成时间线", async () => {
    const ev1 = ev({ id: "alarm:j1", status: "resolved", runId: "run-1" });
    vi.mocked(brainEventApi.list).mockResolvedValue([ev1]);
    vi.mocked(brainEventApi.summary).mockResolvedValue(summaryStub());
    // 第一次展开：detail 失败 → 内联「加载失败，重试」
    vi.mocked(brainEventApi.detail).mockRejectedValueOnce(new Error("boom"));

    const el = document.createElement("view-dispositions");
    document.body.appendChild(el);
    try {
      await vi.waitFor(() => expect(el.querySelector(".j-item")).toBeTruthy());
      (el.querySelector(".j-item") as HTMLElement).click();
      await vi.waitFor(() => expect(el.querySelector(".j-expand-error")).toBeTruthy());

      // 重试：detail 成功 → 面板保持展开并渲染时间线（不再收起）
      vi.mocked(brainEventApi.detail).mockResolvedValueOnce({
        ...ev1,
        evidence: {},
        trace: [traceEntry({})],
        narrative: null,
      });
      (el.querySelector(".j-expand-error button") as HTMLElement).click();
      await vi.waitFor(() => expect(el.querySelector("run-timeline")).toBeTruthy());
      expect(el.querySelector(".j-expand-error")).toBeNull();
    } finally {
      el.remove();
    }
  });
});

describe("组件级：键盘可达（X7——Enter 展开/收起，行可聚焦）", () => {
  it("可点行带 tabindex/role，Enter 键展开时间线", async () => {
    const ev1 = ev({ id: "alarm:j-kbd", status: "resolved", runId: "run-k" });
    vi.mocked(brainEventApi.list).mockResolvedValue([ev1]);
    vi.mocked(brainEventApi.summary).mockResolvedValue(summaryStub());
    vi.mocked(brainEventApi.detail).mockResolvedValue({
      ...ev1,
      evidence: {},
      trace: [traceEntry({})],
      narrative: "键盘可达",
    });
    // 本文件的 API mock 跨测试共享计数——清零后断言相对增量
    vi.mocked(brainEventApi.detail).mockClear();

    const el = document.createElement("view-dispositions");
    document.body.appendChild(el);
    try {
      await vi.waitFor(() => expect(el.querySelector(".j-item")).toBeTruthy());
      const row = el.querySelector(".j-item") as HTMLElement;
      expect(row.getAttribute("tabindex")).toBe("0");
      // 不设 role=button（嵌套交互元素的 WCAG 4.1.2 约束，见 dispositions.ts 注释）
      expect(row.getAttribute("role")).toBeNull();

      // Enter 展开（与点击同语义）
      row.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      await vi.waitFor(() => expect(el.querySelector("run-timeline")).toBeTruthy());
      expect(brainEventApi.detail).toHaveBeenCalledTimes(1);

      // 再按 Enter 收起
      row.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      await vi.waitFor(() => expect(el.querySelector("run-timeline")).toBeNull());
    } finally {
      el.remove();
    }
  });
});
