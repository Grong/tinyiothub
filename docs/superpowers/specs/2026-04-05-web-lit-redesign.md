# TinyIoTHub Web-Lit 重设计规格

> **Date**: 2026-04-05
> **Status**: Draft
> **Supersedes**: N/A

## 1. Overview

**Goal**: Create a new frontend for TinyIoTHub using Lit Web Components, built from scratch with a clean architecture. The new frontend will be hosted at `/web-lit/` and run in parallel with the existing Next.js frontend until ready for production.

**Motivation**:
- Cross-framework component reuse (Web Components)
- Simplified architecture (no React/Next.js complexity)
- Smaller bundle size for edge deployment

**Reference Design**: OpenClaw Control UI (`/Users/chenguorong/code/github/openclaw/ui`)

---

## 2. Architecture

### 2.1 Directory Structure

```
tinyiothub/
├── web-lit/                      # New Lit frontend project
│   ├── src/
│   │   ├── components/           # Reusable Lit Web Components
│   │   │   ├── shell/           # Shell layout component
│   │   │   ├── header/          # Header component
│   │   │   ├── sidebar/         # Sidebar navigation
│   │   │   ├── button.ts
│   │   │   ├── input.ts
│   │   │   ├── card.ts
│   │   │   ├── modal.ts
│   │   │   ├── table.ts
│   │   │   ├── badge.ts
│   │   │   ├── tooltip.ts
│   │   │   └── ...
│   │   ├── pages/               # Page-level components
│   │   │   ├── home-page.ts
│   │   │   ├── signin-page.ts
│   │   │   ├── register-page.ts
│   │   │   ├── dashboard-page.ts
│   │   │   ├── devices-page.ts
│   │   │   ├── device-detail-page.ts
│   │   │   ├── alarms-page.ts
│   │   │   ├── monitoring-page.ts
│   │   │   ├── settings-page.ts
│   │   │   ├── tags-page.ts
│   │   │   ├── templates-page.ts
│   │   │   ├── marketplace-page.ts
│   │   │   └── installed-marketplace-page.ts
│   │   ├── services/            # API layer (symlink to web/service/)
│   │   ├── types/               # TypeScript types (symlink to web/types/)
│   │   ├── i18n/                # Internationalization (symlink to web/i18n/)
│   │   ├── stores/              # Reactive state management
│   │   │   ├── auth-store.ts
│   │   │   ├── app-store.ts
│   │   │   └── ...
│   │   ├── router/              # Client-side routing
│   │   │   └── index.ts
│   │   ├── styles/              # CSS system (copy from openclaw/ui)
│   │   │   ├── base.css
│   │   │   ├── layout.css
│   │   │   ├── components.css
│   │   │   └── ...
│   │   ├── app.ts               # Main app shell
│   │   └── main.ts              # Entry point
│   ├── index.html
│   ├── vite.config.ts
│   └── package.json
└── web/                         # Existing Next.js frontend (unchanged)
```

### 2.2 Reusable Assets

| Asset | Source | Method |
|-------|--------|--------|
| API Services (`web/service/`) | Existing | Symlink |
| Type Definitions (`web/types/`) | Existing | Symlink |
| i18n Resources (`web/i18n/`) | Existing | Symlink |
| CSS System | `openclaw/ui/src/styles/` | Copy + customize |
| Shell Layout | `openclaw/ui/src/ui/components/` | Copy + adapt |

### 2.3 Technology Stack

| Layer | Technology |
|-------|------------|
| Framework | Lit 3.x |
| Build Tool | Vite 8.x |
| State Management | Nanostores |
| Routing | @lit-labs/router |
| HTTP Client | ky (existing, from web/service) |
| Styling | CSS Modules + CSS Variables |
| i18n | i18next (existing, from web/i18n) |
| Testing | Vitest + Playwright |

---

## 3. Design System

### 3.1 Color Palette (from OpenClaw)

```css
:root {
  /* Background */
  --bg: #0e1015;
  --bg-accent: #13151b;
  --bg-elevated: #191c24;
  --bg-hover: #1f2330;

  /* Card/Surface */
  --card: #161920;
  --card-foreground: #f0f0f2;
  --card-highlight: rgba(255, 255, 255, 0.04);

  /* Text */
  --text: #d4d4d8;
  --text-strong: #f4f4f5;
  --muted: #838387;

  /* Border */
  --border: #1e2028;
  --border-strong: #2e3040;

  /* Accent (Red) */
  --accent: #ff5c5c;
  --accent-hover: #ff7070;
  --accent-subtle: rgba(255, 92, 92, 0.1);

  /* Semantic */
  --primary: #ff5c5c;
  --success: #4ade80;
  --warning: #fbbf24;
  --error: #f87171;
}
```

