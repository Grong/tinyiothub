/**
 * agent-runs trace API client 契约测试（AI 运行透明化 T11）。
 *
 * 锁的是：端点路径、camelCase 契约形状（budgetMax/entries/output:null）、
 * unwrap 空响应报错、404 → TraceNotFoundError、非 404 ApiError 原样透传。
 * 模式同 brain-events.test.ts（mock client.js，保留真实 ApiError 类）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./client.js", async (importOriginal) => {
  const orig = await importOriginal<typeof import("./client.js")>();
  return { ...orig, apiGet: vi.fn() };
});

import { apiGet, ApiError } from "./client.js";
import { fetchRunTrace, TraceNotFoundError, type RunTrace } from "./agent-runs.js";

const trace: RunTrace = {
  live: false,
  budgetMax: 12,
  entries: [
    { id: "c1", name: "read_property", args: { thingId: "t1" }, output: "{\"value\": 23}" },
    { id: "c2", name: "invoke_action", args: { thingId: "t1", action: "reboot" }, output: null },
  ],
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("fetchRunTrace", () => {
  it("命中 /agent-runs/{run_id}/trace，unwrap 后保留 camelCase 与 output:null", async () => {
    vi.mocked(apiGet).mockResolvedValue({ code: 0, msg: "ok", result: trace });
    const out = await fetchRunTrace("r-1");
    expect(apiGet).toHaveBeenCalledWith("/agent-runs/r-1/trace");
    expect(out.live).toBe(false);
    expect(out.budgetMax).toBe(12);
    expect(out.entries).toHaveLength(2);
    // output: null（未配对 ToolResult）必须保留为 null，不得吞成 undefined
    expect(out.entries[1].output).toBeNull();
  });

  it("run_id 特殊字符正确转义", async () => {
    vi.mocked(apiGet).mockResolvedValue({ code: 0, msg: "ok", result: trace });
    await fetchRunTrace("run:with/slash");
    expect(apiGet).toHaveBeenCalledWith("/agent-runs/run%3Awith%2Fslash/trace");
  });

  it("404 → TraceNotFoundError（区分「不存在」与「其他故障」）", async () => {
    vi.mocked(apiGet).mockRejectedValue(new ApiError(404, "run 不存在"));
    await expect(fetchRunTrace("ghost")).rejects.toBeInstanceOf(TraceNotFoundError);
  });

  it("非 404 的 ApiError 原样透传（不包成 NotFound）", async () => {
    const boom = new ApiError(500, "run report 数据损坏");
    vi.mocked(apiGet).mockRejectedValue(boom);
    await expect(fetchRunTrace("r-1")).rejects.toBe(boom);
  });

  it("空响应抛错（不静默吞成 undefined）", async () => {
    vi.mocked(apiGet).mockResolvedValue({ code: 0, msg: "ok", result: null });
    await expect(fetchRunTrace("r-1")).rejects.toThrow("空响应");
  });
});
