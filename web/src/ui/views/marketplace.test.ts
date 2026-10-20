/**
 * 市场实例化可视化编辑器测试（X3a 前端，Task 8）。
 *
 * 锁的是：
 * - 提交 payload 组装：attachSimulatedDriver 随「挂模拟驱动」开关（默认开），
 *   extraNodes 由本地待建节点列表组装（templateId/parentPath/name/count/driverName）
 * - 确认按钮文案含总数「确认实例化（将创建 N 个本体）」（dry-run 场景包计数 + 待建节点计数）
 * - 「+ 添加设备」表单确认 → 本地待建节点（虚线）出现在树上、计入总数
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const instantiateMock = vi.hoisted(() => vi.fn());

vi.mock("../../api/marketplace.js", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../api/marketplace.js")>();
  return {
    ...orig,
    marketplaceApi: {
      ...orig.marketplaceApi,
      getTemplates: vi.fn().mockResolvedValue({ result: [] }),
      getDrivers: vi.fn().mockResolvedValue({ result: [] }),
    },
    sceneApi: {
      ...orig.sceneApi,
      listThingTemplates: vi.fn().mockResolvedValue({ data: [], pagination: {} }),
      getThingTemplate: vi.fn(),
      instantiate: instantiateMock,
    },
  };
});
vi.mock("../../api/templates.js", () => ({
  templateApi: {
    getTemplates: vi.fn().mockResolvedValue({ result: [] }),
    getTemplate: vi.fn().mockResolvedValue({ result: null }),
  },
}));
vi.mock("../../api/drivers.js", () => ({
  driverApi: { getDrivers: vi.fn().mockResolvedValue({ result: [] }) },
}));
vi.mock("../components/toast.js", () => ({ success: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() }));

import "./marketplace.js";
import type { MarketplaceView } from "./marketplace.js";

const mounted: HTMLElement[] = [];

beforeEach(() => {
  vi.clearAllMocks();
  instantiateMock.mockResolvedValue({
    nodeCount: 12,
    rootThingId: "r1",
    treePreview: "园区A (campus)",
    warnings: [],
    quota: { current: 0, limit: null, after: 12 },
  });
});
afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
});

async function createView(): Promise<MarketplaceView> {
  const el = document.createElement("view-marketplace") as MarketplaceView;
  document.body.appendChild(el);
  mounted.push(el);
  await el.updateComplete;
  return el;
}

/** 直接把对话框推进到可提交状态（绕过 openSceneDialog 的网络加载）。 */
function primeDialog(el: any) {
  el.sceneDialogItem = { id: "tpl1", name: "smart_campus", category: "scenes", isComposition: true };
  el.sceneDialogVisible = true;
  el.sceneDetailLoading = false;
  el.sceneDetail = { parameters: [], structureSummary: { parameterCount: 0, maxDepth: 1 } };
  el.sceneName = "园区A";
  el.sceneParams = {};
  el.pendingNodes = [];
  el.attachSim = true;
}

describe("实例化提交 payload 组装", () => {
  it("extraNodes 由待建节点组装；attachSimulatedDriver 随开关（默认开）", async () => {
    const el = createViewEl();
    primeDialog(el);
    el.pendingNodes = [
      { id: "p1", templateId: "meter-tpl", category: "meter", propertyCount: 6, name: "电表", count: 3, driverName: "simulator", parentPath: ["building"] },
      { id: "p2", templateId: "water-tpl", category: "meter", propertyCount: 4, name: "水表", count: 1, driverName: null, parentPath: [] },
    ];
    await el.submitScene();
    expect(instantiateMock).toHaveBeenCalledTimes(1);
    const [idArg, body] = instantiateMock.mock.calls[0];
    expect(idArg).toBe("tpl1");
    expect(body.dryRun).toBe(false);
    expect(body.attachSimulatedDriver).toBe(true);
    expect(body.extraNodes).toEqual([
      { templateId: "meter-tpl", parentPath: ["building"], name: "电表", count: 3, driverName: "simulator" },
      { templateId: "water-tpl", parentPath: [], name: "水表", count: 1, driverName: null },
    ]);
  });

  it("开关关闭 → attachSimulatedDriver=false；无待建节点 → extraNodes=[]", async () => {
    const el = createViewEl();
    primeDialog(el);
    el.attachSim = false;
    await el.submitScene();
    const [, body] = instantiateMock.mock.calls[0];
    expect(body.attachSimulatedDriver).toBe(false);
    expect(body.extraNodes).toEqual([]);
  });
});

describe("确认按钮文案与待建节点", () => {
  it("文案含总数：dry-run 计数 + 待建节点计数", async () => {
    const el = await createView();
    primeDialog(el);
    el.preview = { nodeCount: 12, rootThingId: null, treePreview: "园区A (campus)", warnings: [], quota: { current: 0, limit: null, after: 12 } };
    el.pendingNodes = [
      { id: "p1", templateId: "meter-tpl", category: "meter", propertyCount: 6, name: "电表", count: 2, driverName: "simulator", parentPath: [] },
      { id: "p2", templateId: "water-tpl", category: "meter", propertyCount: 4, name: "水表", count: 1, driverName: null, parentPath: [] },
    ];
    await el.updateComplete;
    const btn = el.querySelector(".mp-modal-footer .btn.primary");
    expect(btn?.textContent).toContain("确认实例化（将创建 15 个本体）");
  });

  it("「+ 添加设备」表单确认 → 待建节点入列（默认 simulator），树上出现虚线节点", async () => {
    const el = await createView();
    primeDialog(el);
    el.preview = { nodeCount: 2, rootThingId: null, treePreview: "园区A (campus)\n  1号楼 (building)", warnings: [], quota: { current: 0, limit: null, after: 2 } };
    // 结构走查上下文（openSceneDialog 后台加载的那部分，这里直接注入）
    (el as any).sceneStruct = {
      root: { children: [{ key: "building", category: "building" }] },
      refs: new Map(),
      devices: new Map(),
    };
    el.deviceTemplateOptions = [
      { id: "meter-tpl", name: "power_meter", displayName: "电表", category: "meter", isBuiltin: false, isComposition: false, parameterCount: 0, propertyCount: 6 },
    ];
    el.openAddDeviceFormFor(["building"]);
    el.addName = "电表";
    el.addCount = 2;
    el.confirmAddDevice();
    expect(el.pendingNodes).toHaveLength(1);
    expect(el.pendingNodes[0]).toMatchObject({
      templateId: "meter-tpl",
      parentPath: ["building"],
      name: "电表",
      count: 2,
      driverName: "simulator",
    });
    await el.updateComplete;
    expect(el.querySelector(".st-node--pending")).not.toBeNull();
  });
});

/** createView 的非 await 变体（submitScene 路径不依赖渲染完成） */
function createViewEl(): any {
  const el = document.createElement("view-marketplace") as any;
  document.body.appendChild(el);
  mounted.push(el);
  return el;
}
