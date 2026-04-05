import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'
import { destroyRouter } from './router'

@customElement('tinyiothub-app')
export class App extends LitElement {
  // Use light DOM so this element IS the container
  createRenderRoot() {
    return this
  }

  static styles = css`
    :host {
      display: block;
      min-height: 100vh;
    }
    .app-shell {
      display: flex;
      min-height: 100vh;
    }
    .sidebar {
      width: 240px;
      background: var(--bg-secondary, #1a1a1a);
      flex-shrink: 0;
    }
    .main-content {
      flex: 1;
      display: flex;
      flex-direction: column;
      min-width: 0;
    }
    .topbar {
      height: 56px;
      background: var(--bg-primary, #0a0a0a);
      border-bottom: 1px solid var(--border-color, #2a2a2a);
      display: flex;
      align-items: center;
      padding: 0 24px;
      font-weight: 600;
    }
    .content {
      flex: 1;
      padding: 24px;
    }
  `

  disconnectedCallback() {
    super.disconnectedCallback()
    destroyRouter()
  }

  render() {
    return html`
      <div class="app-shell">
        <div class="sidebar">Sidebar</div>
        <div class="main-content">
          <header class="topbar">TinyIoTHub</header>
          <main class="content"></main>
        </div>
      </div>
    `
  }
}
