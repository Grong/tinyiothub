import { html, type TemplateResult } from "lit";
import { safeStr } from "./utils.js";

export function renderA2uiList(data: Record<string, unknown>): TemplateResult {
  const items = (data.items as Array<{ text: string; secondary?: string }>) || [];
  const ordered = Boolean(data.ordered);

  const listItems = items.map((item) => html`
    <li class="a2ui-list__item">
      <span class="a2ui-list__text">${safeStr(item.text, "")}</span>
      ${item.secondary ? html`<span class="a2ui-list__secondary">${safeStr(item.secondary, "")}</span>` : ""}
    </li>
  `);

  return ordered
    ? html`<ol class="a2ui-list">${listItems}</ol>`
    : html`<ul class="a2ui-list">${listItems}</ul>`;
}
