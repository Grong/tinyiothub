/**
 * 物详情页属性 Tab — 派生属性展示测试（园区物派生属性 Task 10）。
 *
 * 锁的是：
 * - source='derived' 行尾显示「派生」徽标（普通属性行不显示）
 * - derivedStatus.stale=true → 灰点 + 文字"等待数据"
 *   （D11 裁决：状态点必须配文字标签，非纯色编码）
 * - 表达式只读展开：<details> 内表达式原文 + 引用链接 href=#/things/{thing_id}
 * - staleReason 含"引用缺失" → 原因文本整体展示
 *   （本期简化：不逐引用标红划线，只在 stale_reason 展示原因）
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "lit";
import { renderDetailProperties } from "./things-detail.js";
import type { DevicesView } from "./things.js";
import type { ThingProfile, ThingProperty } from "../../types/index.js";
import { thingApi } from "../../api/things.js";

vi.mock("../../api/things.js", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../api/things.js")>();
  return {
    ...orig,
    thingApi: { ...orig.thingApi, updatePropertyDef: vi.fn() },
  };
});
vi.mock("../components/toast.js", () => ({ success: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() }));

const mounted: HTMLElement[] = [];
beforeEach(() => vi.clearAllMocks());
afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
});

function prop(over: Record<string, unknown> = {}): ThingProperty {
  return {
    id: "p1",
    thingId: "t1",
    name: "power",
    displayName: "功率",
    value: null,
    currentValue: "42",
    dataType: "float",
    unit: "kW",
    isReadOnly: true,
    updatedAt: "2026-10-19T10:00:00",
    ...over,
  } as ThingProperty;
}

function profile(properties: ThingProperty[]): ThingProfile {
  return {
    thing: { id: "t1", name: "meter1", displayName: "电表1" } as ThingProfile["thing"],
    isOnline: true,
    properties,
    commands: [],
    recentEvents: [],
    overview: {
      totalProperties: properties.length,
      onlineProperties: 0,
      offlineProperties: 0,
      readonlyProperties: 0,
      writableProperties: 0,
      totalCommands: 0,
      totalEvents: 0,
      activeAlarms: 0,
    },
    generatedAt: "2026-10-19T10:00:00",
  };
}

function hostStub(p: ThingProfile): DevicesView {
  return {
    selectedDevice: p,
    isNumericType: () => false,
    openPropertyHistory: vi.fn(),
    requestUpdate: vi.fn(),
    loadDeviceDetail: vi.fn(),
  } as unknown as DevicesView;
}

function mount(host: DevicesView): HTMLElement {
  const el = document.createElement("div");
  document.body.appendChild(el);
  mounted.push(el);
  render(renderDetailProperties(host), el);
  return el;
}

describe("物详情属性 Tab — 派生展示", () => {
  it("source=derived 行显示「派生」徽标，普通行不显示", () => {
    const el = mount(hostStub(profile([
      prop({ source: "derived", expression: "{meter2.voltage} * 2", refs: "[]" }),
      prop({ id: "p2", name: "temp", displayName: "温度" }),
    ])));
    const badges = el.querySelectorAll(".prop-derived-badge");
    expect(badges.length).toBe(1);
    expect(badges[0].textContent).toContain("派生");
  });

  it("stale=true → 灰点 + 文字「等待数据」（非纯色编码）", () => {
    const el = mount(hostStub(profile([
      prop({
        source: "derived",
        expression: "{meter2.voltage} * 2",
        refs: "[]",
        derivedStatus: { stale: true, staleReason: null, lastEvalAt: null },
      }),
    ])));
    expect(el.querySelector(".status-dot--stale")).toBeTruthy();
    expect(el.textContent).toContain("等待数据");
  });

  it("stale=false/无 derivedStatus → 无灰点无等待文字", () => {
    const el = mount(hostStub(profile([
      prop({
        source: "derived",
        expression: "{meter2.voltage} * 2",
        refs: "[]",
        derivedStatus: { stale: false, staleReason: null, lastEvalAt: "2026-10-19T10:00:00" },
      }),
    ])));
    expect(el.querySelector(".status-dot--stale")).toBeNull();
    expect(el.textContent).not.toContain("等待数据");
  });

  it("表达式只读展开：details 内表达式原文 + 引用链接 #/things/{id}", () => {
    const refs = JSON.stringify([{ thing_id: "t-ref-9", property_name: "voltage" }]);
    const el = mount(hostStub(profile([
      prop({ source: "derived", expression: "{meter2.voltage} * 2", refs }),
    ])));
    const details = el.querySelector("details.prop-derived-details");
    expect(details).toBeTruthy();
    expect(details!.textContent).toContain("{meter2.voltage} * 2");
    const link = details!.querySelector("a.prop-ref-link");
    expect(link).toBeTruthy();
    expect(link!.getAttribute("href")).toBe("#/things/t-ref-9");
    expect(details!.textContent).toContain("voltage");
  });

  it("staleReason 含「引用缺失」→ 原因文本展示（简化：不逐引用标红）", () => {
    const el = mount(hostStub(profile([
      prop({
        source: "derived",
        expression: "{gone.voltage} * 2",
        refs: "[]",
        derivedStatus: { stale: true, staleReason: "引用缺失：电表2.电压", lastEvalAt: null },
      }),
    ])));
    expect(el.querySelector(".status-dot--stale")).toBeTruthy();
    expect(el.textContent).toContain("引用缺失：电表2.电压");
  });
});

describe("物详情属性 Tab — 派生状态卡与行动作（Task 5）", () => {
  it("状态卡：N 派生 / M 等待数据 / 最近计算；M>0 可展开 stale_reason 明细", () => {
    const el = mount(hostStub(profile([
      prop({
        source: "derived",
        expression: "{a.x}",
        refs: "[]",
        derivedStatus: { stale: true, staleReason: "引用缺失：电表2.电压", lastEvalAt: null },
      }),
      prop({
        id: "p2",
        name: "avg",
        source: "derived",
        expression: "{a.y}",
        refs: "[]",
        derivedStatus: { stale: false, staleReason: null, lastEvalAt: "2026-10-19T10:20:30" },
      }),
    ])));
    const card = el.querySelector(".derived-status-card");
    expect(card).toBeTruthy();
    expect(card!.textContent).toContain("2 个属性");
    expect(card!.textContent).toContain("1 个等待数据");
    expect(card!.textContent).toContain("10:20:30");
    const details = card!.querySelector("details");
    expect(details).toBeTruthy();
    expect(details!.textContent).toContain("引用缺失：电表2.电压");
  });

  it("状态卡：M=0 不显示「等待数据」段；无派生属性不显示状态卡", () => {
    const withFresh = mount(hostStub(profile([
      prop({
        source: "derived",
        expression: "{a.x}",
        refs: "[]",
        derivedStatus: { stale: false, staleReason: null, lastEvalAt: "2026-10-19T09:00:00" },
      }),
    ])));
    const card = withFresh.querySelector(".derived-status-card");
    expect(card).toBeTruthy();
    expect(card!.textContent).toContain("1 个属性");
    expect(card!.textContent).not.toContain("等待数据");
    expect(card!.textContent).toContain("09:00:00");

    const noDerived = mount(hostStub(profile([prop({ id: "p3", name: "plain" })])));
    expect(noDerived.querySelector(".derived-status-card")).toBeNull();
  });

  it("数值型普通行显示「绑定表达式」；string/bool 行不显示；派生行显示「编辑表达式」「解绑」", () => {
    const host = hostStub(profile([
      prop({ dataType: "float" }),
      prop({ id: "p2", name: "label", dataType: "string" }),
      prop({ id: "p3", name: "flag", dataType: "bool" }),
      prop({ id: "p4", name: "avg", source: "derived", expression: "{a.x}", refs: "[]" }),
    ]));
    host.isNumericType = (dt?: string) =>
      ["number", "float", "int", "integer", "double"].includes((dt || "").trim().toLowerCase());
    const el = mount(host);
    const texts = [...el.querySelectorAll("button")].map(b => b.textContent?.trim());
    expect(texts.filter(t => t === "绑定表达式").length).toBe(1);
    expect(texts.filter(t => t === "编辑表达式").length).toBe(1);
    expect(texts.filter(t => t === "解绑").length).toBe(1);
  });

  it("解绑：confirm 弹窗 Esc 取消；确认后 PUT {expression:null} 并刷新", async () => {
    vi.mocked(thingApi.updatePropertyDef).mockResolvedValue({} as any);
    const host = hostStub(profile([
      prop({ source: "derived", expression: "{a.x}", refs: "[]" }),
    ]));
    const el = mount(host);
    const unbindBtn = () =>
      [...el.querySelectorAll("button")].find(b => b.textContent?.trim() === "解绑")!;

    // 打开 confirm → Esc 取消，不调 API
    unbindBtn().click();
    render(renderDetailProperties(host), el);
    const overlay = el.querySelector(".unbind-confirm-overlay");
    expect(overlay).toBeTruthy();
    overlay!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    render(renderDetailProperties(host), el);
    expect(el.querySelector(".unbind-confirm-overlay")).toBeNull();
    expect(thingApi.updatePropertyDef).not.toHaveBeenCalled();

    // 再次打开 → 确认 → PUT expression:null
    unbindBtn().click();
    render(renderDetailProperties(host), el);
    const confirmBtn = [...el.querySelectorAll<HTMLButtonElement>(".unbind-confirm-overlay button")]
      .find(b => b.textContent?.includes("确认解绑"))!;
    confirmBtn.click();
    await vi.waitFor(() =>
      expect(thingApi.updatePropertyDef).toHaveBeenCalledWith("t1", "p1", { expression: null }));
    expect(host.loadDeviceDetail).toHaveBeenCalledWith("t1");
  });
});
