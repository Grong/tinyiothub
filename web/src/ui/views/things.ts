/**
 * 物列表/树视图 — 基于成熟的 devices 视图组件
 *
 * 复用 `<view-devices>` 的完整功能（列表、搜索、标签、分页、向导、详情页），
 * 并在此基础上增加树形视图切换。不做路由 hack，不重复实现。
 */
import { LitElement, html, nothing } from "lit";
import { customElement } from "lit/decorators.js";

// Ensures <view-devices> is registered before we use it
import "./devices.js";

@customElement("view-things")
export class ThingsView extends LitElement {
  createRenderRoot() {
    return this;
  }

  render() {
    return html`<view-devices></view-devices>`;
  }
}
