/**
 * A2UI Component Factory
 * Dynamically renders a single A2UI component by type
 */

import { LitElement, html, css, nothing } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { A2uiComponentDescriptor } from '../../../types/agent-types'
import { getTagName } from './catalog/index'
import { hostStyles } from '../../../styles/shared-host'

@customElement('a2ui-component')
export class A2uiComponent extends LitElement {
  @property({ type: Object }) descriptor: A2uiComponentDescriptor | null = null

  private _el: Element | null = null
  private _lastDescriptor: A2uiComponentDescriptor | null = null
  private _boundHandleAction: ((e: Event) => void) | null = null

  static styles = [hostStyles, css`
    :host { display: block; }
    .unknown {
      padding: 8px;
      background: var(--bg-elevated);
      border: 1px dashed var(--border);
      border-radius: 4px;
      font-size: 0.75rem;
      color: var(--text-muted);
    }
  `]

  private _cleanup() {
    if (this._el && this._boundHandleAction) {
      this._el.removeEventListener('a2ui-action', this._boundHandleAction)
    }
    this._el = null
    this._boundHandleAction = null
  }

  private _buildElement() {
    this._cleanup()

    if (!this.descriptor) return

    const { type, props, children, id } = this.descriptor
    const tagName = getTagName(type)

    if (!tagName) {
      this._el = document.createElement('div')
      this._el.className = 'unknown'
      this._el.textContent = `[Unknown: ${type}]`
      this._lastDescriptor = this.descriptor
      return
    }

    const el = document.createElement(tagName)
    if (props) {
      for (const [key, value] of Object.entries(props)) {
        (el as any)[key] = value
      }
    }
    el.setAttribute('data-a2ui-id', id)

    // Delegate a2ui-action events upward — single listener, cleaned up on rebuild
    this._boundHandleAction = (e: Event) => {
      const ce = e as CustomEvent
      this.dispatchEvent(new CustomEvent('a2ui-action', {
        detail: {
          componentId: id,
          ...ce.detail,
        },
        bubbles: true,
        composed: true,
      }))
    }
    el.addEventListener('a2ui-action', this._boundHandleAction)

    // Render children inside slotted content
    if (children && children.length > 0) {
      for (const childDesc of children) {
        const childEl = document.createElement('a2ui-component') as A2uiComponent
        childEl.descriptor = childDesc
        el.appendChild(childEl)
      }
    }

    this._el = el
    this._lastDescriptor = this.descriptor
  }

  updated() {
    if (this.descriptor !== this._lastDescriptor) {
      this._buildElement()
      if (this._el) {
        // Replace Lit's comment placeholder with the real element
        const container = this.querySelector('.host')
        if (container) {
          // Use remove() instead of innerHTML = '' so child LitElements
          // fire disconnectedCallback and clean up their listeners
          while (container.firstChild) {
            container.removeChild(container.firstChild)
          }
          container.appendChild(this._el)
        }
      }
    }
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    this._cleanup()
  }

  render() {
    if (!this.descriptor) return nothing
    const { type } = this.descriptor
    const tagName = getTagName(type)
    if (!tagName) {
      return html`<div class="unknown">[Unknown: ${type}]</div>`
    }
    return html`<div class="host"></div>`
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'a2ui-component': A2uiComponent
  }
}
