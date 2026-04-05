# TinyIoTHub Frontend Architecture

## Overview

The frontend is built with **Lit** (Web Components) and follows a centralized state management pattern inspired by OpenClaw.

## Core Architecture

### State Management (Nanostores)

All application state is managed via **nanostores** atoms:

```
src/stores/
├── auth-store.ts    # Authentication state (token, user)
└── app-state.ts     # Application state (route, nav, notifications)
```

**Auth Store** (`auth-store.ts`):
- `$token` - JWT token (persisted to sessionStorage)
- `$user` - Current user object
- `$isAuthenticated` - Computed: !!token

**App State** (`app-state.ts`):
- `$currentRoute` - Current route name
- `$navCollapsed` - Sidebar collapsed state
- `$searchQuery` - Global search query
- `$alarmCount` - Unread alarm count
- `$notifications` - Notification list

### App Shell Pattern

The root `App` component (`app.ts`) provides the **shell** with CSS Grid:

```
.app-shell {
  display: grid;
  grid-template-columns: var(--shell-nav-width) minmax(0, 1fr);
  grid-template-rows: var(--shell-topbar-height) 1fr;
  grid-template-areas:
    "nav topbar"
    "nav content";
}
```

**Shell Areas**:
- `.shell-nav` - Left sidebar with navigation
- `.topbar` - Header with search, notifications, user menu
- `.content` - Main content area (swaps based on route)

### Page Components

Pages are separate custom elements in `src/pages/`:
- Each page is a `@customElement` LitElement
- Pages use nanostores directly for state
- Pages share styles via CSS variables

### Base Page Pattern (`views/base-page.ts`)

Base class providing:
- Loading/error/empty state rendering helpers
- Common button and form styles
- Page header rendering

### Routing

URL-based routing via `navigate()`:
- Updates `$currentRoute` store
- App re-renders based on route
- Public routes bypass auth check

## Directory Structure

```
src/
├── app.ts                    # Root component (shell)
├── main.ts                  # Entry point
├── views/
│   └── base-page.ts         # Base class for pages
├── pages/                   # Page components
│   ├── dashboard-page.ts
│   ├── devices-page.ts
│   └── ...
├── stores/                  # State management
│   ├── auth-store.ts
│   └── app-state.ts
├── services/                # API services
│   ├── auth.ts
│   ├── devices.ts
│   └── ...
├── lib/                      # Utilities
│   ├── api-client.ts
│   ├── navigate.ts
│   └── config.ts
├── styles/                  # CSS
│   ├── base.css
│   ├── layout.css
│   └── components.css
├── components/              # Shared components
│   └── logo-icon.ts
├── types/                   # TypeScript types
└── i18n/                    # Internationalization
```

## State Flow

```
User Action
    ↓
Component calls store.set() or navigate()
    ↓
Nanostore updates
    ↓
Subscribed components re-render
    ↓
UI Updates
```

## Key Patterns

### Reading State in Components
```typescript
import { $isAuthenticated, $user } from '../stores/auth-store'
import { $currentRoute, navigate } from '../stores/app-state'

// In render method
const isAuth = $isAuthenticated.get()
const user = $user.get()
const route = $currentRoute.get()
```

### Navigation
```typescript
import { navigate } from '../lib/navigate'

// Navigate to a route
navigate('dashboard')

// Update route without navigation
import { setCurrentRoute } from '../stores/app-state'
setCurrentRoute('settings')
```

### Protected Routes
```typescript
import { PUBLIC_ROUTES } from '../stores/app-state'

if (!PUBLIC_ROUTES.includes(route) && !$isAuthenticated.get()) {
  navigate('signin')
}
```

## Best Practices

1. **Don't duplicate state** - Use stores, don't copy state to local @state
2. **Subscribe to stores** - Use `.subscribe()` in `connectedCallback`
3. **Cleanup subscriptions** - Unsubscribe in `disconnectedCallback`
4. **Use CSS variables** - All colors/spacing via `--var(--name)` not hardcoded
5. **Centralize API calls** - Services in `src/services/`, not in components

## Theme System

CSS variables defined in `styles/base.css`:
- `--bg`, `--card`, `--border` - Backgrounds
- `--text`, `--text-strong`, `--muted` - Text colors
- `--accent`, `--accent-hover` - Brand color
- `--ok`, `--warn`, `--danger` - Status colors

## Build & Dev

```bash
npm run dev    # Start dev server (port 5173)
npm run build  # Production build
```
