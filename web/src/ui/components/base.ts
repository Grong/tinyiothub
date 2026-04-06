// UI组件基类和工具

import { LitElement } from 'lit';
import { property } from 'lit/decorators.js';

export class BaseComponent extends LitElement {
  @property({ type: Boolean }) loading = false;
  @property({ type: String }) error: string | null = null;

  createRenderRoot() {
    // 使用 light DOM，这样可以使用全局样式
    return this;
  }

  protected showError(message: string) {
    this.error = message;
    this.dispatchEvent(new CustomEvent('error', {
      detail: { message },
      bubbles: true,
      composed: true,
    }));
  }

  protected clearError() {
    this.error = null;
  }

  protected showLoading() {
    this.loading = true;
  }

  protected hideLoading() {
    this.loading = false;
  }

  protected emit<T = any>(eventName: string, detail?: T) {
    this.dispatchEvent(new CustomEvent(eventName, {
      detail,
      bubbles: true,
      composed: true,
    }));
  }
}
