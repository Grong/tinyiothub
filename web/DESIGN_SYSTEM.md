# TinyIoTHub Web Design System

This document defines the design tokens and CSS architecture for the `web/` frontend.

---

## 1. CSS Architecture

### 1.1 No Inline CSS Monoliths

**Rule**: If a Lit component’s inline `<style>` exceeds ~200 lines, extract it into a dedicated `.css` file in the same directory and import it.

```ts
// Good
import "./home.css";

// Bad: 1400 lines of CSS inside render()
```

**Why**: Vite can optimize, cache, and compress standalone CSS files. Inline styles bloat JS bundles and hurt first paint.

### 1.2 View-Scoped Tokens

Views that need their own aesthetic layer (e.g., landing pages, dashboards) should define scoped custom properties under their root selector:

```css
view-home {
  --home-bg-deep: #02040a;
  --home-surface: rgba(10, 14, 22, 0.85);
  --home-accent-cyan: #00d4ff;
  --home-accent-violet: #7b61ff;
  --home-shadow-1: 0 4px 20px rgba(0,0,0,0.35);
  --home-shadow-2: 0 16px 60px rgba(0,0,0,0.28);
  --home-shadow-3: 0 40px 100px rgba(0,212,255,0.04);
  --home-shadow-inset: inset 0 0 0 1px rgba(255,255,255,0.04);
}
```

**Rule**: No hardcoded colors/shadows repeated 5+ times in a view. Tokenize them.

---

## 2. Token Reference

### Spacing Scale

| Token | Value | Usage |
|-------|-------|-------|
| `--space-1` | 4px | Tight gaps, icon margins |
| `--space-2` | 8px | Default inline gaps, small padding |
| `--space-3` | 12px | Card padding, list item gaps |
| `--space-4` | 16px | Section gaps, modal padding |
| `--space-5` | 20px | Form group gaps |
| `--space-6` | 24px | Page section margins |
| `--space-7` | 32px | Large section spacing |
| `--space-8` | 40px | Hero/page header spacing |
| `--space-9` | 60px | Full-page layout gaps |

### Radius Scale

| Token | Value | Usage |
|-------|-------|-------|
| `--radius-sm` | 6px | Small buttons, tags |
| `--radius-md` | 10px | Default cards, inputs |
| `--radius-lg` | 14px | Modals, panels |
| `--radius-xl` | 20px | Large cards, dialogs |
| `--radius-full` | 9999px | Pills, avatars |

### Duration Scale

| Token | Value | Usage |
|-------|-------|-------|
| `--duration-instant` | 0ms | Immediate state changes |
| `--duration-fast` | 120ms | Hover, focus, micro-interactions |
| `--duration-normal` | 200ms | Default transitions |
| `--duration-slow` | 350ms | Page transitions, modals |

### Easing Curves

| Token | Value | Usage |
|-------|-------|-------|
| `--ease-out` | `cubic-bezier(0.16, 1, 0.3, 1)` | Entrances, reveals |
| `--ease-in-out` | `cubic-bezier(0.4, 0, 0.2, 1)` | General transitions |
| `--ease-spring` | `cubic-bezier(0.34, 1.56, 0.64, 1)` | Playful interactions |

### Semantic Surfaces (Dark / Light)

| Token | Dark | Light |
|-------|------|-------|
| `--bg-subtle` | `rgba(255,255,255,0.03)` | `rgba(0,0,0,0.03)` |
| `--overlay-backdrop` | `rgba(0,0,0,0.5)` | `rgba(0,0,0,0.5)` |
| `--overlay-hover` | `rgba(0,0,0,0.2)` | `rgba(0,0,0,0.2)` |
| `--danger-fill-5` | `rgba(239,68,68,0.05)` | `rgba(220,38,38,0.05)` |
| `--danger-line-15` | `rgba(239,68,68,0.15)` | `rgba(220,38,38,0.15)` |
| `--muted-fill-10` | `rgba(107,114,128,0.10)` | `rgba(113,113,122,0.10)` |
| `--ok-fill-10` | `rgba(16,185,129,0.10)` | `rgba(22,163,74,0.10)` |

### Tuya Visual Language Tokens

The UI adopts a deep-space IoT dashboard aesthetic inspired by Tuya: dark navy canvases, cyan-to-violet accents, subtle glows, and glassmorphism.

| Token | Value | Usage |
|-------|-------|-------|
| `--accent-gradient` | `linear-gradient(135deg, #00d4ff 0%, #7b61ff 100%)` | Primary gradients: buttons, metric bars, stat values |
| `--accent-gradient-soft` | `linear-gradient(135deg, rgba(0,212,255,0.9) 0%, rgba(0,152,255,0.9) 60%, rgba(123,97,255,0.8) 100%)` | Hover states, softer glows |
| `--accent-glow` | `rgba(0, 212, 255, 0.35)` | Standard glow shadow color |
| `--accent-glow-strong` | `rgba(0, 212, 255, 0.45)` | Intense glows, pulsing dots |
| `--glass-bg` | `rgba(26, 29, 37, 0.65)` | Glass card backgrounds |
| `--glass-border` | `rgba(255, 255, 255, 0.08)` | Glass edge borders |
| `--glass-blur` | `blur(20px) saturate(180%)` | Backdrop-filter for glass panels |
| `--bg-deep-space` | layered radial gradients over `--bg` | Page background: faint cyan/purple orbs |

