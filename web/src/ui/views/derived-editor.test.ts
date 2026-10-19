/**
 * 派生编辑器 drawer 测试（园区物派生属性 Task 11）。
 *
 * 锁的是（简报 6 条用例）：
 * 1) 物选择器：树形渲染 + 搜索过滤 + 选物后只列数值属性（float/int/number/double，大小写不敏感）
 * 2) 选属性 → 表达式区插入 `{物名.属性名}` 文本
 * 3) 运算符按钮（× → " * "）与 descendant_count() 插入
 * 4) 保存 400 → 后端明细文本原样行内红字（非裸 toast），drawer 保持打开
 * 5) 保存成功 → toast + drawer 关闭 + 重新加载详情（属性行出现）
 * 6) 无候选物空态："本工作区还没有带数值属性的物"
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "lit";
import {
  renderThingPicker,
  renderDerivedEditor,
  openDerivedEditorCreate,
  saveDerivedEditor,
  type DerivedEditorHost,
  type DerivedEditorState,
} from "./derived-editor.js";
import { thingApi } from "../../api/things.js";
import { success } from "../components/toast.js";

vi.mock("../../api/things.js", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../api/things.js")>();
  return {
    ...orig,
    thingApi: {
      ...orig.thingApi,
      list: vi.fn(),
      getProperties: vi.fn(),
      createProperty: vi.fn(),
      updatePropertyDef: vi.fn(),
    },
  };
});
vi.mock("../components/toast.js", () => ({ success: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() }));

const mounted: HTMLElement[] = [];
beforeEach(() => vi.clearAllMocks());
afterEach(() => { while (mounted.length) mounted.pop()?.remove(); });

function container(): HTMLElement {
  const el = document.createElement("div");
  document.body.appendChild(el);
  mounted.push(el);
  return el;
}

function hostStub(patch: Partial<DerivedEditorState> = {}): DerivedEditorHost {
  const host: DerivedEditorHost = {
    selectedDevice: { thing: { id: "t-host" } },
    derivedEditor: null,
    requestUpdate: vi.fn(),
    loadDeviceDetail: vi.fn(),
  };
  openDerivedEditorCreate(host);
  host.derivedEditor = { ...host.derivedEditor!, ...patch };
  return host;
}

function mountDrawer(host: DerivedEditorHost): HTMLElement {
  const el = container();
  render(renderDerivedEditor(host), el);
  return el;
}

describe("物选择器 renderThingPicker", () => {
  it("树形渲染 + 搜索过滤 + 选物后只列数值属性（大小写不敏感）", async () => {
    const el = container();
    const onPick = vi.fn();
    renderThingPicker(el, {
      onPick,
      fetchThings: async () => [
        { id: "t1", name: "meter1", displayName: "电表1" },
        { id: "t2", name: "sub-meter", parentId: "t1" },
        { id: "t3", name: "gateway" },
      ],
      fetchProperties: async () => [
        { name: "voltage", dataType: "float" },
        { name: "count", dataType: "INT" },
        { name: "ratio", dataType: "Double" },
        { name: "num", dataType: "number" },
        { name: "label", dataType: "string" },
        { name: "online", dataType: "bool" },
      ],
    });
    await vi.waitFor(() => expect(el.querySelectorAll(".thing-picker__thing").length).toBe(3));
    // 树形：子物缩进比父物深
    const child = el.querySelector<HTMLElement>('[data-thing="t2"]')!;
    const root = el.querySelector<HTMLElement>('[data-thing="t1"]')!;
    expect(parseInt(child.style.paddingLeft)).toBeGreaterThan(parseInt(root.style.paddingLeft));

    // 搜索过滤
    const search = el.querySelector<HTMLInputElement>(".thing-picker__search")!;
    search.value = "gateway";
    search.dispatchEvent(new Event("input"));
    expect(el.querySelectorAll(".thing-picker__thing").length).toBe(1);
    search.value = "不存在";
    search.dispatchEvent(new Event("input"));
    expect(el.textContent).toContain("无匹配的物");
    el.querySelector<HTMLButtonElement>(".thing-picker__clear")!.click();
    expect(el.querySelectorAll(".thing-picker__thing").length).toBe(3);

    // 选物 → 只列数值属性（float/INT/Double/number 通过；string/bool 被过滤）
    root.click();
    await vi.waitFor(() => expect(el.querySelectorAll(".thing-picker__prop").length).toBe(4));
    const names = [...el.querySelectorAll(".thing-picker__prop")].map(b => b.getAttribute("data-prop"));
    expect(names).toEqual(["voltage", "count", "ratio", "num"]);
  });

  it("无候选物空态：本工作区还没有带数值属性的物", async () => {
    const el = container();
    renderThingPicker(el, { onPick: vi.fn(), fetchThings: async () => [] });
    await vi.waitFor(() => expect(el.textContent).toContain("本工作区还没有带数值属性的物"));
  });
});

describe("派生编辑器 drawer", () => {
  it("选物选属性 → 表达式区插入 {物名.属性名} 文本", async () => {
    vi.mocked(thingApi.list).mockResolvedValue({ result: { items: [{ id: "t1", name: "meter2" }] } } as any);
    vi.mocked(thingApi.getProperties).mockResolvedValue({ result: [{ name: "voltage", dataType: "float" }] } as any);

    const host = hostStub({ pickerOpen: true });
    const el = mountDrawer(host);
    await vi.waitFor(() => expect(el.querySelector(".thing-picker__thing")).toBeTruthy());
    el.querySelector<HTMLButtonElement>(".thing-picker__thing")!.click();
    await vi.waitFor(() => expect(el.querySelector(".thing-picker__prop")).toBeTruthy());
    el.querySelector<HTMLButtonElement>(".thing-picker__prop")!.click();

    expect(host.derivedEditor!.expression).toBe("{meter2.voltage}");
    render(renderDerivedEditor(host), el);
    expect(el.querySelector<HTMLTextAreaElement>(".derived-drawer__expression")!.value).toBe("{meter2.voltage}");
  });

  it("运算符按钮与 descendant_count() 插入（追加到末尾）", () => {
    const host = hostStub({ expression: "{a.x}" });
    const el = mountDrawer(host);
    const buttons = [...el.querySelectorAll<HTMLButtonElement>(".derived-drawer__toolbar button")];
    buttons.find(b => b.textContent === "×")!.click();
    render(renderDerivedEditor(host), el);
    const buttons2 = [...el.querySelectorAll<HTMLButtonElement>(".derived-drawer__toolbar button")];
    buttons2.find(b => b.textContent === "descendant_count()")!.click();

    expect(host.derivedEditor!.expression).toBe("{a.x} * descendant_count()");
  });

  it("保存 400 → 明细文本原样行内红字，drawer 保持打开，不发 toast", async () => {
    vi.mocked(thingApi.createProperty).mockRejectedValue(new Error("语法错误：第 5 个字符附近"));
    const host = hostStub({ name: "avg_power", expression: "{a.x} +" });
    const el = mountDrawer(host);

    await saveDerivedEditor(host);
    render(renderDerivedEditor(host), el);

    const err = el.querySelector(".derived-editor__error");
    expect(err).toBeTruthy();
    expect(err!.textContent).toContain("语法错误：第 5 个字符附近");
    expect(host.derivedEditor).not.toBeNull();
    expect(success).not.toHaveBeenCalled();
  });

  it("保存成功 → toast + drawer 关闭 + 重新加载详情", async () => {
    vi.mocked(thingApi.createProperty).mockResolvedValue({ result: { id: "p-new" } } as any);
    const host = hostStub({ name: "avg_power", expression: "{a.x} * 2", displayName: "平均功率" });
    mountDrawer(host);

    await saveDerivedEditor(host);

    expect(thingApi.createProperty).toHaveBeenCalledWith("t-host", expect.objectContaining({
      name: "avg_power",
      expression: "{a.x} * 2",
      displayName: "平均功率",
    }));
    expect(success).toHaveBeenCalledWith("派生属性已保存");
    expect(host.derivedEditor).toBeNull();
    expect(host.loadDeviceDetail).toHaveBeenCalledWith("t-host");
  });
});
