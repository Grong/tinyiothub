import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { hostStyles } from '../../../../styles/shared-host'

interface ControlDef {
  id: string
  type: 'toggle' | 'slider' | 'button'
  label: string
  value: unknown
  min?: number
  max?: number
  options?: string[]
}

@customElement('control-panel')
export class ControlPanel extends LitElement {
  @property({ type: Array }) controls: ControlDef[] = []
  // Loading state for button controls — set externally via setLoading()
  @state() private loading = false

  setLoading(v: boolean) {
    this.loading = v
  }

  static styles = [hostStyles, css`
    :host { display: block; }
    .panel {
      background: var(--card);
      box-shadow: var(--glass-shadow-sm);
      border-radius: var(--radius);
      padding: 12px;
    }
    .control-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      margin-bottom: 12px;
    }
    .control-label { font-size: 0.875rem; font-weight: 500; }
    .toggle {
      width: 40px;
      height: 22px;
      border-radius: 11px;
      border: none;
      cursor: pointer;
      position: relative;
      transition: background 0.2s;
    }
    .toggle.on { background: var(--ok); }
    .toggle.off { background: var(--border); }
    .toggle::after {
      content: '';
      position: absolute;
      top: 2px;
      width: 18px;
      height: 18px;
      border-radius: 50%;
      background: var(--card);
      transition: left 0.2s;
    }
    .toggle.on::after { left: 20px; }
    .toggle.off::after { left: 2px; }
    .slider-row {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-bottom: 12px;
    }
    .slider-row input[type="range"] { flex: 1; }
    .slider-value { font-size: 0.75rem; font-family: monospace; min-width: 40px; text-align: right; }
    .button-row { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 12px; }
    .button-row button {
      padding: 5px 12px;
      border-radius: 4px;
      background: transparent;
      cursor: pointer;
      font-size: 0.8125rem;
      color: var(--text);
    }
    .button-row button:hover { background: var(--bg-elevated); }
    .button-row button:disabled { opacity: 0.5; cursor: not-allowed; }
  `]

  private _emitAction(control: ControlDef, value: unknown) {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: control.type, id: control.id, type: control.type, value },
      bubbles: true, composed: true,
    }))
  }

  private _handleToggle(control: ControlDef) {
    this._emitAction(control, !control.value)
  }

  private _handleSlider(control: ControlDef, e: Event) {
    const value = Number((e.target as HTMLInputElement).value)
    this._emitAction(control, value)
  }

  private _handleButton(control: ControlDef) {
    this._emitAction(control, control.value)
  }

  render() {
    return html`
      <div class="panel">
        ${this.controls.map(control => {
          if (control.type === 'toggle') {
            return html`
              <div class="control-row">
                <span class="control-label">${control.label}</span>
                <button
                  class="toggle ${control.value ? 'on' : 'off'}"
                  @click="${() => this._handleToggle(control)}"
                ></button>
              </div>
            `
          }
          if (control.type === 'slider') {
            return html`
              <div class="slider-row">
                <span class="control-label">${control.label}</span>
                <input
                  type="range"
                  min="${control.min ?? 0}"
                  max="${control.max ?? 100}"
                  value="${control.value as number}"
                  @input="${(e: Event) => this._handleSlider(control, e)}"
                />
                <span class="slider-value">${control.value}</span>
              </div>
            `
          }
          if (control.type === 'button') {
            return html`
              <div class="button-row">
                <button
                  ?disabled="${this.loading}"
                  @click="${() => this._handleButton(control)}"
                >${control.label}</button>
              </div>
            `
          }
          return ''
        })}
      </div>
    `
  }
}
