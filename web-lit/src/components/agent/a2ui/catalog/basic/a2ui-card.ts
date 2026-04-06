import { LitElement, html} from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('a2ui-card')
export class A2uiCard extends LitElement {
  createRenderRoot() { return this }
  @property({ type: String }) title = ''

  

  render() {
    return html`
      <div class="card">
        ${this.title ? html`<div class="card-title">${this.title}</div>` : ''}
        <slot></slot>
      </div>
    `
  }
}
