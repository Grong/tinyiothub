/**
 * 场景交互树组件测试（X3a 前端，Task 8）。
 *
 * 锁的是简报 4 条用例：
 * 1) 树渲染：节点类型徽标 + 属性数 + 驱动标记（模拟→"模拟"文字标签，非纯色编码，D11）
 * 2) 参数化节点（楼栋/楼层）树上 +/- 改数 → 触发参数变更回调
 * 3) 「+ 添加设备」→ 回调带出挂载点 keyPath；待建节点虚线框样式、可删除
 * 4) 提交 payload 组装：extra_nodes 含 template_id/parent_path/count/driver_name
 *    （key 路径来自场景结构走查：根 []、楼栋 ["building"]、楼层 ["building","floor"]）
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "lit";
import {
  buildExtraNodesPayload,
  buildSceneTree,
  parseTreePreview,
  renderSceneTree,
  type PendingNode,
  type SceneStructRoot,
  type SceneTreeHost,
  type SceneTreeNode,
} from "./scene-tree.js";

const mounted: HTMLElement[] = [];
beforeEach(() => vi.clearAllMocks());
afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
});

function hostStub(over: Partial<SceneTreeHost> = {}): SceneTreeHost {
  return {
    onTreeParamStep: vi.fn(),
    onTreeAddDevice: vi.fn(),
    onTreeRemovePending: vi.fn(),
    onTreePendingDriver: vi.fn(),
    renderAddDeviceForm: () => null,
    ...over,
  };
}

function mount(host: SceneTreeHost, nodes: SceneTreeNode[]): HTMLElement {
  const el = document.createElement("div");
  render(renderSceneTree(host, nodes, { editable: true }), el);
  document.body.appendChild(el);
  mounted.push(el);
  return el;
}

// 1 栋楼 × 2 层 × 每层 2 传感器的园区预览（与后端 tree_preview 同格式）
const PREVIEW = [
  "园区A (campus)",
  "  1号楼 (building)",
  "    1F (floor)",
  "      温湿度传感器1 (sensor)",
  "      温湿度传感器2 (sensor)",
  "    2F (floor)",
  "      温湿度传感器1 (sensor)",
  "      温湿度传感器2 (sensor)",
].join("\n");

const ROOT_SCENE: SceneStructRoot = {
  parameters: [
    { name: "building_count", default: 1 },
    { name: "floor_count", default: 2 },
  ],
  children: [
    {
      key: "building",
      category: "building",
      count_param: "building_count",
      children: [
        {
          key: "floor",
          category: "floor",
          count_param: "floor_count",
          children: [{ key: "th_sensor", template_ref: "th_sensor", count: 2 }],
        },
      ],
    },
  ],
};

const DEVICE_TEMPLATES = new Map([["th_sensor", { propertyCount: 2, category: "sensor" }]]);

function baseArgs(over: Record<string, unknown> = {}) {
  return {
    previewText: PREVIEW,
    rootScene: ROOT_SCENE,
    rootName: "smart_campus",
    deviceTemplates: DEVICE_TEMPLATES,
    params: { building_count: 1, floor_count: 2 },
    editableParams: new Set(["building_count", "floor_count"]),
    rootPropertyCount: 5,
    attachSim: true,
    pending: [] as PendingNode[],
    ...over,
  };
}

describe("parseTreePreview", () => {
  it("按缩进解析为嵌套树（名称 + 类型）", () => {
    const roots = parseTreePreview(PREVIEW);
    expect(roots).toHaveLength(1);
    expect(roots[0].name).toBe("园区A");
    expect(roots[0].category).toBe("campus");
    expect(roots[0].children[0].name).toBe("1号楼");
    expect(roots[0].children[0].children.map((n) => n.name)).toEqual(["1F", "2F"]);
    expect(roots[0].children[0].children[0].children).toHaveLength(2);
  });
});

describe("buildSceneTree — 结构走查叠加", () => {
  it("key 路径：根 []、楼栋 [building]、楼层 [building,floor]；template_ref 叶为设备", () => {
    const tree = buildSceneTree(baseArgs());
    const root = tree[0];
    const building = root.children[0];
    const floor = building.children[0];
    const sensor = floor.children[0];
    expect(root.keyPath).toEqual([]);
    expect(building.keyPath).toEqual(["building"]);
    expect(floor.keyPath).toEqual(["building", "floor"]);
    expect(sensor.keyPath).toEqual(["building", "floor", "th_sensor"]);
    expect(sensor.isDevice).toBe(true);
    expect(building.isDevice).toBe(false);
  });

  it("scene_ref + param_mapping：引用模板子树同样挂 key（目标参数取映射值）", () => {
    // smart_campus → scene_ref smart_building（floor_count 经 param_mapping 传入）
    const campus: SceneStructRoot = {
      parameters: [
        { name: "building_count", default: 1 },
        { name: "floor_count", default: 2 },
      ],
      children: [
        {
          key: "building",
          category: "building",
          scene_ref: "smart_building",
          count_param: "building_count",
          param_mapping: { floor_count: "floor_count" },
        },
      ],
    };
    const building: SceneStructRoot = {
      parameters: [{ name: "floor_count", default: 9 }],
      children: [
        {
          key: "floor",
          category: "floor",
          count_param: "floor_count",
          children: [{ key: "th_sensor", template_ref: "th_sensor", count: 2 }],
        },
      ],
    };
    const tree = buildSceneTree(
      baseArgs({
        rootScene: campus,
        refScenes: new Map([["smart_building", building]]),
      }),
    );
    const b = tree[0].children[0];
    expect(b.keyPath).toEqual(["building"]);
    expect(b.children.map((n) => n.name)).toEqual(["1F", "2F"]); // 映射值 2 而非目标默认 9
    expect(b.children[0].keyPath).toEqual(["building", "floor"]);
  });

  it("参数化节点带 countParam；属性数：根=rootPropertyCount、设备=模板 propertyCount", () => {
    const tree = buildSceneTree(baseArgs());
    expect(tree[0].propertyCount).toBe(5);
    expect(tree[0].children[0].countParam).toBe("building_count");
    expect(tree[0].children[0].children[0].countParam).toBe("floor_count");
    expect(tree[0].children[0].children[0].children[0].propertyCount).toBe(2);
  });

  it("驱动标记：挂模拟开关开 → 叶设备 sim；关 → none；容器无驱动标记", () => {
    const on = buildSceneTree(baseArgs({ attachSim: true }));
    expect(on[0].children[0].children[0].children[0].driver).toBe("sim");
    expect(on[0].children[0].driver).toBeNull();
    const off = buildSceneTree(baseArgs({ attachSim: false }));
    expect(off[0].children[0].children[0].children[0].driver).toBe("none");
  });

  it("待建节点按 parentPath 叠加到挂载点（count>1 显示 ×N），驱动三态", () => {
    const pending: PendingNode[] = [
      { id: "p1", templateId: "meter", category: "meter", propertyCount: 6, name: "电表", count: 3, driverName: "simulator", parentPath: ["building"] },
      { id: "p2", templateId: "water", category: "meter", propertyCount: 4, name: "水表", count: 1, driverName: "modbus_rtu", parentPath: [] },
      { id: "p3", templateId: "water", category: "meter", propertyCount: 4, name: "水表2", count: 1, driverName: null, parentPath: [] },
    ];
    const tree = buildSceneTree(baseArgs({ pending }));
    const buildingMeter = tree[0].children[0].children.find((n) => n.pending?.id === "p1");
    expect(buildingMeter?.name).toBe("电表 ×3");
    expect(buildingMeter?.driver).toBe("sim");
    const rootChildren = tree[0].children.filter((n) => n.pending);
    expect(rootChildren.map((n) => n.driver)).toEqual(["physical", "none"]);
  });
});

describe("renderSceneTree — 渲染与交互", () => {
  it("用例1：类型徽标 + 属性数 + 驱动文字标签（模拟→「模拟」，非纯色）", () => {
    const el = mount(hostStub(), buildSceneTree(baseArgs()));
    // 类型徽标
    expect(el.textContent).toContain("building");
    expect(el.textContent).toContain("sensor");
    // 属性数
    expect(el.textContent).toContain("2 属性");
    // 驱动标记必须带文字（D11）
    const sim = el.querySelector('[data-driver="sim"]');
    expect(sim).not.toBeNull();
    expect(sim!.textContent).toContain("模拟");
  });

  it("用例2：参数化节点 +/- 改数 → 触发参数变更回调", () => {
    const host = hostStub();
    const el = mount(host, buildSceneTree(baseArgs()));
    const buildingRow = el.querySelector('[data-node-id] [data-count-param="building_count"]');
    expect(buildingRow).not.toBeNull();
    const plus = buildingRow!.querySelector('[data-step="1"]') as HTMLButtonElement;
    const minus = buildingRow!.querySelector('[data-step="-1"]') as HTMLButtonElement;
    plus.click();
    minus.click();
    expect(host.onTreeParamStep).toHaveBeenCalledWith("building_count", 1);
    expect(host.onTreeParamStep).toHaveBeenCalledWith("building_count", -1);
  });

  it("用例3：「+ 添加设备」回调带挂载点 keyPath；待建节点虚线样式、可删", () => {
    const pending: PendingNode[] = [
      { id: "p1", templateId: "meter", category: "meter", propertyCount: 6, name: "电表", count: 1, driverName: "simulator", parentPath: ["building"] },
    ];
    const host = hostStub();
    const el = mount(host, buildSceneTree(baseArgs({ pending })));
    // 挂载按钮（容器节点有，设备叶节点没有）
    const addBtns = [...el.querySelectorAll(".st-add-device")];
    expect(addBtns.length).toBeGreaterThan(0);
    (addBtns[1] as HTMLButtonElement).click(); // 根之后第一个 = 楼栋
    expect(host.onTreeAddDevice).toHaveBeenCalledWith(["building"]);
    // 传感器行不出现挂载按钮
    const sensorNode = el.querySelector('[data-node-id] [data-driver="sim"]')!.closest(".st-node")!;
    expect(sensorNode.querySelector(":scope > .st-row .st-add-device")).toBeNull();
    // 待建节点虚线 + 删除
    const pendingEl = el.querySelector(".st-node--pending");
    expect(pendingEl).not.toBeNull();
    expect((pendingEl as HTMLElement).getAttribute("style")).toContain("dashed");
    (pendingEl!.querySelector(".st-pending-del") as HTMLButtonElement).click();
    expect(host.onTreeRemovePending).toHaveBeenCalledWith("p1");
  });

  it("待建节点可改驱动（下拉触发回调，null=无驱动）", () => {
    const pending: PendingNode[] = [
      { id: "p1", templateId: "meter", category: "meter", propertyCount: 6, name: "电表", count: 1, driverName: "simulator", parentPath: [] },
    ];
    const host = hostStub();
    const el = document.createElement("div");
    render(
      renderSceneTree(host, buildSceneTree(baseArgs({ pending })), {
        editable: true,
        driverOptions: [
          { value: "simulator", label: "模拟驱动 (simulator)" },
          { value: "none", label: "无驱动" },
        ],
      }),
      el,
    );
    document.body.appendChild(el);
    mounted.push(el);
    const select = el.querySelector(".st-pending-driver") as HTMLSelectElement;
    expect(select).not.toBeNull();
    select.value = "none";
    select.dispatchEvent(new Event("change"));
    expect(host.onTreePendingDriver).toHaveBeenCalledWith("p1", null);
  });
});

describe("用例4：提交 payload 组装", () => {
  it("extra_nodes 含 templateId/parentPath/name/count/driverName（driverName 可为 null）", () => {
    const payload = buildExtraNodesPayload([
      { id: "p1", templateId: "meter", category: "meter", propertyCount: 6, name: "电表", count: 3, driverName: "simulator", parentPath: ["building"] },
      { id: "p2", templateId: "water", category: "meter", propertyCount: 4, name: "水表", count: 1, driverName: null, parentPath: [] },
    ]);
    expect(payload).toEqual([
      { templateId: "meter", parentPath: ["building"], name: "电表", count: 3, driverName: "simulator" },
      { templateId: "water", parentPath: [], name: "水表", count: 1, driverName: null },
    ]);
  });
});
