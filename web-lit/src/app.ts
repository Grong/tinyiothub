import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'

@customElement('tinyiothub-app')
export class App extends LitElement {
  static styles = css`
    :host {
      display: contents;
    }
  `

  @state() private _currentRoute = '/'

  connectedCallback() {
    super.connectedCallback()
    this._currentRoute = window.location.pathname
    window.addEventListener('popstate', () => {
      this._currentRoute = window.location.pathname
    })
  }

  render() {
    return html`<div>Hello TinyIoTHub</div>`
  }
}
