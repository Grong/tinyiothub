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
  findTreeNode,
  isDescendantOrSelf,
  parseTreePreview,
  renderSceneTree,
  reparentTreeNode,
  type PendingNode,
  type SceneStructRoot,
  type SceneTreeHost,
  type SceneTreeNode,
} from "./scene-tree.js";

// toast 仅作物视图（用例4）导入链的副作用隔离
vi.mock("../components/toast.js", () => ({ success: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() }));

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

  it("待建节点按 parentPath 叠加到挂载点（count>1 显示 ×N）", () => {
    const pending: PendingNode[] = [
      { id: "p1", templateId: "meter", category: "meter", propertyCount: 6, name: "电表", count: 3, driverName: "simulator", parentPath: ["building"] },
      { id: "p2", templateId: "water", category: "meter", propertyCount: 4, name: "水表", count: 1, driverName: "modbus_rtu", parentPath: [] },
    ];
    const tree = buildSceneTree(baseArgs({ pending }));
    const buildingMeter = tree[0].children[0].children.find((n) => n.pending?.id === "p1");
    expect(buildingMeter?.name).toBe("电表 ×3");
    expect(buildingMeter?.driver).toBe("sim");
    expect(tree[0].children.find((n) => n.pending?.id === "p2")?.driver).toBe("physical");
  });

  it("待建节点 driverName=null = 服从总开关（与后端协议同口径）：开→「模拟」，关→「无」", () => {
    const pending: PendingNode[] = [
      { id: "p1", templateId: "water", category: "meter", propertyCount: 4, name: "水表", count: 1, driverName: null, parentPath: [] },
    ];
    const on = buildSceneTree(baseArgs({ pending, attachSim: true }));
    expect(on[0].children.find((n) => n.pending)?.driver).toBe("sim");
    const off = buildSceneTree(baseArgs({ pending, attachSim: false }));
    expect(off[0].children.find((n) => n.pending)?.driver).toBe("none");
  });

  it("降级保护①：scene_ref 拉取失败 → 该节点整棵子树标记「引用加载失败，不可挂接」，根仍可挂接", () => {
    const campus: SceneStructRoot = {
      parameters: [{ name: "building_count", default: 1 }],
      children: [{ key: "building", category: "building", scene_ref: "smart_building", count_param: "building_count" }],
    };
    const preview = ["园区A (campus)", "  1号楼 (building)", "    1F (floor)"].join("\n");
    // refScenes 为空 = 引用模板拉取失败
    const tree = buildSceneTree(
      baseArgs({ previewText: preview, rootScene: campus, refScenes: new Map(), params: { building_count: 1 } }),
    );
    const root = tree[0];
    const building = root.children[0];
    const floor = building.children[0];
    expect(root.keyPath).toEqual([]);
    expect(root.mountBlocked).toBeFalsy();
    expect(building.keyPath).toBeNull();
    expect(building.mountBlocked).toContain("引用加载失败");
    expect(floor.mountBlocked).toContain("引用加载失败");
    const el = mount(hostStub(), tree);
    const buildingEl = [...el.querySelectorAll(".st-node")].find(
      (n) => n.querySelector(":scope > .st-row .st-name")?.textContent === "1号楼",
    )!;
    expect(buildingEl.querySelector(":scope > .st-row .st-add-device")).toBeNull();
    expect(buildingEl.textContent).toContain("不可挂接");
    // 根不受影响
    const rootEl = el.querySelector(".st-node")!;
    expect(rootEl.querySelector(":scope > .st-row .st-add-device")).not.toBeNull();
  });

  it("降级保护②：骨架/预览数量发散 → 该层整层标记「结构对齐失败」禁用加设备（不静默错位）", () => {
    // 结构按参数展开 1 栋楼，预览却有 2 栋（如 500 截断/参数竞态）→ 兄弟序不可信
    const preview = ["园区A (campus)", "  1号楼 (building)", "  2号楼 (building)"].join("\n");
    const tree = buildSceneTree(baseArgs({ previewText: preview, params: { building_count: 1, floor_count: 2 } }));
    const root = tree[0];
    expect(root.keyPath).toEqual([]); // 根层 1v1 对齐正常
    for (const b of root.children) {
      expect(b.keyPath).toBeNull();
      expect(b.mountBlocked).toContain("结构对齐失败");
    }
    const el = mount(hostStub(), tree);
    expect(el.textContent).toContain("结构对齐失败，不可挂接");
    // 发散层无加设备按钮；根仍有
    const addBtns = [...el.querySelectorAll(".st-add-device")];
    expect(addBtns).toHaveLength(1);
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

// ──────────────────────────────────────────────
// Task 9：拖拽换父 + 树上新增设备（D12 落地）
// ──────────────────────────────────────────────

/** 物树节点（keyPath = 根到自身的 thing id 路径，末位=自身 id） */
function thingNode(id: string, children: SceneTreeNode[] = [], path: string[] = [id]): SceneTreeNode {
  return {
    id,
    name: id,
    category: "thing",
    children,
    keyPath: path,
    mountBlocked: null,
    propertyCount: null,
    isDevice: false,
    countParam: null,
    driver: null,
  };
}

/** A ─ B ─ C；D（B 的路径 ["A","B"]，C 的路径 ["A","B","C"]） */
function thingForest(): SceneTreeNode[] {
  return [
    thingNode("A", [thingNode("B", [thingNode("C", [], ["A", "B", "C"])], ["A", "B"])]),
    thingNode("D"),
  ];
}

function rowOf(el: HTMLElement, id: string): HTMLElement {
  const row = el.querySelector(`[data-node-id="${id}"] > .st-row`);
  expect(row, `节点 ${id} 的行`).not.toBeNull();
  return row as HTMLElement;
}

/**
 * jsdom 无 DragEvent/DataTransfer 实现（组件侧对 dataTransfer 一律可选链访问，
 * 仅真实浏览器需要 setData 才能触发原生 drop），故用原生 Event + 最小 stub。
 */
function dndEvent(type: string): Event {
  const e = new Event(type, { bubbles: true, cancelable: true });
  (e as unknown as { dataTransfer: unknown }).dataTransfer = {
    setData: () => {},
    effectAllowed: "",
    dropEffect: "",
  };
  return e;
}

describe("Task 9：拖拽换父（D12）", () => {
  it("用例1：拖到自身/后代 → 红框 class + 拒绝提示，不发请求", () => {
    const host = hostStub({ onTreeReparent: vi.fn() });
    const el = mount(host, thingForest());
    const rowA = rowOf(el, "A");
    const rowC = rowOf(el, "C");
    // 拖 A
    rowA.dispatchEvent(dndEvent("dragstart"));
    expect(rowA.getAttribute("draggable")).toBe("true");
    // 落到自身 → 红框
    rowA.dispatchEvent(dndEvent("dragover"));
    expect(rowA.classList.contains("st-drop-bad")).toBe(true);
    // 落到后代 C → 红框 + 提示
    rowC.dispatchEvent(dndEvent("dragover"));
    expect(rowC.classList.contains("st-drop-bad")).toBe(true);
    const hint = el.querySelector(".st-drop-hint") as HTMLElement;
    expect(hint).not.toBeNull();
    expect(hint.style.display).not.toBe("none");
    expect(hint.textContent).toContain("不能移动");
    // 非法落点 drop → 不回调（不落库）
    rowC.dispatchEvent(dndEvent("drop"));
    expect(host.onTreeReparent).not.toHaveBeenCalled();
    // dragend 后清理
    rowA.dispatchEvent(dndEvent("dragend"));
    expect(rowA.classList.contains("st-drop-bad")).toBe(false);
    expect((el.querySelector(".st-drop-hint") as HTMLElement).style.display).toBe("none");
  });

  it("用例2：合法落点 drop → 调 PUT parent_id，树局部刷新", async () => {
    const onTreeReparent = vi.fn();
    const host = hostStub({ onTreeReparent });
    const forest = thingForest();
    const el = mount(host, forest);
    const rowC = rowOf(el, "C");
    const rowD = rowOf(el, "D");
    // 拖 C 到 D（非自身/后代）→ 合法落点标记，drop 触发回调
    rowC.dispatchEvent(dndEvent("dragstart"));
    rowD.dispatchEvent(dndEvent("dragover"));
    expect(rowD.classList.contains("st-drop-ok")).toBe(true);
    expect(rowD.classList.contains("st-drop-bad")).toBe(false);
    rowD.dispatchEvent(dndEvent("drop"));
    expect(onTreeReparent).toHaveBeenCalledWith("C", "D");
    // 编排层：调 PUT（updateParent stub）+ 返回局部移动后的新树
    const updateParent = vi.fn().mockResolvedValue({});
    const next = await reparentTreeNode(forest, "C", "D", { updateParent, onError: vi.fn() });
    expect(updateParent).toHaveBeenCalledWith("C", "D");
    expect(findTreeNode(next, "D")!.children.map((n) => n.id)).toEqual(["C"]);
    expect(findTreeNode(next, "B")!.children).toHaveLength(0);
    // 原树不被改写（不可变局部刷新）
    expect(findTreeNode(forest, "B")!.children).toHaveLength(1);
  });

  it("用例3：PUT 失败 → toast + 树回滚原位置", async () => {
    const forest = thingForest();
    const onError = vi.fn();
    const updateParent = vi.fn().mockRejectedValue(new Error("网络错误"));
    const next = await reparentTreeNode(forest, "B", "D", { updateParent, onError });
    expect(onError).toHaveBeenCalledWith(expect.stringContaining("网络错误"));
    // 回滚：返回原树（位置不变，引用同原）
    expect(next).toBe(forest);
    expect(findTreeNode(next, "A")!.children.map((n) => n.id)).toEqual(["B"]);
    expect(findTreeNode(next, "D")!.children).toHaveLength(0);
  });

  it("编排层成环兜底：目标为自身/后代时 reparentTreeNode 不调 API 原样返回", async () => {
    const forest = thingForest();
    const updateParent = vi.fn();
    expect(isDescendantOrSelf(findTreeNode(forest, "A")!, "C")).toBe(true);
    expect(isDescendantOrSelf(findTreeNode(forest, "A")!, "A")).toBe(true);
    expect(isDescendantOrSelf(findTreeNode(forest, "A")!, "D")).toBe(false);
    const next = await reparentTreeNode(forest, "A", "C", { updateParent, onError: vi.fn() });
    expect(updateParent).not.toHaveBeenCalled();
    expect(next).toBe(forest);
  });

  it("用例4：节点「+」→ 打开建物向导且 parent_id 预填为该节点", async () => {
    // 树侧：「+」回调带出该节点的 id 路径（末位 = 该节点 thing id）
    const host = hostStub();
    const el = mount(host, thingForest());
    const addBtn = rowOf(el, "B").querySelector(".st-add-device") as HTMLButtonElement;
    expect(addBtn).not.toBeNull();
    addBtn.click();
    expect(host.onTreeAddDevice).toHaveBeenCalledWith(["A", "B"]);

    // 视图侧：openWizard(parentId) 预填，提交载荷带 parentId
    const { DevicesView } = await import("./things.js");
    const { thingApi } = await import("../../api/things.js");
    const view = new DevicesView();
    view.openWizard("thing-B");
    expect(view.wizParentId).toBe("thing-B");
    // 工具栏「新建物」走 click（Event 入参）→ 不预填
    view.openWizard(new MouseEvent("click"));
    expect(view.wizParentId).toBeNull();

    const createSpy = vi
      .spyOn(thingApi, "createFromTemplate")
      .mockResolvedValue({ code: 0, result: { id: "new-1" } } as never);
    vi.spyOn(thingApi, "list").mockResolvedValue({ code: 0, result: { items: [], total: 0 } } as never);
    view.openWizard("thing-B");
    view.wizSelectedTemplate = { id: "tpl-1", name: "电表", deviceInfo: {}, commands: [] } as never;
    view.wizName = "子电表";
    await view.submitWizard();
    expect(createSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        templateId: "tpl-1",
        input: expect.objectContaining({ parentId: "thing-B" }),
      }),
    );
  });
});
