import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('devices-page')
export class DevicesPage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `
  render() {
    return html`<div>Devices Page - Placeholder</div>`
  }
}
