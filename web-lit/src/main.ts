import './styles/base.css'
import './styles/layout.css'
import './styles/components.css'
import { App } from './app'
import { initRouter } from './router'

// Mount the app shell
const root = document.getElementById('app')
if (root) {
  const app = new App()
  root.appendChild(app)

  // Initialize router after app is mounted
  // App uses light DOM (createRenderRoot returns this), so the app element IS the container
  initRouter(app)
}
