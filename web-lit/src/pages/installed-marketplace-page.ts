import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('installed-marketplace-page')
export class InstalledMarketplacePage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `
  render() {
    return html`<div>Installed Marketplace Page - Placeholder</div>`
  }
}
