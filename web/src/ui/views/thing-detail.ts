/**
 * 物详情页 — 基于成熟的 devices 视图组件
 *
 * `<view-devices>` 根据 URL 路径自动判断列表/详情模式。
 * 当 URL 为 /things/:id 时，自动展示详情视图（四 Tab + 属性/事件/动作/知识）。
 */
import { LitElement, html } from "lit";
import { customElement } from "lit/decorators.js";
import "./devices.js";

@customElement("view-thing-detail")
export class ThingDetailView extends LitElement {
  createRenderRoot() {
    return this;
  }

  render() {
    return html`<view-devices></view-devices>`;
  }
}
