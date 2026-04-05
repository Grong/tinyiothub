import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('register-page')
export class RegisterPage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `
  render() {
    return html`<div>Register Page - Placeholder</div>`
  }
}
