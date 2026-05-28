import { html, type TemplateResult } from "lit";

export function renderA2uiList(data: Record<string, unknown>): TemplateResult {
  const items = (data.items as Array<{ text: string; secondary?: string }>) || [];
  const ordered = Boolean(data.ordered);
  const tag = ordered ? "ol" : "ul";

  return html`
    <${tag} class="a2ui-list">
      ${items.map((item) => html`
        <li class="a2ui-list__item">
          <span class="a2ui-list__text">${item.text}</span>
          ${item.secondary ? html`<span class="a2ui-list__secondary">${item.secondary}</span>` : ""}
        </li>
      `)}
    </${tag}>
  `;
}
