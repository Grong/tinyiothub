/**
 * 树视图接线层测试（终审 I-1/M-e，2026-10-20）。
 *
 * 锁的是：
 * - loadThingTree 用后端 ListThingsParams 的真实分页参数名 limit（不是
 *   page/pageSize——后者被 serde 静默丢弃，limit 缺省 50，超 50 物时树
 *   静默截断且 buildThingForest 把父不在批内的节点提升为根、层级错排）
 * - 响应 total > 已加载条数 → 树顶部显示「仅显示前 200 个物」提示（D11）
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const listMock = vi.hoisted(() => vi.fn());

vi.mock("../../api/things.js", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../api/things.js")>();
  return {
    ...orig,
    thingApi: { ...orig.thingApi, list: listMock },
  };
});
vi.mock("../../api/drivers.js", () => ({
  driverApi: {
    getDriverNames: vi.fn().mockResolvedValue({ result: [] }),
    getDrivers: vi.fn().mockResolvedValue({ result: [] }),
    getDriverConfig: vi.fn().mockResolvedValue({ result: null }),
  },
}));
vi.mock("../../api/tags.js", () => ({
  tagApi: { getTags: vi.fn().mockResolvedValue({ result: { data: [] } }) },
}));
vi.mock("../components/toast.js", () => ({ success: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() }));

import "./things.js";
import type { DevicesView } from "./things.js";

const mounted: HTMLElement[] = [];

const mkThing = (id: string, parentId?: string) => ({
  id,
  name: `物${id}`,
  displayName: `物${id}`,
  category: "sensors",
  parentId,
});

async function createView(): Promise<DevicesView> {
  const el = document.createElement("view-things") as DevicesView;
  document.body.appendChild(el);
  mounted.push(el);
  await el.updateComplete;
  listMock.mockClear(); // 隔开 connectedCallback 里 loadDevices 的调用
  return el;
}

beforeEach(() => {
  vi.clearAllMocks();
  listMock.mockResolvedValue({ result: { items: [], total: 0 } });
});
afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
});

describe("loadThingTree 分页参数（I-1）", () => {
  it("传 limit（非 page/pageSize）", async () => {
    const el = await createView();
    listMock.mockResolvedValue({ result: { items: [mkThing("a")], total: 1 } });
    await el.loadThingTree();
    expect(listMock).toHaveBeenCalledTimes(1);
    const arg = listMock.mock.calls[0][0];
    expect(arg).toEqual({ limit: "200" });
    expect(arg).not.toHaveProperty("page");
    expect(arg).not.toHaveProperty("pageSize");
  });

  it("total > 已加载条数 → 截断提示「仅显示前 200 个物」", async () => {
    const el = await createView();
    el.viewMode = "tree";
    const items = Array.from({ length: 200 }, (_, i) => mkThing(`t${i}`));
    listMock.mockResolvedValue({ result: { items, total: 250 } });
    await el.loadThingTree();
    await el.updateComplete;
    expect(el.treeTruncated).toBe(true);
    expect(el.textContent).toContain("仅显示前 200 个物");
  });

  it("total == 已加载条数 → 无截断提示", async () => {
    const el = await createView();
    el.viewMode = "tree";
    listMock.mockResolvedValue({
      result: { items: [mkThing("root"), mkThing("child", "root")], total: 2 },
    });
    await el.loadThingTree();
    await el.updateComplete;
    expect(el.treeTruncated).toBe(false);
    expect(el.textContent).not.toContain("仅显示前");
    // 层级正确装配：child 挂 root 下（森林只有 1 个根）
    expect(el.treeNodes).toHaveLength(1);
    expect(el.treeNodes[0].children[0]?.id).toBe("child");
  });
});
