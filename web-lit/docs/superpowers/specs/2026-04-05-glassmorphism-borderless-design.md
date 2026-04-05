# Glassmorphism Borderless Design

> Date: 2026-04-05
> Scope: All UI elements — sidebar, topbar, cards, buttons, pills, callouts, form inputs, stat cards

## Goal

Transform the web-lit UI into a borderless glassmorphism design. Remove all visible borders, use semi-transparent backgrounds, subtle shadows, and `backdrop-filter: blur()` to establish visual hierarchy.

## Approach

**Pure color + subtle gradient** — keep existing dark/light theme base colors, add glass effects only where they create depth (sidebar, topbar). Cards and buttons lose borders but stay solid.

**Performance budget** — `backdrop-filter` only on sidebar and topbar (fixed/sticky elements). No blur on scrollable content cards.

## Changes

### 1. base.css — New CSS variables

Add glass-specific tokens:

```css
--glass-bg: rgba(14, 16, 21, 0.72);      /* dark sidebar glass */
--glass-bg-light: rgba(255, 255, 255, 0.06); /* card glass overlay */
--glass-shadow: 0 2px 16px rgba(0, 0, 0, 0.18);
--glass-shadow-sm: 0 1px 4px rgba(0, 0, 0, 0.12);
--glass-shadow-lg: 0 8px 32px rgba(0, 0, 0, 0.24);
```

Light mode overrides with appropriate values.

### 2. layout.css — Sidebar + Topbar

| Element | Remove | Add |
|---------|--------|-----|
| `.shell-nav` | `border-right` | — |
| `.sidebar` | solid bg | `background: color-mix(in srgb, var(--bg) 85%, transparent)` + `backdrop-filter: blur(16px)` |
| `.topbar` | `border-bottom` | `box-shadow: 0 1px 8px rgba(0,0,0,0.12)` |
| `.sidebar-shell__footer` | `border-top` | `box-shadow: 0 -1px 8px rgba(0,0,0,0.08)` |
| `.nav-item` | `border: 1px solid transparent` | borderless, use bg + shadow on hover/active |
| `.nav-item:hover` | `border-color` | `box-shadow` subtle |
| `.nav-item.active` | `border-color` | stronger bg + glow shadow |
| `.nav-collapse-toggle` | `border` | bg + shadow |
| `.sidebar-version` | `border` | bg + shadow |

### 3. components.css — Cards, Buttons, Pills, Inputs

| Element | Remove | Add |
|---------|--------|-----|
| `.card` | `border` | `box-shadow: 0 1px 3px rgba(0,0,0,0.12)` |
| `.card:hover` | `border-color` | deeper shadow |
| `.stat` | `border` | shadow |
| `.btn` | `border` | bg opacity + shadow on hover |
| `.btn.primary` | `border` | keep solid, shadow on hover |
| `.pill` | `border` | bg only |
| `.callout` | `border` | bg only |
| `.field input/textarea/select` | `border` | bottom-border only or bg tint |
| `.code-block` | `border` | bg only |

### 4. dashboard-page.ts — Inline styles

- `.stat-card`: remove `border`, add `box-shadow`
- `.card`: sync with global `.card` (already covered by components.css)
- `.card-header`: remove `border-bottom`, use shadow or spacing

### 5. app-header.ts — Inline styles

- `.btn-ghost`, `.btn-primary`: remove `border`
- `.dropdown`: remove `border`, use `box-shadow`
- `.topbar-btn`: remove `border`
- `.user-avatar`: no change (borderless already)

### 6. base-page.ts — Inline styles

- `.card`, `.btn`, `.form-input`: sync with global styles

## Files Modified

1. `src/styles/base.css` — CSS variables
2. `src/styles/layout.css` — Sidebar, topbar, nav
3. `src/styles/components.css` — Cards, buttons, pills, inputs
4. `src/pages/dashboard-page.ts` — Stat cards, cards
5. `src/components/app-header.ts` — Dropdown, buttons
6. `src/views/base-page.ts` — Card, button, form styles

## Verification

- `npm run build` passes
- Visual check: no visible borders in dark and light themes
- Hover/active states still provide clear feedback via shadows and bg changes
