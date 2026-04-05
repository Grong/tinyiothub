import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('settings-page')
export class SettingsPage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `
  render() {
    return html`<div>Settings Page - Placeholder</div>`
  }
}
