import { html, type TemplateResult } from "lit";

export function renderA2uiText(data: Record<string, unknown>): TemplateResult {
  const text = String(data.text || "");
  const style = data.style as string | undefined;
  if (style === "heading") return html`<h3 class="a2ui-heading">${text}</h3>`;
  if (style === "subtitle") return html`<p class="a2ui-subtitle">${text}</p>`;
  if (style === "caption") return html`<small class="a2ui-caption">${text}</small>`;
  return html`<p class="a2ui-text">${text}</p>`;
}
