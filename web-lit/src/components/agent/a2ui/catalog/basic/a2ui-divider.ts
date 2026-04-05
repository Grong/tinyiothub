import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('a2ui-divider')
export class A2uiDivider extends LitElement {
  static styles = css`
    :host { display: block; }
    hr {
      border: none;
      border-top: 1px solid var(--border, #e2e8f0);
      margin: 8px 0;
    }
  `

  render() {
    return html`<hr />`
  }
}
