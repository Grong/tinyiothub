import { LitElement, html } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('a2ui-row')
export class A2uiRow extends LitElement {
  render() {
    return html`<div class="row"><slot></slot></div>`
  }
}
