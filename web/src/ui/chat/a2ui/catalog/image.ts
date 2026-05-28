import { html, type TemplateResult } from "lit";

export function renderA2uiImage(data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void): TemplateResult {
  const src = String(data.src || "");
  const alt = String(data.alt || "");
  const width = data.width ? String(data.width) : undefined;
  const height = data.height ? String(data.height) : undefined;
  const fit = String(data.fit || "cover");
  const clickable = Boolean(data.clickable);

  if (!src) {
    return html`
      <div class="a2ui-image a2ui-image--broken" style=${[width && `width:${width}`, height && `height:${height}`].filter(Boolean).join(";") || ""}>
        <span class="a2ui-image__placeholder">
          <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="6" y="10" width="36" height="28" rx="3"/><circle cx="16" cy="20" r="3"/><path d="M6 32l10-10 8 8 4-4 14 14"/></svg>
        </span>
      </div>
    `;
  }

  const img = html`
    <img class="a2ui-image__img a2ui-image__img--fit-${fit}"
      src=${src} alt=${alt}
      style=${[width && `width:${width}`, height && `height:${height}`].filter(Boolean).join(";") || ""}
      @load=${(e: Event) => (e.target as HTMLElement).classList.add("a2ui-image__img--loaded")}
      @error=${(e: Event) => (e.target as HTMLElement).classList.add("a2ui-image__img--error")}
    />
  `;

  if (clickable) {
    return html`
      <button class="a2ui-image a2ui-image--clickable" @click=${() => { if (onAction) onAction("openImage", { src }); }}>
        ${img}
        <span class="a2ui-image__expand">
          <svg viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg"><polyline points="8,4 16,4 16,12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><line x1="13" y1="7" x2="4" y2="16" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
        </span>
      </button>
    `;
  }

  return html`<div class="a2ui-image">${img}</div>`;
}
