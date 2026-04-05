import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('templates-page')
export class TemplatesPage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `
  render() {
    return html`<div>Templates Page - Placeholder</div>`
  }
}
