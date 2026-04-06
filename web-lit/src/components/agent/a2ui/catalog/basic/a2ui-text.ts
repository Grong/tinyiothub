import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import { hostStyles } from '../../../../../styles/shared-host'

@customElement('a2ui-text')
export class A2uiText extends LitElement {
  @property({ type: String }) text = ''
  @property({ type: String }) variant: 'body' | 'caption' | 'heading' = 'body'

  static styles = [hostStyles, css`
    :host { display: block; }
    .body { font-size: 0.875rem; line-height: 1.5; }
    .caption { font-size: 0.75rem; color: var(--text-muted); }
    .heading { font-size: 1rem; font-weight: 600; }
  `]

  render() {
    return html`<span class="${this.variant}">${this.text}</span>`
  }
}
