import { LitElement, html} from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('a2ui-column')
export class A2uiColumn extends LitElement {
  createRenderRoot() { return this }
  

  render() {
    return html`<div class="column"><slot></slot></div>`
  }
}
