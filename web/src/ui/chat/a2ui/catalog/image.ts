import { html, nothing, type TemplateResult } from "lit";

const BROKEN_SVG = html`
  <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="1.5">
    <rect x="6" y="10" width="36" height="28" rx="3"/>
    <circle cx="16" cy="20" r="3"/>
    <path d="M6 32l10-10 8 8 4-4 14 14"/>
  </svg>
`;

const VALID_FITS = new Set(["contain", "cover", "fill", "scale-down", "intrinsic"]);

/**
 * Standard A2UI Image component.
 *
 * Props:
 *   src     — image URL (required; empty → broken placeholder)
 *   alt     — alt text for accessibility
 *   fit     — object-fit: contain | cover | fill | scale-down | intrinsic (default: contain)
 *   width   — optional CSS width override
 *   height  — optional CSS height override
 *
 * CSS variables:
 *   --a2ui-image-max-height — default 60vh
 *
 * States: loading (fade-in), loaded, broken (no src), error (load failed).
 */
export function renderA2uiImage(
  data: Record<string, unknown>,
  _onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const src = String(data.src || "");
  const alt = String(data.alt || "");
  const fit = VALID_FITS.has(String(data.fit)) ? String(data.fit) : "contain";

  // Only set explicit overrides as inline styles; CSS handles defaults
  const w = data.width != null ? String(data.width) : "";
  const h = data.height != null ? String(data.height) : "";
  const inlineStyle = [w && `width:${w}`, h && `height:${h}`].filter(Boolean).join(";") || nothing;

  // ── No src: broken placeholder ──
  if (!src) {
    return html`
      <div class="a2ui-image a2ui-image--broken" role="img" aria-label=${alt || "图片"}>
        <span class="a2ui-image__placeholder">${BROKEN_SVG}</span>
      </div>
    `;
  }

  return html`
    <div class="a2ui-image">
      <div class="a2ui-image__wrap">
        <img
          class="a2ui-image__img a2ui-image__img--fit-${fit}"
          src=${src}
          alt=${alt}
          decoding="async"
          style=${inlineStyle}
          @load=${(e: Event) => (e.target as HTMLElement).classList.add("a2ui-image__img--loaded")}
          @error=${(e: Event) => {
            const img = e.target as HTMLElement;
            img.classList.add("a2ui-image__img--error");
            (img.closest(".a2ui-image") as HTMLElement)?.classList.add("a2ui-image--errored");
          }}
        />
        <span class="a2ui-image__error-fallback">
          <span class="a2ui-image__placeholder">${BROKEN_SVG}</span>
          <span class="a2ui-image__error-text">图片加载失败</span>
        </span>
      </div>
    </div>
  `;
}
