import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('signin-page')
export class SigninPage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `
  render() {
    return html`<div>Signin Page - Placeholder</div>`
  }
}
