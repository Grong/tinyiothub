import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('device-detail-page')
export class DeviceDetailPage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `
  render() {
    return html`<div>Device Detail Page - Placeholder</div>`
  }
}
