/**
 * run-timeline 组件测试（AI 运行透明化 T11）。
 *
 * 锁的是：
 * - 双模式渲染（live=精简行 / full=可展开 JSON）
 * - 步状态：done ✓ / 进行中 spinner（output:null 且 live）/ 未配对 ✗（output:null 且 !live）
 * - 预算头 "n / budgetMax"，n 略超 budgetMax 不截断
 * - 工具名中文映射（以 crates/agent 工具目录为准）
 * - S3 安全裁定：trace 内容一律纯文本节点，注入串不得变成 DOM
 */
import { afterEach, describe, expect, it } from "vitest";
import "./run-timeline.js";
import type { RunTimeline } from "./run-timeline.js";
import type { TraceEntry } from "../../api/agent-runs.js";

function entry(over: Partial<TraceEntry>): TraceEntry {
  return { id: "c1", name: "read_property", args: { thing_id: "t1", property: "temperature" }, output: "23", ...over };
}

async function mount(opts: {
  entries: TraceEntry[];
  live?: boolean;
  budgetMax?: number;
  mode?: "live" | "full";
}): Promise<RunTimeline> {
  const el = document.createElement("run-timeline") as RunTimeline;
  el.entries = opts.entries;
  el.live = opts.live ?? false;
  el.budgetMax = opts.budgetMax ?? 12;
  el.mode = opts.mode ?? "live";
  document.body.appendChild(el);
  await el.updateComplete;
  return el;
}

const mounted: HTMLElement[] = [];
afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
});
async function mountTracked(opts: Parameters<typeof mount>[0]): Promise<RunTimeline> {
  const el = await mount(opts);
  mounted.push(el);
  return el;
}

describe("run-timeline · live 模式（卡片精简行）", () => {
  it("渲染工具中文名 + 参数摘要；完成步 ✓、进行中步 spinner（无脉冲点）", async () => {
    const el = await mountTracked({
      mode: "live",
      live: true,
      entries: [
        entry({ id: "c1", name: "read_property" }),
        entry({ id: "c2", name: "invoke_action", args: { thing_id: "t1", action: "reboot" }, output: null }),
      ],
    });
    const rows = el.querySelectorAll(".rt-step");
    expect(rows).toHaveLength(2);

    // 中文映射（简报逐字：read_property=读取设备属性、invoke_action=执行处置动作）
    expect(rows[0].textContent).toContain("读取设备属性");
    expect(rows[1].textContent).toContain("执行处置动作");
    // 参数摘要出现在精简行
    expect(rows[0].textContent).toContain("temperature");

    // 完成步 ✓，进行中步 spinner（output:null 且 live）
    expect(rows[0].querySelector(".rt-mark-done")).toBeTruthy();
    expect(rows[1].querySelector(".rt-spinner")).toBeTruthy();
    // live 精简行不可展开（无 details）
    expect(el.querySelector("details")).toBeNull();
  });

  it("进行中步 spinner 有动画类但无脉冲点（评审裁定：禁脉冲点）", async () => {
    const el = await mountTracked({ mode: "live", live: true, entries: [entry({ output: null })] });
    expect(el.querySelector(".rt-spinner")).toBeTruthy();
    expect(el.querySelector(".rt-pulse")).toBeNull();
  });
});

describe("run-timeline · full 模式（详情页）", () => {
  it("每步可展开（details），含参数/输出 JSON 与元信息；✓/✗ 标记", async () => {
    const el = await mountTracked({
      mode: "full",
      live: false,
      entries: [
        entry({ id: "c1", name: "query_events", output: "[{\"e\":1}]" }),
        entry({ id: "c2", name: "invoke_action", output: null }),
      ],
    });
    const steps = el.querySelectorAll("details.rt-step");
    expect(steps).toHaveLength(2);

    // 完成步 ✓；output:null 且 run 已结束 = 未配对 ToolResult → ✗
    expect(steps[0].querySelector(".rt-mark-done")).toBeTruthy();
    expect(steps[1].querySelector(".rt-mark-failed")).toBeTruthy();

    // 展开后可见参数/输出 JSON 与元信息（原始工具名 + entry id）
    (steps[0] as HTMLDetailsElement).open = true;
    expect(steps[0].textContent).toContain("query_events");
    expect(steps[0].textContent).toContain("c1");
    expect(steps[0].querySelector(".rt-json")).toBeTruthy();
  });
});

describe("run-timeline · 预算头", () => {
  it('显示 "n / budgetMax"', async () => {
    const el = await mountTracked({ entries: [entry({}), entry({ id: "c2" })], budgetMax: 5 });
    expect(el.querySelector(".rt-budget")?.textContent?.trim()).toBe("2 / 5");
  });

  it("n 略超 budgetMax 不截断（预算超跑也要如实显示）", async () => {
    const entries = Array.from({ length: 6 }, (_, i) => entry({ id: `c${i}` }));
    const el = await mountTracked({ entries, budgetMax: 5 });
    expect(el.querySelector(".rt-budget")?.textContent?.trim()).toBe("6 / 5");
    // 步骤也不截断
    expect(el.querySelectorAll(".rt-step")).toHaveLength(6);
  });
});

describe("run-timeline · S3 安全裁定：纯文本渲染", () => {
  it("args/output 中的注入串渲染为文本，不产生 DOM 节点", async () => {
    const evil = "<img src=x onerror=alert(1)>";
    const el = await mountTracked({
      mode: "full",
      entries: [entry({ name: `read_property${evil}`, args: { payload: evil }, output: evil })],
    });
    expect(el.querySelector("img")).toBeNull();
    // 内容仍以纯文本形式可见
    expect(el.textContent).toContain(evil);
  });
});

describe("run-timeline · 工具名中文映射", () => {
  it("覆盖 thing-agent 实际工具；未知工具回退原始名", async () => {
    const el = await mountTracked({
      mode: "live",
      entries: [
        entry({ id: "c1", name: "read_property" }),
        entry({ id: "c2", name: "query_events" }),
        entry({ id: "c3", name: "invoke_action" }),
        entry({ id: "c4", name: "some_unknown_tool" }),
      ],
    });
    const text = el.textContent ?? "";
    expect(text).toContain("读取设备属性");
    expect(text).toContain("查询历史事件");
    expect(text).toContain("执行处置动作");
    expect(text).toContain("some_unknown_tool");
  });
});
