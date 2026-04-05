import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('monitoring-page')
export class MonitoringPage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `
  render() {
    return html`<div>Monitoring Page - Placeholder</div>`
  }
}
