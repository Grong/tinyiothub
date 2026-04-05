import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('tags-page')
export class TagsPage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `
  render() {
    return html`<div>Tags Page - Placeholder</div>`
  }
}
