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
