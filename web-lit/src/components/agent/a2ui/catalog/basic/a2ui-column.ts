import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('a2ui-column')
export class A2uiColumn extends LitElement {
  static styles = css`
    :host { display: block; }
    .column {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
  `

  render() {
    return html`<div class="column"><slot></slot></div>`
  }
}
