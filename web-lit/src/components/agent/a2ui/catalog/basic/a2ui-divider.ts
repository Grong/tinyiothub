import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'
import { hostStyles } from '../../../../../styles/shared-host'

@customElement('a2ui-divider')
export class A2uiDivider extends LitElement {
  static styles = [hostStyles, css`
    :host { display: block; }
    hr {
      border: none;
      box-shadow: 0 1px 0 var(--card-highlight);
      margin: 8px 0;
    }
  `]

  render() {
    return html`<hr />`
  }
}
