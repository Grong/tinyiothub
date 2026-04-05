import './styles/base.css'
import './styles/layout.css'
import './styles/components.css'
import { App } from './app'

// Mount the app
const root = document.getElementById('app')
if (root) {
  const app = new App()
  root.appendChild(app)
}
