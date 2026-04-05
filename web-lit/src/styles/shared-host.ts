/**
 * Shared host styles for Lit shadow DOM components.
 *
 * CSS custom properties (like --font-body) pierce shadow DOM automatically,
 * but regular properties (font-family, color) do NOT. Every shadow root
 * component must include this in its static styles to inherit typography.
 */
import { css } from 'lit'

export const hostStyles = css`
  :host {
    font-family: var(--font-body);
    color: var(--text);
  }
`
