import { html, nothing, type TemplateResult } from "lit";

const BROKEN_SVG = html`
  <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="1.5">
    <rect x="6" y="10" width="36" height="28" rx="3"/>
    <circle cx="16" cy="20" r="3"/>
    <path d="M6 32l10-10 8 8 4-4 14 14"/>
  </svg>
`;

const EXPAND_SVG = html`
  <svg viewBox="0 0 20 20">
    <polyline points="8,4 16,4 16,12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
    <line x1="13" y1="7" x2="4" y2="16" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
  </svg>
`;

const CLOSE_SVG = html`
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20">
    <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
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
 * Lightbox: windowed overlay (not fullscreen), click backdrop or × to close.
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
      <button class="a2ui-image__trigger"
        @click=${(e: Event) => {
          const lb = (e.currentTarget as HTMLElement).parentElement?.querySelector(".a2ui-image-lightbox");
          if (lb) lb.classList.add("a2ui-image-lightbox--open");
        }}
        aria-label=${`查看大图: ${alt || src}`}>
        <img
          class="a2ui-image__img a2ui-image__img--fit-${fit}"
          src=${src}
          alt=${alt}
          loading="lazy"
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
        <span class="a2ui-image__expand">${EXPAND_SVG}</span>
      </button>

      <!-- Windowed lightbox — appended inline, positioned fixed via CSS -->
      <div class="a2ui-image-lightbox"
        @click=${(e: Event) => {
          if (e.target === e.currentTarget) {
            (e.currentTarget as HTMLElement).classList.remove("a2ui-image-lightbox--open");
          }
        }}>
        <button class="a2ui-image-lightbox__close"
          @click=${(e: Event) => {
            const lb = (e.target as HTMLElement).closest(".a2ui-image-lightbox");
            if (lb) lb.classList.remove("a2ui-image-lightbox--open");
          }}
          aria-label="关闭">
          ${CLOSE_SVG}
        </button>
        <img class="a2ui-image-lightbox__img" src=${src} alt=${alt} />
        ${alt ? html`<p class="a2ui-image-lightbox__caption">${alt}</p>` : nothing}
      </div>
    </div>
  `;
}
