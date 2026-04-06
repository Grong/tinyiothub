import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { apiPost } from '../api-client'
import { navigate } from '../../lib/navigate'

export function renderRegister(_state: AppViewState) {
  let error = ''
  let loading = false

  async function handleSubmit(e: Event) {
    e.preventDefault()
    const fd = new FormData(e.target as HTMLFormElement)
    const name = fd.get('name') as string
    const username = fd.get('username') as string
    const email = fd.get('email') as string
    const password = fd.get('password') as string
    loading = true
    error = ''
    try {
      await apiPost('auth/register', { name, username, password, email })
      navigate('/signin')
    } catch (err) {
      error = err instanceof Error ? err.message : 'Registration failed'
    } finally {
      loading = false
    }
  }

  return html`
    <div class="auth-page">
      <div class="auth-card">
        <h2>Register</h2>
        ${error ? html`<div class="callout callout-error">${error}</div>` : nothing}
        <form @submit=${handleSubmit}>
          <div class="form-group">
            <label for="name">Name</label>
            <input id="name" name="name" type="text" class="form-field" required />
          </div>
          <div class="form-group">
            <label for="username">Username</label>
            <input id="username" name="username" type="text" class="form-field" required />
          </div>
          <div class="form-group">
            <label for="email">Email</label>
            <input id="email" name="email" type="email" class="form-field" />
          </div>
          <div class="form-group">
            <label for="password">Password</label>
            <input id="password" name="password" type="password" class="form-field" required />
          </div>
          <button type="submit" class="btn btn-primary btn-full" ?disabled=${loading}>
            ${loading ? 'Registering...' : 'Register'}
          </button>
        </form>
        <p class="auth-footer">
          Already have an account? <a href="/signin" @click=${(e: Event) => { e.preventDefault(); navigate('/signin') }}>Sign In</a>
        </p>
      </div>
    </div>
  `
}
