import { LitElement, html} from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('a2ui-text')
export class A2uiText extends LitElement {
  createRenderRoot() { return this }
  @property({ type: String }) text = ''
  @property({ type: String }) variant: 'body' | 'caption' | 'heading' = 'body'

  

  render() {
    return html`<span class="${this.variant}">${this.text}</span>`
  }
}
