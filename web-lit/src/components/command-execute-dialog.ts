// web-lit/src/components/command-execute-dialog.ts
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { deviceApi } from '../services/devices'
import type { DeviceCommand } from '../services/devices'
import { hostStyles } from '../styles/shared-host'

@customElement('command-execute-dialog')
export class CommandExecuteDialog extends LitElement {
  static styles = [hostStyles, css`
    :host { display: block; }
    .overlay {
      position: fixed;
      inset: 0;
      z-index: 1000;
      background: rgba(0, 0, 0, 0.6);
      backdrop-filter: blur(4px);
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .dialog {
      background: var(--bg);
      width: 90vw;
      max-width: 500px;
      border-radius: var(--radius-lg);
    }
    .header {
      display: flex;
      justify-content: space-between;
      padding: 16px 20px;
      box-shadow: 0 1px 0 var(--card-highlight);
    }
    .header h3 { margin: 0; font-size: 16px; }
    .close-btn {
      width: 32px; height: 32px;
      display: flex; align-items: center; justify-content: center;
      border: none; border-radius: var(--radius-md);
      background: transparent; color: var(--muted); cursor: pointer;
    }
    .body { padding: 20px; }
    .command-info {
      background: var(--card);
      padding: 12px;
      border-radius: var(--radius-md);
      margin-bottom: 16px;
    }
    .command-name { font-weight: 600; margin-bottom: 4px; }
    .command-id { font-size: 11px; color: var(--muted); font-family: monospace; margin-bottom: 4px; }
    .command-desc { font-size: 12px; color: var(--muted); }
    .param-group { margin-bottom: 12px; }
    .param-label {
      display: flex;
      align-items: center;
      gap: 6px;
      font-size: 13px;
      font-weight: 500;
      margin-bottom: 4px;
    }
    .param-type {
      font-size: 10px;
      padding: 1px 6px;
      border-radius: 4px;
      background: var(--bg-muted);
      color: var(--muted);
    }
    .param-input {
      width: 100%;
      padding: 8px 12px;
      background: var(--card);
      border: none;
      border-bottom: 1px solid var(--input);
      color: var(--text);
      font-size: 14px;
    }
    .param-select {
      width: 100%;
      padding: 8px 12px;
      background: var(--card);
      border: none;
      border-bottom: 1px solid var(--input);
      color: var(--text);
      font-size: 14px;
    }
    .footer {
      display: flex;
      justify-content: flex-end;
      gap: 12px;
      padding: 16px 20px;
      box-shadow: 0 -1px 0 var(--card-highlight);
    }
    .btn {
      padding: 8px 16px;
      border-radius: var(--radius-md);
      font-size: 14px;
      cursor: pointer;
      border: none;
    }
    .btn-secondary { background: var(--bg-secondary); color: var(--text); }
    .btn-primary { background: var(--accent); color: white; }
    .btn-primary:disabled { opacity: 0.6; }
    .toast {
      position: fixed;
      bottom: 24px;
      left: 50%;
      transform: translateX(-50%);
      padding: 12px 24px;
      background: var(--card);
      border-radius: var(--radius-md);
      z-index: 2000;
    }
    .toast.success { border-left: 4px solid var(--ok); }
    .toast.error { border-left: 4px solid var(--danger); }
  `]

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
