// web-lit/src/components/command-execute-dialog.ts
import { LitElement, html} from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi } from '../services/devices'
import type { DeviceCommand } from '../services/devices'

@customElement('command-execute-dialog')
export class CommandExecuteDialog extends LitElement {
  createRenderRoot() { return this }
  

  @property({ type: Boolean }) open = false
  @property({ type: Object }) command!: DeviceCommand
  @property({ type: String }) deviceId = ''
  @state() params: Record<string, any> = {}
  @state() executing = false
  @state() toast = ''

  updated(changedProperties: Map<string, any>) {
    if (changedProperties.has('open') && this.open) {
      const defaults: Record<string, any> = {}
      if (this.command?.parameters) {
        Object.entries(this.command.parameters).forEach(([k, v]) => { defaults[k] = v })
      }
      this.params = defaults
    }
  }

  private close() {
    this.open = false
    this.dispatchEvent(new CustomEvent('close'))
  }

  private handleParamChange(key: string, value: any) {
    this.params = { ...this.params, [key]: value }
  }

  private async execute() {
    this.executing = true
    try {
      await deviceApi.executeCommand(this.deviceId, this.command.id, this.params)
      this.showToast('命令已发送', 'success')
      this.close()
      this.dispatchEvent(new CustomEvent('success'))
    } catch (err: any) {
      this.showToast(err.message || '执行失败', 'error')
    } finally {
      this.executing = false
    }
  }

  private showToast(message: string, type: 'success' | 'error') {
    this.toast = `${type}:${message}`
    setTimeout(() => { this.toast = '' }, 3000)
  }

  render() {
    if (!this.open) return html``
    const paramEntries = Object.entries(this.command?.parameters || {})
    return html`
      <div class="overlay" @click=${() => this.close()}>
        <div class="dialog" @click=${(e: Event) => e.stopPropagation()}>
          <div class="header">
            <h3>执行指令</h3>
            <button class="close-btn" @click=${() => this.close()}>
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 6L6 18M6 6l12 12"/>
              </svg>
            </button>
          </div>
          <div class="body">
            <div class="command-info">
              <div class="command-name">${this.command?.name}</div>
              <div class="command-id">ID: ${this.command?.id}</div>
              <div class="command-desc">${this.command?.description || '无描述'}</div>
            </div>
            ${paramEntries.length > 0 ? paramEntries.map(([key, defaultValue]) => {
              const paramType = typeof defaultValue
              const placeholder = defaultValue != null ? String(defaultValue) : ''
              return html`
              <div class="param-group">
                <label class="param-label">
                  ${key}
                  <span class="param-type">${paramType}</span>
                </label>
                ${paramType === 'boolean' ? html`
                  <select class="param-select"
                    @change=${(e: Event) => this.handleParamChange(key, (e.target as HTMLSelectElement).value === 'true')}>
                    <option value="true" ?selected=${this.params[key] === true}>true</option>
                    <option value="false" ?selected=${this.params[key] === false || this.params[key] == null}>false</option>
                  </select>
                ` : html`
                  <input type=${paramType === 'number' ? 'number' : 'text'}
                    class="param-input"
                    placeholder=${placeholder}
                    .value=${this.params[key] ?? defaultValue ?? ''}
                    @input=${(e: InputEvent) => this.handleParamChange(key, (e.target as HTMLInputElement).value)}
                  />
                `}
              </div>
            `}) : html`<p style="color: var(--muted); font-size: 13px;">此指令无需参数</p>`}
          </div>
          <div class="footer">
            <button class="btn btn-secondary" @click=${() => this.close()}>取消</button>
            <button class="btn btn-primary" ?disabled=${this.executing} @click=${this.execute}>
              ${this.executing ? '执行中...' : '确认执行'}
            </button>
          </div>
        </div>
      </div>
      ${this.toast ? html`<div class="toast ${this.toast.split(':')[0]}">${this.toast.split(':')[1]}</div>` : ''}
    `
  }
}

declare global {
  interface HTMLElementTagNameMap { 'command-execute-dialog': CommandExecuteDialog }
}
