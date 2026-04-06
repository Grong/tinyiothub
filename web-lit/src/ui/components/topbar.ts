import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { icon } from '../icons'
import { toggleTheme } from '../theme'
import { logout } from '../controllers/auth'

export function renderTopbar(state: AppViewState) {
  return html`
    <header class="topbar">
      <div class="topbar-left">
        <input
          type="text"
          class="topbar-search"
          placeholder="Search..."
          .value=${state.searchQuery}
          @input=${(e: Event) => { state.searchQuery = (e.target as HTMLInputElement).value }}
        />
      </div>
      <div class="topbar-right">
        <button class="topbar-btn" @click=${() => { state.themeMode = toggleTheme(state.themeMode) }} title="Toggle theme">
          ${icon(state.themeMode === 'dark' ? 'sun' : 'moon')}
        </button>
        ${state.user ? html`
          <div class="topbar-user">
            <span class="topbar-username">${state.user.name}</span>
            <button class="topbar-btn" @click=${() => logout(state)} title="Logout">
              ${icon('logout')}
            </button>
          </div>
        ` : nothing}
      </div>
    </header>
  `
}