### 3.2 Shell Layout

```
┌─────────────────────────────────────────────────────────┐
│  Nav (258px)  │           Topbar (52px)                │
│                ├─────────────────────────────────────────┤
│                │                                         │
│   Sidebar      │              Content                    │
│   Navigation   │              Area                       │
│                │                                         │
│                │                                         │
└─────────────────────────────────────────────────────────┘
```

**CSS Variables**:
- `--shell-nav-width`: 258px
- `--shell-nav-rail-width`: 78px (collapsed)
- `--shell-topbar-height`: 52px
- `--shell-pad`: 16px
- `--shell-gap`: 16px

### 3.3 Typography

```css
font-family: system-ui, -apple-system, sans-serif;
font-size: 14px (base);
line-height: 1.5;
```

### 3.4 Spacing System

| Token | Value |
|-------|-------|
| `--space-1` | 4px |
| `--space-2` | 8px |
| `--space-3` | 12px |
| `--space-4` | 16px |
| `--space-6` | 24px |
| `--space-8` | 32px |

### 3.5 Component States

All interactive components must support:
- Default
- Hover
- Active/Pressed
- Focused (ring: 2px `--accent` outline, offset 2px)
- Disabled (opacity: 0.5, cursor: not-allowed)

**Focus ring**: All keyboard-focusable elements must show a visible focus indicator. No `outline: none` without replacement.

### 3.6 Interaction State Specifications

Every feature must implement all states below. "—" means state does not apply.

```
FEATURE                    | LOADING              | EMPTY                    | ERROR                    | SUCCESS                  | PARTIAL
--------------------------|---------------------|--------------------------|--------------------------|--------------------------|--------
Device List               | Skeleton rows (5)   | "No devices yet" + CTA  | Red banner + retry btn   | Toast + update list      | Filtered, no results
Device Detail             | Spinner in content  | N/A (single item)      | Red banner + back btn    | Auto-dismiss toast       | Some data missing
Alarm List                | Skeleton rows (5)   | "No alarms" + icon      | Red banner + retry btn   | Toast + highlight new    | Filtered, some dismissed
Dashboard Stats            | Skeleton cards (4)  | N/A (always has data)  | Gray out + "Offline"     | Auto-refresh in 30s      | Partial data
Sign-in Form              | Spinner in button   | N/A                     | Inline field errors       | Redirect to /dashboard   | Network timeout
Register Form             | Spinner in button   | N/A                     | Inline field errors       | Redirect to /signin     | Validation errors
Settings Form             | Spinner in save btn | N/A                     | Inline error + toast     | Toast + disable save     | Unsaved changes warn
Marketplace               | Skeleton cards (6)  | "Nothing found" + clear  | Red banner + retry btn   | Toast on install         | Some installable
Command Send              | Spinner in panel    | N/A                     | Red inline + retry       | Green toast              | Device offline warn
```

**Loading state: Skeleton pattern**
- Use animated pulse (opacity 0.5 → 1.0, 1.5s ease-in-out loop)
- Skeleton matches the shape of real content (table rows, cards, text lines)
- Never show spinners for content areas

**Empty state: Warm + Actionable**
- Centered illustration (simple SVG, IoT-themed: gateway, sensor, cloud)
- Friendly message: "No [items] yet" — not "No items found"
- Primary CTA button to add/create first item
- Secondary text explaining what these are for

**Error state: Recoverable**
- Red banner at top of affected area (not full-page overlay)
- Message: what failed + why (if known)
- Retry button (primary action)
- Error ID for support if persistent

**Success state: Feedback + Proceed**
- Toast notification (bottom-right, 4s auto-dismiss)
- Action confirmation text (not just "Success")
- Navigate or update affected list/panel

---

## 4. Page Routes

### 4.1 Shell Types

