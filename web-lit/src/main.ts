import { App } from './app'

const root = document.getElementById('app')
if (root) {
  const app = new App()
  root.appendChild(app)
}
