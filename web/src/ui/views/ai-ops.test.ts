/**
 * ai-ops 执行历史测试（investigate 断口修复：trace 透出到执行历史）。
 *
 * 锁的是：logs 响应含 trace 时，执行历史详情内嵌 run-timeline（full 模式）；
 * 无 trace（旧 tick）时无时间线、无空壳。
 */
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/client.js", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../api/client.js")>();
  return {
    ...orig,
    apiGet: vi.fn(),
    apiPost: vi.fn(),
    apiPut: vi.fn(),
    getWorkspaceId: vi.fn(() => "ws1"),
  };
});
vi.mock("../../api/sse-client.js", () => ({
  connectSse: vi.fn(() => ({ close() {}, closed: true })),
}));

import { apiGet } from "../../api/client.js";
import "./ai-ops.js";
import "../shared/run-timeline.js";

const tickLog = {
  timestamp: "2026-10-12 10:00:00",
  taskCount: 2,
  status: "success",
  result: "巡检完成，温度正常",
  autoExecuted: [],
  pendingProposals: [],
  trace: [
    { id: "c1", name: "read_property", args: { thingId: "t1" }, output: "26.5" },
    { id: "c2", name: "query_events", args: {}, output: null },
  ],
};

const tickLogNoTrace = { ...tickLog, trace: [] };

function mockApis(logs: unknown[]) {
  vi.mocked(apiGet).mockImplementation((url: string) => {
    // workspace 列表从 /agents 派生（loadWorkspaces → selectedWsId → loadAll）
    if (url === "/agents")
      return Promise.resolve({ code: 0, msg: "", result: { agents: [{ id: "a1", name: "运维", workspaceId: "ws1" }] } } as never);
    if (url.includes("/heartbeat/logs")) return Promise.resolve({ code: 0, msg: "", result: { logs } } as never);
    if (url.includes("/heartbeat/approvals")) return Promise.resolve({ code: 0, msg: "", result: { proposals: [] } } as never);
    if (url.includes("/heartbeat/config")) return Promise.resolve({ code: 0, msg: "", result: null } as never);
    return Promise.resolve({ code: 0, msg: "", result: null } as never);
  });
}

const mounted: HTMLElement[] = [];
afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
});

async function mountView(): Promise<HTMLElement> {
  const el = document.createElement("view-ai-ops");
  document.body.appendChild(el);
  mounted.push(el);
  await vi.waitFor(() => expect(el.querySelector(".ao-timeline__item")).toBeTruthy());
  return el;
}

describe("ai-ops 执行历史 trace 透出", () => {
  it("有 trace 的 tick：详情内嵌 run-timeline（full 模式），逐步可见", async () => {
    mockApis([tickLog]);
    const el = await mountView();
    // 展开详情
    (el.querySelector(".ao-timeline__summary") as HTMLElement).click();
    await vi.waitFor(() => expect(el.querySelector("run-timeline")).toBeTruthy());
    expect(el.querySelector("run-timeline")!.textContent).toContain("读取设备属性");
  });

  it("无 trace 的旧 tick：无时间线、无空壳", async () => {
    mockApis([tickLogNoTrace]);
    const el = await mountView();
    (el.querySelector(".ao-timeline__summary") as HTMLElement).click();
    await new Promise((r) => setTimeout(r, 20));
    expect(el.querySelector("run-timeline")).toBeNull();
    expect(el.textContent).toContain("巡检完成");
  });
});
