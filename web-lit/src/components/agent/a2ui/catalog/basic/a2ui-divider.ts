import { LitElement, html} from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('a2ui-divider')
export class A2uiDivider extends LitElement {
  createRenderRoot() { return this }
  

  render() {
    return html`<hr />`
  }
}
