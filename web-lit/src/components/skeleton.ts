// web-lit/src/components/skeleton.ts
import { LitElement, html} from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('skeleton')
export class Skeleton extends LitElement {
  createRenderRoot() { return this }
  

  @property({ type: String }) variant = 'text' // text, title, card

  render() {
    return html`<div class="skeleton skeleton-${this.variant}"></div>`
  }
}

declare global {
  interface HTMLElementTagNameMap { 'skeleton': Skeleton }
}
