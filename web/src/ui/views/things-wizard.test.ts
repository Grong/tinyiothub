/**
 * 建物向导模板过滤测试（Fix B，2026-10-20）。
 *
 * 锁的是：场景包（category='scenes'）不出现在"新建物选模板"候选中——
 * 场景包是组合模板（子树蓝图），只能走市场实例化；选它建单物语义错位
 * （实测：选园区模板建物看不到属性骨架，触发本次修复）。
 * 非场景包模板（含边界：空 category、null、大小写变体外的精确匹配）不受影响。
 */
import { describe, expect, it } from "vitest";
import { wizardSelectableTemplates } from "./things-wizard.js";

const tpl = (id: string, category?: string | null) => ({ id, category });

describe("wizardSelectableTemplates", () => {
  it("过滤 category='scenes' 的场景包", () => {
    const out = wizardSelectableTemplates([
      tpl("campus", "scenes"),
      tpl("meter", "sensors"),
      tpl("floor", "scenes"),
    ]);
    expect(out.map(t => t.id)).toEqual(["meter"]);
  });

  it("无场景包时原样保留（顺序不变）", () => {
    const out = wizardSelectableTemplates([tpl("a", "sensors"), tpl("b", "controllers"), tpl("c", "others")]);
    expect(out.map(t => t.id)).toEqual(["a", "b", "c"]);
  });

  it("空/缺失 category 不过滤", () => {
    const out = wizardSelectableTemplates([tpl("a", ""), tpl("b", null), tpl("c", undefined), tpl("d")]);
    expect(out).toHaveLength(4);
  });

  it("空输入返回空", () => {
    expect(wizardSelectableTemplates([])).toEqual([]);
  });
});
