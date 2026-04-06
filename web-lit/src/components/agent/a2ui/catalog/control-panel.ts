import { LitElement, html} from 'lit'
import { customElement, property, state } from 'lit/decorators.js'

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
  createRenderRoot() { return this }
  @property({ type: Array }) controls: ControlDef[] = []
  // Loading state for button controls — set externally via setLoading()
  @state() private loading = false

  setLoading(v: boolean) {
    this.loading = v
  }

  

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
