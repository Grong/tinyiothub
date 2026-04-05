import { atom } from 'nanostores'

export const $sidebarCollapsed = atom<boolean>(false)
export const $theme = atom<'dark' | 'light'>('dark')

export function toggleSidebar() {
  $sidebarCollapsed.set(!$sidebarCollapsed.get())
}

export function setTheme(theme: 'dark' | 'light') {
  $theme.set(theme)
}
