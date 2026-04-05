import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('dashboard-page')
export class DashboardPage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `
  render() {
    return html`<div>Dashboard Page - Placeholder</div>`
  }
}
