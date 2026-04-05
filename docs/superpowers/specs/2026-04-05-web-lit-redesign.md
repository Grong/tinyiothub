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
- Focused (ring: `--ring`)
- Disabled (opacity: 0.5)

---

## 4. Page Routes

| Route | Component | Description |
|-------|-----------|-------------|
| `/` | `HomePage` | Landing page |
| `/signin` | `SigninPage` | Login |
| `/tenant/register` | `RegisterPage` | Registration |
| `/dashboard` | `DashboardPage` | Main dashboard |
| `/devices` | `DevicesPage` | Device list |
| `/device-detail/:id` | `DeviceDetailPage` | Device detail |
| `/alarms` | `AlarmsPage` | Alarm management |
| `/monitoring` | `MonitoringPage` | Real-time monitoring |
| `/settings` | `SettingsPage` | User settings |
| `/tags` | `TagsPage` | Tag management |
| `/templates` | `TemplatesPage` | Device templates |
| `/marketplace` | `MarketplacePage` | Plugin marketplace |
| `/installed-marketplace` | `InstalledMarketplacePage` | Installed plugins |

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
| `<data-chart>` | Time-series chart (ECharts) |
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

## 7. Development Phases

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

## 8. Build & Deployment

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

## 9. Open Questions

| Question | Status |
|----------|--------|
| SEO strategy (SSR/prerender)? | Open |
| Authentication backend integration? | Open |
| WebSocket for real-time data? | Open |
| ECharts for charts or lighter alternative? | Open |

---

## 10. Success Criteria

- [ ] All 14 pages implemented
- [ ] Base component library complete
- [ ] Responsive design (desktop + tablet)
- [ ] Authentication flow working
- [ ] API integration with Rust backend
- [ ] Bundle size < 200KB (gzipped)
- [ ] Playwright tests passing