| Shell Type | Pages | Description |
|------------|-------|-------------|
| **Standalone** | `/`, `/signin`, `/tenant/register` | No sidebar, no topbar. Full-bleed pages for marketing and auth. |
| **App Shell** | All other routes | Sidebar (258px) + Topbar (52px) + Content. Authenticated workspace. |

### 4.2 Route Map

| Route | Component | Shell | Description |
|-------|-----------|-------|-------------|
| `/` | `HomePage` | Standalone | Landing page (marketing) |
| `/signin` | `SigninPage` | Standalone | Login |
| `/tenant/register` | `RegisterPage` | Standalone | Registration |
| `/dashboard` | `DashboardPage` | App Shell | Main dashboard |
| `/devices` | `DevicesPage` | App Shell | Device list |
| `/device-detail/:id` | `DeviceDetailPage` | App Shell | Device detail |
| `/alarms` | `AlarmsPage` | App Shell | Alarm management |
| `/monitoring` | `MonitoringPage` | App Shell | Real-time monitoring |
| `/settings` | `SettingsPage` | App Shell | User settings |
| `/tags` | `TagsPage` | App Shell | Tag management |
| `/templates` | `TemplatesPage` | App Shell | Device templates |
| `/marketplace` | `MarketplacePage` | App Shell | Plugin marketplace |
| `/installed-marketplace` | `InstalledMarketplacePage` | App Shell | Installed plugins |

### 4.3 Sidebar Navigation

**Primary Nav (always visible in sidebar)**:
- Dashboard (overview icon)
- Devices (device icon)
- Alarms (bell icon)
- Monitoring (chart icon)

**Secondary Nav (collapsible section)**:
- Templates (box icon)
- Marketplace (store icon)
- Installed Plugins (plug icon)

**Footer Nav**:
- Settings (gear icon)

**Navigation behavior**:
- Active item: `--accent` background tint, `--accent` text color
- Hover: `--bg-hover` background
- Collapsed mode (78px): icons only, tooltips on hover
- Mobile (<768px): drawer overlay, hamburger toggle in topbar

### 4.4 Responsive Breakpoints

| Viewport | Width | Shell Behavior |
|----------|-------|---------------|
| **Desktop** | ≥1024px | Full sidebar (258px), all labels visible |
| **Tablet** | 768px–1023px | Collapsed sidebar (78px), icons only, tooltips |
| **Mobile** | <768px | No sidebar, hamburger menu in topbar, drawer overlay |

**Tablet behavior**:
- Sidebar auto-collapses to 78px rail
- Labels hidden, icons + tooltips on hover
- Content area expands to fill space
- No drawer — always visible rail

**Mobile behavior**:
- Sidebar hidden entirely
- Topbar adds hamburger button (left)
- Sidebar renders as drawer (slides in from left, overlay backdrop)
- Drawer: full-height, 280px wide, backdrop click closes
- All nav items stack vertically

**Content reflow**:
- Dashboard: 4-col stats → 2-col → 1-col
- Device list: full table → card stack
- Forms: 2-col fields → single column

### 4.5 Accessibility

**Keyboard navigation**:
- `Tab` / `Shift+Tab`: move between interactive elements
- `Enter` / `Space`: activate buttons, toggle checkboxes
- `Escape`: close modals, dropdowns, drawers
- `Arrow keys`: navigate within dropdowns, tabs, table rows
- Sidebar items: focusable with visible focus ring (2px `--accent` outline)

**Focus management**:
- Modal opens: focus moves to first interactive element inside
- Modal closes: focus returns to trigger element
- Drawer opens: focus moves to close button or first item
- Page navigation: focus moves to `<main>` or page heading

**ARIA landmarks**:
```
<header>    — topbar
<nav>       — sidebar (role="navigation", aria-label="Main")
<main>      — content area (role="main")
<aside>     — secondary panels (e.g., device detail side panel)
<footer>    — page footer (if any)
```

**ARIA labels**:
- Sidebar nav: `aria-label="Main navigation"`
- Hamburger button: `aria-label="Open menu"`, `aria-expanded`, `aria-controls`
- Alarm badge: `aria-label="N unread alarms"`
- Modal: `role="dialog"`, `aria-modal="true"`, `aria-labelledby`

**Touch targets**:
- Minimum 44×44px for all interactive elements (buttons, links, inputs)
- Icon-only buttons: tooltips with visible text label

