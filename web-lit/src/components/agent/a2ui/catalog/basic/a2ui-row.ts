import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('a2ui-row')
export class A2uiRow extends LitElement {
  static styles = css`
    :host { display: block; }
    .row {
      display: flex;
      flex-direction: row;
      gap: 8px;
      align-items: center;
    }
    ::slotted(*) { flex: 0 0 auto; }
  `

  render() {
    return html`<div class="row"><slot></slot></div>`
  }
}
