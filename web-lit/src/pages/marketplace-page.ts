import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('marketplace-page')
export class MarketplacePage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `
  render() {
    return html`<div>Marketplace Page - Placeholder</div>`
  }
}