**Color contrast**:
- All text: minimum 4.5:1 contrast ratio (WCAG AA)
- Large text (≥18px): minimum 3:1
- Interactive element boundaries: 3:1 against adjacent colors
- Error/warning states: never rely on color alone — always paired with icon or text

**Screen reader**:
- Live regions for dynamic content: `aria-live="polite"` for toast notifications
- `aria-busy="true"` during loading states
- Form errors announced immediately on invalid submission

---

## 5. Component Library

### 5.1 Core Components (Phase 1)

| Component | Description |
|-----------|-------------|
| `<lit-button>` | Button with variants (primary, secondary, ghost, danger) |
| `<lit-input>` | Text input with label and error state |
| `<lit-card>` | Card container with header/body/footer |
| `<lit-modal>` | Dialog modal with backdrop |
| `<lit-table>` | Data table with sorting |
| `<lit-badge>` | Status badge |
| `<lit-tooltip>` | Tooltip overlay |
| `<lit-dropdown>` | Dropdown menu |
| `<lit-tabs>` | Tab navigation |
| `<lit-spinner>` | Loading spinner |

### 5.2 Layout Components (Phase 2)

| Component | Description |
|-----------|-------------|
| `<lit-shell>` | Main shell with nav + content |
| `<lit-header>` | Top header bar |
| `<lit-sidebar>` | Left navigation sidebar |
| `<lit-page>` | Page container with header |

### 5.3 IoT-Specific Components (Phase 3+)

| Component | Description |
|-----------|-------------|
| `<device-card>` | Device summary card |
| `<device-status>` | Device status indicator |
| `<data-chart>` | Time-series chart (Chart.js) |
| `<alarm-list>` | Alarm list with filters |
| `<protocol-badge>` | Protocol type badge (Modbus, MQTT, etc.) |

---

## 6. State Management

### 6.1 Stores (Nanostores)

```typescript
// auth-store.ts
interface AuthState {
  token: string | null;
  user: User | null;
  isAuthenticated: boolean;
}

// app-store.ts
interface AppState {
  sidebarCollapsed: boolean;
  currentPage: string;
  theme: 'dark' | 'light';
}
```

### 6.2 Data Flow

```
User Action
    ↓
Page Component (Lit)
    ↓
Store Action (nanostores)
    ↓
Service Layer (ky HTTP client)
    ↓
API Response
    ↓
Store Update
    ↓
UI Re-render
```

---

## 7. User Journeys

### 7.1 First-Time User Journey (Onboarding)

```
STEP | USER DOES                    | USER FEELS              | PLAN SPECIFIES
-----|------------------------------|-------------------------|-------------------
1    | Lands on /                  | Curious, evaluating     | Hero: "Edge Intelligence" tagline, value prop, CTA "Get Started"
2    | Clicks "Get Started"        | Ready to try            | Redirect to /tenant/register
3    | Fills registration form      | Anticipating setup     | Email + password, tenant name. No company size dropdown
4    | Submits → lands on /dashboard | Accomplished, curious  | Empty dashboard with onboarding banner: "Add your first device"
5    | Clicks "Add First Device"    | Empowered               | Opens device creation modal or /devices with create wizard
6    | Selects template or manual   | Focused                 | Template cards (Modbus, MQTT, ONVIF, SNMP) + manual option
7    | Configures device + tests     | Validating              | Test connection button, live status feedback
8    | Device online → sees data    | Excited, trusting       | Dashboard auto-refreshes, shows first data point + celebration toast
```

### 7.2 Returning Operator Journey (Daily Use)

```
STEP | USER DOES                    | USER FEELS              | PLAN SPECIFIES
-----|------------------------------|-------------------------|-------------------
1    | Lands on /dashboard          | habitual, efficient     | Stats cards visible immediately (no loading if cached)
2    | Skims alarm count badge      | Alert, assessing        | Red badge on Alarms nav if unread alarms. Badge count = urgency cue
3    | Clicks Alarms                | Focused, urgency        | Alarm list sorted by severity + time. Critical on top.
4    | Acknowledges alarm           | Relieved, responsible   | "Acknowledge" button → alarm moves to "Acknowledged" state, badge decrements
5    | Investigates device          | Analytical              | Clicks device → Device Detail with live metrics + command panel
6    | Sends command (e.g., reset) | Empowered, in control   | Confirmation dialog → spinner → success toast → device responds
```

### 7.3 First Device Empty State

When `/dashboard` has zero devices:

```
┌─────────────────────────────────────────────────────────┐
│                                                         │
│              [Gateway SVG illustration]                  │
│                                                         │
│              "No devices yet"                          │
│                                                         │
│       Your edge gateway is ready. Add your first        │
│       device to start monitoring.                       │
│                                                         │
│       [  Add First Device  ]  (primary button)          │
│                                                         │
│       Or [Browse Templates]  (ghost button)             │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### 7.4 Auth Flow

```
Sign-in (/signin):
- Single email + password form
- "Remember me" checkbox (persists session 30 days)
- Forgot password link → (future: reset flow)
- Error: inline field error, no full-page error
- Success: redirect to URL param `?redirect=` or /dashboard

Register (/tenant/register):
- Tenant name + email + password
- Terms acceptance checkbox
- Error: inline field errors
- Success: auto-login → redirect to /dashboard
```

---

## 8. Development Phases

### Phase 0: Scaffold (P0)
- [ ] Initialize `web-lit/` with Vite + Lit
- [ ] Copy CSS system from `openclaw/ui/src/styles/`
- [ ] Configure TypeScript
- [ ] Set up symlinks for `services/`, `types/`, `i18n/`
- [ ] Implement basic `<lit-shell>` layout
- [ ] Set up routing with `@lit-labs/router`
- [ ] Create placeholder pages for all routes

### Phase 1: Base Components (P0)
- [ ] Button component
- [ ] Input component
- [ ] Card component
- [ ] Modal component
- [ ] Table component
- [ ] Badge component
- [ ] Tooltip component
- [ ] Dropdown component
- [ ] Tabs component
- [ ] Spinner component

### Phase 2: Layout Components (P0)
- [ ] Shell component with responsive sidebar
- [ ] Header with user menu
- [ ] Sidebar navigation with icons
- [ ] Page container component

### Phase 3: Authentication (P1)
- [ ] Sign-in page
- [ ] Register page
- [ ] Auth store integration
- [ ] Protected route guard

### Phase 4: Dashboard (P1)
- [ ] Dashboard layout
- [ ] Stats cards
- [ ] Recent devices widget
- [ ] Alarm summary widget

### Phase 5: Device Management (P1)
- [ ] Device list page
- [ ] Device detail page
- [ ] Device status component
- [ ] Device command panel
- [ ] Monitoring charts

### Phase 6: Other Pages (P2)
- [ ] Alarms page with filters
- [ ] Settings page
- [ ] Tags page
- [ ] Templates page
- [ ] Marketplace page
- [ ] Installed marketplace page

---

## 9. Build & Deployment

### 8.1 Build Output
- Single HTML entry point
- Bundled JS/CSS assets
- Static hosting compatible (any CDN)

### 8.2 Dev Server
```bash
cd web-lit && pnpm dev
# Runs on http://localhost:5173
```

### 8.3 Production Build
```bash
cd web-lit && pnpm build
# Output in web-lit/dist/
```

---

## 10. Resolved Decisions

| Decision | Resolution | Rationale |
|----------|------------|-----------|
| **Authentication** | Reuse existing JWT auth from Rust backend | Same `/api/v1/auth/login` endpoint, store token in localStorage |
| **Real-time data** | Polling for MVP | Poll device data every 30s; WebSocket added in v2 if needed |
| **Charts** | Chart.js (lightweight) | ~60KB vs ECharts ~300KB; compatible with bundle size target |
| **SEO strategy** | Not applicable | Dashboard app, not a marketing site; no SSR needed |
| **API client deps** | Extract pure TS from `web/service/` | Remove React Query/Next.js dependencies; use ky directly |

### 9.1 API Client Adaptation

The existing `web/service/` layer has React Query and Next.js dependencies. Before symlinking:

1. Copy `web/service/` to `web-lit/src/services/`
2. Remove React Query wrappers, keep pure async functions
3. Replace `import from 'ky'` with direct fetch or ky standalone
4. Verify `web/types/` has no React types

---

## 11. Success Criteria

- [ ] All 14 pages implemented
- [ ] Base component library complete
- [ ] Responsive design (desktop + tablet)
- [ ] Authentication flow working
- [ ] API integration with Rust backend
- [ ] Bundle size < 300KB (gzipped) — using Chart.js (~60KB) instead of ECharts
- [ ] Playwright tests passing