---

## 3. Visual Patterns

### 3.1 Deep Space Background

The global page background uses layered radial gradients to create an immersive dark-space atmosphere without hurting readability.

```css
body {
  background: var(--bg-deep-space);
}
```

`--bg-deep-space` is defined in `base.css` as two large radial orbs (cyan top-left, violet bottom-right) over the base `--bg` color. Opacity is kept very low (4-6%) so data remains the hero.

### 3.2 Gradient Border Cards

Cards that need a premium edge use a 1px gradient border implemented with `mask-composite: exclude`.

```css
.card--gradient-border {
  position: relative;
  background: var(--card);
  border-radius: var(--radius-lg);
}

.card--gradient-border::before {
  content: '';
  position: absolute;
  inset: 0;
  border-radius: inherit;
  padding: 1px;
  background: linear-gradient(135deg, rgba(255,255,255,0.12), rgba(255,255,255,0.03));
  -webkit-mask: linear-gradient(#fff 0 0) content-box, linear-gradient(#fff 0 0);
  -webkit-mask-composite: xor;
  mask-composite: exclude;
  pointer-events: none;
}
```

Used on: `.device-card`, `.template-card`, `.alarm-summary`, and any card that needs to feel "premium".

### 3.3 Glowing Status Dots

Status indicators are small gradient circles with soft neon glows and a slow pulse animation.

```css
.status-dot--success {
  background: linear-gradient(135deg, #00d4ff 0%, #22c55e 100%);
  box-shadow: 0 0 8px rgba(0, 212, 255, 0.5);
  animation: pulse-glow 2s ease-in-out infinite;
}
```

Variants: `--success`, `--warning` (amber-to-red), `--danger` (red), and `--glow` (accent cyan). Never use flat background colors for status dots in the Tuya style.

### 3.4 Metric Bar Shine

Progress/metric bars use the accent gradient plus a sweeping light reflection that animates continuously.

```css
.metric-bar__fill {
  background: var(--accent-gradient);
  box-shadow: 0 0 10px var(--accent-glow);
}

.metric-bar__fill::after {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(90deg, transparent, rgba(255,255,255,0.25), transparent);
  transform: translateX(-100%);
  animation: bar-shine 3s ease-in-out infinite;
}
```

### 3.5 Gradient Text for Stats

Large numbers and headings can use gradient text to draw attention without adding extra layout weight.

```css
.stat-card__value {
  background: var(--accent-gradient);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
}
```

### 3.6 Glassmorphism Panels

Floating panels (modals, toasts, dropdowns) use a semi-transparent dark background with heavy blur so they feel like they sit above the deep-space canvas.

```css
.glass-panel {
  background: var(--glass-bg);
  border: 1px solid var(--glass-border);
  backdrop-filter: var(--glass-blur);
  -webkit-backdrop-filter: var(--glass-blur);
}
```

### 3.7 Floating Card Shadow (Home-Panel Style)

For premium landing-page cards, use a 3-layer shadow stack plus a subtle inset highlight to create levitation without borders.

```css
.floating-card {
  background: rgba(10, 14, 22, 0.85);
  border: none;
  box-shadow:
    0 4px 20px rgba(0,0,0,0.35),
    0 16px 60px rgba(0,0,0,0.28),
    0 40px 100px rgba(0,212,255,0.04),
    inset 0 0 0 1px rgba(255,255,255,0.04);
  transition: all 0.3s ease;
}

.floating-card:hover {
  background: rgba(10, 14, 22, 0.95);
  transform: translateY(-4px);
  box-shadow:
    0 8px 30px rgba(0,0,0,0.4),
    0 20px 70px rgba(0,0,0,0.32),
    0 50px 120px rgba(0,212,255,0.06),
    inset 0 0 0 1px rgba(255,255,255,0.06);
}
```

**Rule**: Floating cards must not have visible `border` — the inset shadow creates the edge.

### 3.8 Header Glassmorphism

Fixed headers should gain a frosted-glass effect on scroll.

```css
.header--scrolled {
  background: rgba(2, 4, 10, 0.72);
  backdrop-filter: blur(20px) saturate(140%);
  -webkit-backdrop-filter: blur(20px) saturate(140%);
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  box-shadow: 0 4px 30px rgba(0, 0, 0, 0.25);
}
```

The scroll class must be dynamically applied via component state (e.g., `headerScrolled`) rather than relying solely on CSS `:hover` or `position: sticky` tricks.

---

## 4. Motion & Accessibility

### 4.1 Scroll-Triggered Reveals

Use `IntersectionObserver` to fade-in elements as they enter the viewport.

