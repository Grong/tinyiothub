import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { login } from '../controllers/auth'
import { navigate } from '../../lib/navigate'

export function renderSignin(state: AppViewState) {
  let error = ''

  function handleSubmit(e: Event) {
    e.preventDefault()
    const fd = new FormData(e.target as HTMLFormElement)
    const username = fd.get('username') as string
    const password = fd.get('password') as string
    login(state, username, password).catch(err => {
      error = err instanceof Error ? err.message : 'Login failed'
    })
  }

  return html`
    <div class="auth-page">
      <div class="auth-card">
        <h2>Sign In</h2>
        ${error ? html`<div class="callout callout-error">${error}</div>` : nothing}
        <form @submit=${handleSubmit}>
          <div class="form-group">
            <label for="username">Username</label>
            <input id="username" name="username" type="text" class="form-field" required />
          </div>
          <div class="form-group">
            <label for="password">Password</label>
            <input id="password" name="password" type="password" class="form-field" required />
          </div>
          <button type="submit" class="btn btn-primary btn-full" ?disabled=${state.authLoading}>
            ${state.authLoading ? 'Signing in...' : 'Sign In'}
          </button>
        </form>
        <p class="auth-footer">
          Don't have an account? <a href="/register" @click=${(e: Event) => { e.preventDefault(); navigate('/register') }}>Register</a>
        </p>
      </div>
    </div>
  `
}
