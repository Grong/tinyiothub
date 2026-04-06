import { html } from 'lit'
import type { AppViewState } from '../app-view-state'
import { navigate } from '../../lib/navigate'

export function renderHome(_state: AppViewState) {
  return html`
    <div class="home-page">
      <div class="home-hero">
        <h1>TinyIoTHub</h1>
        <p>Edge IoT Gateway Management Platform</p>
        <div class="home-actions">
          <button class="btn btn-primary" @click=${() => navigate('/signin')}>Sign In</button>
          <button class="btn btn-secondary" @click=${() => navigate('/register')}>Register</button>
        </div>
      </div>
      <div class="home-features">
        <div class="feature-card">
          <h3>Device Management</h3>
          <p>Manage IoT devices across multiple protocols</p>
        </div>
        <div class="feature-card">
          <h3>Real-time Monitoring</h3>
          <p>Monitor device status and performance in real-time</p>
        </div>
        <div class="feature-card">
          <h3>AI Agent</h3>
          <p>Intelligent agent for device management and automation</p>
        </div>
      </div>
    </div>
  `
}
