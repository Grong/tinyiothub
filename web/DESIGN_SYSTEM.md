# TinyIoTHub Web Design System

This document defines the design tokens and CSS architecture for the `web/` frontend.

## Token Reference

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
| `--accent-gradient` | `linear-gradient(135deg, #00d4ff 0%, #0098FF 50%, #7b61ff 100%)` | Primary gradients: buttons, metric bars, stat values |
| `--accent-gradient-soft` | `linear-gradient(135deg, rgba(0,212,255,0.9) 0%, rgba(0,152,255,0.9) 60%, rgba(123,97,255,0.8) 100%)` | Hover states, softer glows |
| `--accent-glow` | `rgba(0, 212, 255, 0.35)` | Standard glow shadow color |
| `--accent-glow-strong` | `rgba(0, 212, 255, 0.45)` | Intense glows, pulsing dots |
| `--glass-bg` | `rgba(26, 29, 37, 0.65)` | Glass card backgrounds |
| `--glass-border` | `rgba(255, 255, 255, 0.08)` | Glass edge borders |
| `--glass-blur` | `blur(20px) saturate(180%)` | Backdrop-filter for glass panels |
| `--bg-deep-space` | layered radial gradients over `--bg` | Page background: faint cyan/purple orbs |

## Visual Patterns

### Deep Space Background

The global page background uses layered radial gradients to create an immersive dark-space atmosphere without hurting readability.

```css
body {
  background: var(--bg-deep-space);
}
```

`--bg-deep-space` is defined in `base.css` as two large radial orbs (cyan top-left, violet bottom-right) over the base `--bg` color. Opacity is kept very low (4-6%) so data remains the hero.

### Gradient Border Cards

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

### Glowing Status Dots

Status indicators are small gradient circles with soft neon glows and a slow pulse animation.

```css
.status-dot--success {
  background: linear-gradient(135deg, #00d4ff 0%, #22c55e 100%);
  box-shadow: 0 0 8px rgba(0, 212, 255, 0.5);
  animation: pulse-glow 2s ease-in-out infinite;
}
```

Variants: `--success`, `--warning` (amber-to-red), `--danger` (red), and `--glow` (accent cyan). Never use flat background colors for status dots in the Tuya style.

### Metric Bar Shine

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

### Gradient Text for Stats

Large numbers and headings can use gradient text to draw attention without adding extra layout weight.

```css
.stat-card__value {
  background: var(--accent-gradient);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
}
```

### Glassmorphism Panels

Floating panels (modals, toasts, dropdowns) use a semi-transparent dark background with heavy blur so they feel like they sit above the deep-space canvas.

```css
.glass-panel {
  background: var(--glass-bg);
  border: 1px solid var(--glass-border);
  backdrop-filter: var(--glass-blur);
  -webkit-backdrop-filter: var(--glass-blur);
}
```

## CSS File Organization

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

## Rules

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