```css
.reveal {
  opacity: 0;
  transform: translateY(24px);
  transition: opacity 0.6s var(--ease-out), transform 0.6s var(--ease-out);
  will-change: opacity, transform;
}

.reveal.is-visible {
  opacity: 1;
  transform: translateY(0);
}

.reveal-delay-1 { transition-delay: 0.1s; }
.reveal-delay-2 { transition-delay: 0.2s; }
.reveal-delay-3 { transition-delay: 0.3s; }
```

**Rule**: Any view section that renders below the fold should be wrapped in `.reveal`.

### 4.2 Reduced Motion

All continuous animations (orbits, pulses, drifts, spins) must respect `prefers-reduced-motion`.

```css
@media (prefers-reduced-motion: reduce) {
  .sphere-scene,
  .orbit-particle,
  .ambient-orb,
  .reveal {
    animation: none !important;
    opacity: 1;
    transform: none;
    transition: none;
  }
}
```

---

## 5. Gradient Rules

### 5.1 Prefer Smooth 2-Stop Gradients

Avoid 3-stop gradients with hard middle anchors. They often create banding or muddy transitions.

```css
/* Good */
background: linear-gradient(135deg, #00d4ff 0%, #7b61ff 100%);

/* Bad: middle #0098FF at 50% creates a muddy transition zone */
background: linear-gradient(135deg, #00d4ff 0%, #0098FF 50%, #7b61ff 100%);
```

### 5.2 Brand Gradients Must Survive Theme Switch

Light theme should not drop the violet endpoint. Keep the brand gradient recognizable in both modes.

```css
/* Dark */
.btn--primary {
  background: linear-gradient(135deg, #00d4ff 0%, #7b61ff 100%);
}

/* Light */
.btn--primary {
  background: linear-gradient(135deg, #0099cc 0%, #7b61ff 100%);
}
```

---

## 6. CSS File Organization

`styles.css` is an import-only manifest. All app-specific rules live in partials under `src/styles/`.

```
src/styles/
  base.css          # Theme tokens, animations, global resets
  layout.css        # Shell layout, grid system
  layout.mobile.css # Responsive layout overrides
  components/       # Reusable UI components (modular)
    _index.css      # Import manifest
    animations.css  # Keyframes, entrance utilities
    buttons.css     # .btn variants, toggles
    cards.css       # .card, stats, chips
    forms.css       # .field, inputs, cron form
    tables.css      # .data-table, HTML tables
    modals.css      # .modal, .modal-overlay
    chat.css        # Chat message layouts
    agents.css      # Agent/model grids, dashboard
    a2ui.css        # A2UI surface components
  config.css        # Configuration UI patterns (cfg-toggle only)
  login.css         # Auth/login specific styles
  utilities.css     # Generic helpers (flex, gap, page headers, toasts)
  views/
    shared.css      # Data tables, modals, status badges, empty states
    models.css      # Model marketplace, filter bars
    monitoring.css  # Metrics, health lists, stats grids
    devices.css     # Device cards, detail views, command lists
    settings.css    # Settings tabs, API keys, skills panel
    alarms.css      # Alarm cards, alarm tables, summary widgets
    events.css      # Event list items
    wizard.css      # Wizard dialogs, template cards
```

### View-Local CSS

If a single view is complex enough (e.g., `view-home`), it may own a co-located `.css` file:

```
src/ui/views/
  home.ts
  home.css
  home-panel.ts
```

These files are imported directly by the view component and should be scoped with the view tag (e.g., `view-home .card`).

---

## 7. Rules

1. **No hardcoded pixel values in view CSS** except for:
   - `1px` borders
   - `max-width` breakpoints in `@media` queries
   - Canvas dimensions (`width: 100%; height: 280px;` for charts)

2. **All new views must add rules to the relevant partial** under `styles/views/{view}.css` or extend `styles/views/shared.css` for cross-cutting patterns.

3. **Use tokens first**: reach for `--space-*`, `--radius-*`, `--duration-*`, and semantic color tokens before inventing new values.

4. **Accessibility for overlays**: every modal / wizard / dialog must have:
   - `role="dialog"` and `aria-modal="true"`
   - Descriptive `aria-label`
   - `Escape` key support to close
   - Focus trap (Tab cycles within the dialog)
   - Focus restoration to the trigger element on close

5. **Performance hints**: apply `contain: layout style` to cards and `will-change` to animated properties (modal opacity, metric-bar width).

6. **Tuya visual language**: when adding premium surfaces, use the established deep-space + glow system:
   - Gradient borders via `::before` + `mask-composite: exclude`
   - Accent gradients (`--accent-gradient`) for primary actions, metric bars, and hero stats
   - Glowing status dots instead of flat colors
   - Keep glow opacity modest so text readability stays first

7. **No inline CSS monoliths**: A Lit component with >200 lines of CSS must import a dedicated `.css` file.

8. **Floating cards are borderless**: Use the 4-layer shadow stack (3 drop-shadows + 1 inset) instead of `border`.

9. **Respect reduced motion**: Any `animation` that runs continuously must be disabled inside `@media (prefers-reduced-motion: reduce)`.
