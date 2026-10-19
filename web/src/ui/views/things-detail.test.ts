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
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "lit";
import { renderDetailProperties } from "./things-detail.js";
import type { DevicesView } from "./things.js";
import type { ThingProfile, ThingProperty } from "../../types/index.js";

const mounted: HTMLElement[] = [];
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
