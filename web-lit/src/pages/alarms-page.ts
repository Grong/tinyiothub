import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('alarms-page')
export class AlarmsPage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `
  render() {
    return html`<div>Alarms Page - Placeholder</div>`
  }
}
