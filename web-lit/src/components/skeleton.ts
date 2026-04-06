// web-lit/src/components/skeleton.ts
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import { hostStyles } from '../styles/shared-host'

@customElement('skeleton')
export class Skeleton extends LitElement {
  static styles = [hostStyles, css`
    :host { display: block; }
    .skeleton {
      background: linear-gradient(90deg, var(--card) 25%, var(--bg-hover) 50%, var(--card) 75%);
      background-size: 200% 100%;
      animation: skeleton-loading 1.5s infinite;
      border-radius: var(--radius-md);
    }
    @keyframes skeleton-loading {
      0% { background-position: 200% 0; }
      100% { background-position: -200% 0; }
    }
    .skeleton-text {
      height: 14px;
      margin-bottom: 8px;
    }
    .skeleton-title {
      height: 20px;
      width: 60%;
      margin-bottom: 12px;
    }
    .skeleton-card {
      height: 120px;
    }
  `]

  @property({ type: String }) variant = 'text' // text, title, card

  render() {
    return html`<div class="skeleton skeleton-${this.variant}"></div>`
  }
}

declare global {
  interface HTMLElementTagNameMap { 'skeleton': Skeleton }
}
