// web-lit/src/ui/theme.ts
export type ThemeMode = 'dark' | 'light'

const STORAGE_KEY = 'theme-mode'

export function getStoredTheme(): ThemeMode {
  const stored = localStorage.getItem(STORAGE_KEY)
  if (stored === 'light' || stored === 'dark') return stored
  return 'dark'
}

export function applyTheme(mode: ThemeMode): void {
  document.documentElement.setAttribute('data-theme-mode', mode)
  localStorage.setItem(STORAGE_KEY, mode)
}

export function initTheme(): ThemeMode {
  const mode = getStoredTheme()
  applyTheme(mode)
  return mode
}

export function toggleTheme(current: ThemeMode): ThemeMode {
  const next = current === 'dark' ? 'light' : 'dark'
  applyTheme(next)
  return next
}
