import { LitElement, html } from "lit";
import { customElement, property } from "lit/decorators.js";
import type { ThemeMode } from "../theme.js";

@customElement("theme-toggle")
export class ThemeToggle extends LitElement {
  @property({ type: String }) theme: ThemeMode = "system";

  // 不使用 Shadow DOM，这样可以使用全局样式
  createRenderRoot() {
    return this;
  }

  private getThemeIndex(): number {
    const order: ThemeMode[] = ["system", "light", "dark"];
    return Math.max(0, order.indexOf(this.theme));
  }

  private handleThemeClick(theme: ThemeMode, event: MouseEvent) {
    this.dispatchEvent(
      new CustomEvent("theme-change", {
        detail: { theme, event },
        bubbles: true,
        composed: true,
      })
    );
  }

  render() {
    const index = this.getThemeIndex();

    return html`
      <div class="theme-toggle" style="--theme-index: ${index};">
        <div class="theme-toggle__track" role="group" aria-label="Theme">
          <span class="theme-toggle__indicator"></span>
          <button
            class="theme-toggle__button ${this.theme === "system" ? "active" : ""}"
            @click=${(e: MouseEvent) => this.handleThemeClick("system", e)}
            aria-pressed=${this.theme === "system"}
            aria-label="System theme"
            title="跟随系统"
          >
            <svg class="theme-icon" viewBox="0 0 24 24" aria-hidden="true">
              <rect width="20" height="14" x="2" y="3" rx="2"></rect>
              <line x1="8" x2="16" y1="21" y2="21"></line>
              <line x1="12" x2="12" y1="17" y2="21"></line>
            </svg>
          </button>
          <button
            class="theme-toggle__button ${this.theme === "light" ? "active" : ""}"
            @click=${(e: MouseEvent) => this.handleThemeClick("light", e)}
            aria-pressed=${this.theme === "light"}
            aria-label="Light theme"
            title="亮色模式"
          >
            <svg class="theme-icon" viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="12" cy="12" r="4"></circle>
              <path d="M12 2v2"></path>
              <path d="M12 20v2"></path>
              <path d="m4.93 4.93 1.41 1.41"></path>
              <path d="m17.66 17.66 1.41 1.41"></path>
              <path d="M2 12h2"></path>
              <path d="M20 12h2"></path>
              <path d="m6.34 17.66-1.41 1.41"></path>
              <path d="m19.07 4.93-1.41 1.41"></path>
            </svg>
          </button>
          <button
            class="theme-toggle__button ${this.theme === "dark" ? "active" : ""}"
            @click=${(e: MouseEvent) => this.handleThemeClick("dark", e)}
            aria-pressed=${this.theme === "dark"}
            aria-label="Dark theme"
            title="暗色模式"
          >
            <svg class="theme-icon" viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401"
              ></path>
            </svg>
          </button>
        </div>
      </div>
    `;
  }
}
