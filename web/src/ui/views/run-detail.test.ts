/**
 * run 详情页测试（AI 运行透明化 T13）。
 *
 * 锁的是：
 * - 层级：元信息条（状态/工具数/耗时）→ 时间线主视觉（full 模式）→ verdict 折叠 → think 折叠
 * - 三态：loading=骨架屏（3 行灰占位）/ 双源 404=「该运行记录不存在或已过期」+ 返回链接 /
 *   空 trace=「本次运行未调用工具」+ 元信息条（D4）
 * - 数据源：fetchRunTrace 404 → 退 brainEventApi.detail（trace + narrative + evidence）
 * - live 中 3s 轮询（代际守卫），翻 false 停止，卸载停轮询
 * - 导出 JSON：组装 { meta, entries } 写剪贴板（扩展 #8）
 * - S3 纯文本渲染铁律：trace/verdict/think 注入串不得变成 DOM
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "./run-detail.js";
import type { RunDetailView } from "./run-detail.js";
import { fetchRunTrace, TraceNotFoundError, type TraceEntry } from "../../api/agent-runs.js";
import { brainEventApi, type BrainEventDetail } from "../../api/brain-events.js";

vi.mock("../../api/agent-runs.js", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../api/agent-runs.js")>();
  return { ...orig, fetchRunTrace: vi.fn() };
});
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
vi.mock("../components/toast.js", () => ({ success: vi.fn(), error: vi.fn() }));

function entry(over: Partial<TraceEntry> = {}): TraceEntry {
  return { id: "c1", name: "read_property", args: { thing_id: "t1", property: "temperature" }, output: "23", ...over };
}

function detailStub(over: Partial<BrainEventDetail> = {}): BrainEventDetail {
  return {
    id: "alarm:j1",
    source: "alarm",
    alarmId: null,
    thingId: "t1",
    title: "温度异常调查",
    verdict: null,
    reason: "",
    suggestedAction: null,
    actionCategory: null,
    risk: null,
    runId: "run-1",
    tickId: null,
    ticketId: null,
    status: "resolved",
    triageMode: "auto",
    createdAt: "2026-10-08T10:00:00Z",
    judgedAt: "2026-10-08T10:01:30Z",
    stateEnteredAt: null,
    resolvedAt: null,
    evidence: null,
    trace: [],
    narrative: null,
    ...over,
  };
}

const mounted: HTMLElement[] = [];
beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
});

async function mountView(runId: string): Promise<RunDetailView> {
  const el = document.createElement("view-run-detail") as RunDetailView;
  el.runId = runId;
  document.body.appendChild(el);
  mounted.push(el);
  return el;
}

describe("run-detail · 层级与主数据源", () => {
  it("元信息条（已结束/工具数）→ full 时间线 → h1 标题 + 返回链接", async () => {
    vi.mocked(fetchRunTrace).mockResolvedValue({
      live: false,
      budgetMax: 12,
      entries: [entry({ id: "c1" }), entry({ id: "c2", name: "invoke_action", args: { action: "reboot" } })],
    });
    const el = await mountView("run-1");
    await vi.waitFor(() => expect(el.querySelector(".rd-meta")).toBeTruthy());

    // h1 = 标题 + 「← 返回」链接（D8）
    expect(el.querySelector("h1")).toBeTruthy();
    const back = el.querySelector<HTMLAnchorElement>(".rd-back");
    expect(back?.textContent).toContain("返回");

    // 元信息条：状态 + 工具数
    const meta = el.querySelector(".rd-meta")!;
    expect(meta.textContent).toContain("已结束");
    expect(meta.textContent).toContain("2");

    // 时间线主视觉：full 模式（每步 details 可展开）
    const timeline = el.querySelector("run-timeline");
    expect(timeline).toBeTruthy();
    await vi.waitFor(() => expect(timeline!.querySelectorAll("details.rt-step").length).toBe(2));
  });

  it("trace 端点 404 → 退 brain-event detail：时间线/narrative/verdict 折叠/think 折叠/耗时", async () => {
    vi.mocked(fetchRunTrace).mockRejectedValue(new TraceNotFoundError("patrol:p1"));
    vi.mocked(brainEventApi.detail).mockResolvedValue(
      detailStub({
        id: "patrol:p1",
        source: "patrol",
        trace: [entry({ id: "c1" })],
        narrative: "巡检结论：一切正常。",
        evidence: {
          summary: "正文第一段。<think>逐步推理过程</think>正文第二段。\n```json\n{\"verdict\":\"ok\"}\n```",
        },
      }),
    );
    const el = await mountView("patrol:p1");
    await vi.waitFor(() => expect(el.querySelector(".rd-meta")).toBeTruthy());

    // 时间线来自 detail.trace（完成态，无预算头）
    const timeline = el.querySelector("run-timeline");
    expect(timeline).toBeTruthy();
    expect(timeline!.querySelector(".rt-budget")).toBeNull();

    // narrative 透出
    expect(el.textContent).toContain("巡检结论：一切正常。");

    // verdict 原文折叠 + think 块折叠（D2），正文保留
    const folds = el.querySelectorAll("details.rd-fold");
    expect(folds.length).toBe(2);
    expect(folds[0].textContent).toContain("verdict 原文");
    expect(folds[0].textContent).toContain("{\"verdict\":\"ok\"}");
    expect(folds[1].textContent).toContain("思考过程");
    expect(folds[1].textContent).toContain("逐步推理过程");
    expect(el.textContent).toContain("正文第一段。");

    // 元信息条：耗时由 createdAt → judgedAt 推导（90 秒 = 1 分 30 秒）
    expect(el.querySelector(".rd-meta")!.textContent).toContain("1 分 30 秒");
  });
});

describe("run-detail · 三态", () => {
  it("loading = 骨架屏（3 行灰占位）", async () => {
    vi.mocked(fetchRunTrace).mockReturnValue(new Promise(() => {}));
    const el = await mountView("run-1");
    await el.updateComplete;
    const skeleton = el.querySelector(".rd-skeleton");
    expect(skeleton).toBeTruthy();
    expect(skeleton!.querySelectorAll(".skeleton-text").length).toBe(3);
  });

  it("双源 404 → 「该运行记录不存在或已过期」+ 返回链接", async () => {
    vi.mocked(fetchRunTrace).mockRejectedValue(new TraceNotFoundError("gone"));
    vi.mocked(brainEventApi.detail).mockRejectedValue(new Error("404"));
    const el = await mountView("gone");
    await vi.waitFor(() => expect(el.querySelector(".rd-notfound")).toBeTruthy());
    expect(el.querySelector(".rd-notfound")!.textContent).toContain("该运行记录不存在或已过期");
    const back = el.querySelector<HTMLAnchorElement>(".rd-notfound .rd-back");
    expect(back).toBeTruthy();
  });

  it("空 trace → 「本次运行未调用工具」+ 元信息条仍渲染（D4）", async () => {
    vi.mocked(fetchRunTrace).mockResolvedValue({ live: false, budgetMax: 12, entries: [] });
    const el = await mountView("run-1");
    await vi.waitFor(() => expect(el.querySelector(".rd-empty")).toBeTruthy());
    expect(el.querySelector(".rd-empty")!.textContent).toContain("本次运行未调用工具");
    expect(el.querySelector(".rd-meta")).toBeTruthy();
  });
});

describe("run-detail · live 轮询", () => {
  it("live:true 时 3s 轮询；翻 false 后停止", async () => {
    vi.useFakeTimers();
    try {
      vi.mocked(fetchRunTrace)
        .mockResolvedValueOnce({ live: true, budgetMax: 12, entries: [entry({ id: "c1", output: null })] })
        .mockResolvedValueOnce({ live: false, budgetMax: 12, entries: [entry({ id: "c1" }), entry({ id: "c2" })] });
      const el = await mountView("run-1");
      await vi.advanceTimersByTimeAsync(0);
      expect(fetchRunTrace).toHaveBeenCalledTimes(1);
      // live 态：元信息条「进行中」
      expect(el.querySelector(".rd-meta")!.textContent).toContain("进行中");

      await vi.advanceTimersByTimeAsync(3_000);
      expect(fetchRunTrace).toHaveBeenCalledTimes(2);
      // 翻 false：停止轮询，状态翻「已结束」
      expect(el.querySelector(".rd-meta")!.textContent).toContain("已结束");
      await vi.advanceTimersByTimeAsync(9_000);
      expect(fetchRunTrace).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("离开页面（卸载）停止轮询", async () => {
    vi.useFakeTimers();
    try {
      vi.mocked(fetchRunTrace).mockResolvedValue({ live: true, budgetMax: 12, entries: [] });
      const el = await mountView("run-1");
      await vi.advanceTimersByTimeAsync(0);
      expect(fetchRunTrace).toHaveBeenCalledTimes(1);
      el.remove();
      await vi.advanceTimersByTimeAsync(9_000);
      expect(fetchRunTrace).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("run-detail · 导出 JSON（扩展 #8）", () => {
  it("组装 { meta, entries } 写入剪贴板", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    vi.mocked(fetchRunTrace).mockResolvedValue({
      live: false,
      budgetMax: 12,
      entries: [entry({ id: "c1" }), entry({ id: "c2" })],
    });
    const el = await mountView("run-9");
    await vi.waitFor(() => expect(el.querySelector(".rd-export")).toBeTruthy());

    el.querySelector<HTMLButtonElement>(".rd-export")!.click();
    await vi.waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));

    const payload = JSON.parse(writeText.mock.calls[0][0] as string);
    expect(payload.meta.id).toBe("run-9");
    expect(payload.meta.toolCount).toBe(2);
    expect(payload.entries).toHaveLength(2);
    expect(payload.entries[0].name).toBe("read_property");
  });
});

describe("run-detail · S3 纯文本渲染铁律", () => {
  it("trace 参数中的注入串不变成 DOM", async () => {
    vi.mocked(fetchRunTrace).mockResolvedValue({
      live: false,
      budgetMax: 12,
      entries: [entry({ args: { q: "<img src=x onerror=alert(1)>" } })],
    });
    const el = await mountView("run-1");
    await vi.waitFor(() => expect(el.querySelector("run-timeline")).toBeTruthy());
    await vi.waitFor(() => expect(el.querySelector("run-timeline")!.querySelector("details.rt-step")).toBeTruthy());
    expect(el.querySelector("img")).toBeNull();
    expect(el.textContent).toContain("<img src=x onerror=alert(1)>");
  });

  it("evidence 中 think/verdict 的注入串一律文本节点", async () => {
    vi.mocked(fetchRunTrace).mockRejectedValue(new TraceNotFoundError("alarm:j1"));
    vi.mocked(brainEventApi.detail).mockResolvedValue(
      detailStub({
        evidence: {
          summary: "<think>思考 <script>alert(1)</script> 完毕</think>正文\n```json\n{\"v\":\"<img src=y>\"}\n```",
        },
      }),
    );
    const el = await mountView("alarm:j1");
    await vi.waitFor(() => expect(el.querySelector(".rd-meta")).toBeTruthy());
    expect(el.querySelector("script")).toBeNull();
    expect(el.querySelector("img")).toBeNull();
    expect(el.textContent).toContain("思考 <script>alert(1)</script> 完毕");
  });
});
