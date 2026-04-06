/**
 * A2UI Surface Manager
 * Manages component lifecycle within a single A2UI surface
 */

import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import type { A2uiComponentDescriptor } from '../../../types/agent-types'
import { hostStyles } from '../../../styles/shared-host'

@customElement('a2ui-surface')
export class A2uiSurface extends LitElement {
  @property({ type: String }) surfaceId = ''
  @property({ type: String }) title = ''
  @state() components: A2uiComponentDescriptor[] = []
  @state() dataModel: Record<string, unknown> = {}

  static styles = [hostStyles, css`
    :host { display: block; }
    .surface {
      background: var(--card);
      box-shadow: var(--glass-shadow-sm);
      border-radius: var(--radius);
      padding: 12px;
      overflow: hidden;
    }
    .surface-title {
      font-size: 0.8125rem;
      font-weight: 600;
      margin-bottom: 8px;
      color: var(--text-muted);
    }
    .components {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
  `]

  /** Set components externally (from message-group) */
  setComponents(components: A2uiComponentDescriptor[]) {
    this.components = components
  }

  /** Set data model externally */
  setDataModel(data: Record<string, unknown>) {
    this.dataModel = data
  }

  private _handleAction = (e: Event) => {
    const ce = e as CustomEvent
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: ce.detail,
      bubbles: true,
      composed: true,
    }))
  }

  render() {
    return html`
      <div class="surface" data-surface-id="${this.surfaceId}">
        ${this.title ? html`<div class="surface-title">${this.title}</div>` : ''}
        <div class="components" @a2ui-action="${this._handleAction}">
          ${this.components.map(comp =>
            html`<a2ui-component .descriptor="${comp}"></a2ui-component>`
          )}
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'a2ui-surface': A2uiSurface
  }
}
