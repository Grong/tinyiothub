// Page component props and state types

export interface PageProps {
  // Navigation
  route: string
  navigate: (route: string) => void

  // Auth state (from app)
  isAuthenticated: boolean
  user: UserInfo | null

  // App-level state
  navCollapsed: boolean
  alarmCount: number
  searchQuery: string

  // App actions
  toggleNav: () => void
  setSearchQuery: (q: string) => void
  logout: () => void
}

export interface UserInfo {
  id: string
  name: string
  email?: string
  phone?: string
  avatar?: string
}

export interface PageState {
  loading: boolean
  error: string | null
}

// Loading, empty, error states for pages
export interface PageRenderContext {
  isLoading: boolean
  error: string | null
  empty: boolean
  emptyMessage?: string
}
