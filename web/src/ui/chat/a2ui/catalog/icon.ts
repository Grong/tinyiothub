import { html, svg, type TemplateResult } from "lit";

type SvgIconDef = [string, TemplateResult];

const SVG_ICONS: Record<string, SvgIconDef> = {
  info:        ["0 0 20 20", svg`<circle cx="10" cy="10" r="9" fill="none" stroke="currentColor" stroke-width="1.5"/><line x1="10" y1="6" x2="10" y2="6" stroke="currentColor" stroke-width="2" stroke-linecap="round"/><line x1="10" y1="9" x2="10" y2="14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>`],
  warning:     ["0 0 20 20", svg`<path d="M10 3L19 18H1L10 3z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><line x1="10" y1="8" x2="10" y2="12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><circle cx="10" cy="15" r="1" fill="currentColor"/>`],
  error:       ["0 0 20 20", svg`<circle cx="10" cy="10" r="9" fill="none" stroke="currentColor" stroke-width="1.5"/><line x1="7" y1="7" x2="13" y2="13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><line x1="13" y1="7" x2="7" y2="13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>`],
  success:     ["0 0 20 20", svg`<circle cx="10" cy="10" r="9" fill="none" stroke="currentColor" stroke-width="1.5"/><polyline points="6,10 9,13 14,7" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>`],
  settings:    ["0 0 20 20", svg`<circle cx="10" cy="10" r="3" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M10 2v2m0 12v2M2 10h2m12 0h2M4.5 4.5l1.5 1.5m8 8l1.5 1.5M4.5 15.5l1.5-1.5m8-8l1.5-1.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>`],
  refresh:     ["0 0 20 20", svg`<path d="M17 10a7 7 0 01-14 0 7 7 0 0114 0z" fill="none" stroke="currentColor" stroke-width="1.5"/><polyline points="13,7 17,10 13,13" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>`],
  delete:      ["0 0 20 20", svg`<path d="M5 6h10M8 6V4a1 1 0 011-1h2a1 1 0 011 1v2M7 6v11a1 1 0 001 1h4a1 1 0 001-1V6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>`],
  edit:        ["0 0 20 20", svg`<path d="M13 4l3 3L7 16H4v-3L13 4z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>`],
  add:         ["0 0 20 20", svg`<line x1="10" y1="4" x2="10" y2="16" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><line x1="4" y1="10" x2="16" y2="10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>`],
  search:      ["0 0 20 20", svg`<circle cx="9" cy="9" r="5" fill="none" stroke="currentColor" stroke-width="1.5"/><line x1="13" y1="13" x2="17" y2="17" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>`],
  lock:        ["0 0 20 20", svg`<rect x="5" y="9" width="10" height="8" rx="1" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M7 9V6a3 3 0 016 0v3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>`],
  unlock:      ["0 0 20 20", svg`<rect x="5" y="9" width="10" height="8" rx="1" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M7 9V6a3 3 0 015-2.6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>`],
  user:        ["0 0 20 20", svg`<circle cx="10" cy="7" r="4" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M3 19v-1a6 6 0 0112 0v1" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>`],
  clock:       ["0 0 20 20", svg`<circle cx="10" cy="10" r="8" fill="none" stroke="currentColor" stroke-width="1.5"/><polyline points="10,6 10,10 13,13" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>`],
  chart:       ["0 0 20 20", svg`<rect x="3" y="12" width="3" height="5" rx="0.5" fill="none" stroke="currentColor" stroke-width="1.5"/><rect x="8" y="7" width="3" height="10" rx="0.5" fill="none" stroke="currentColor" stroke-width="1.5"/><rect x="13" y="3" width="3" height="14" rx="0.5" fill="none" stroke="currentColor" stroke-width="1.5"/>`],
  bell:        ["0 0 20 20", svg`<path d="M8 17h4M5 9a5 5 0 0110 0v4l1 2H4l1-2V9z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>`],
  star:        ["0 0 20 20", svg`<polygon points="10,2 12.5,7.5 18.5,8 14,12 15.5,18 10,14.5 4.5,18 6,12 1.5,8 7.5,7.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>`],
  menu:        ["0 0 20 20", svg`<line x1="4" y1="6" x2="16" y2="6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><line x1="4" y1="10" x2="16" y2="10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><line x1="4" y1="14" x2="16" y2="14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>`],
  close:       ["0 0 20 20", svg`<line x1="6" y1="6" x2="14" y2="14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><line x1="14" y1="6" x2="6" y2="14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>`],
  check:       ["0 0 20 20", svg`<polyline points="5,10 9,14 15,6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>`],
  arrowUp:     ["0 0 20 20", svg`<polyline points="6,12 10,7 14,12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>`],
  arrowDown:   ["0 0 20 20", svg`<polyline points="6,8 10,13 14,8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>`],
  arrowLeft:   ["0 0 20 20", svg`<polyline points="12,6 7,10 12,14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>`],
  arrowRight:  ["0 0 20 20", svg`<polyline points="8,6 13,10 8,14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>`],
  wifi:        ["0 0 20 20", svg`<path d="M10 14a1.5 1.5 0 010 3h0" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/><path d="M7 11a5 5 0 016 0" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><path d="M4 8a10 10 0 0112 0" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>`],
  bluetooth:   ["0 0 20 20", svg`<polyline points="7,6 13,14 7,14 13,6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>`],
  battery:     ["0 0 20 20", svg`<rect x="3" y="6" width="14" height="9" rx="2" fill="none" stroke="currentColor" stroke-width="1.5"/><line x1="17" y1="9" x2="17" y2="12" stroke="currentColor" stroke-width="2" stroke-linecap="round"/><rect x="5" y="8" width="3" height="5" rx="0.5" fill="currentColor"/>`],
  thermometer: ["0 0 20 20", svg`<circle cx="7" cy="14" r="3" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M7 12V4a2 2 0 014 0v8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>`],
};

export function renderA2uiIcon(data: Record<string, unknown>): TemplateResult {
  const name = String(data.name || "info");
  const size = String(data.size || "md");
  const custom = data.custom as string | undefined;

  // Allow custom text/emoji override
  if (custom) {
    return html`<span class="a2ui-icon a2ui-icon--${size} a2ui-icon--custom">${custom}</span>`;
  }

  const def = SVG_ICONS[name];
  if (def) {
    const [viewBox, content] = def;
    return html`<span class="a2ui-icon a2ui-icon--${size}">
      <svg viewBox=${viewBox} xmlns="http://www.w3.org/2000/svg" class="a2ui-icon__svg">${content}</svg>
    </span>`;
  }

  // Fallback: try emoji map for backward compat
  const EMOJI_FALLBACK: Record<string, string> = {
    home: "⌂", heart: "♥", link: "🔗", download: "⬇", upload: "⬆", copy: "📋",
  };
  const fallback = EMOJI_FALLBACK[name] || name;
  return html`<span class="a2ui-icon a2ui-icon--${size} a2ui-icon--emoji">${fallback}</span>`;
}
