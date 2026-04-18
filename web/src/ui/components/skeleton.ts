import { LitElement, html, css } from "lit";
import { customElement, property } from "lit/decorators.js";

@customElement("skeleton-loader")
export class SkeletonLoader extends LitElement {
  static styles = css`
    :host {
      display: block;
    }
    
    .skeleton {
      background: linear-gradient(90deg, var(--bg-hover, #262a35) 25%, var(--bg-elevated, #323842) 50%, var(--bg-hover, #262a35) 75%);
      background-size: 200% 100%;
      animation: skeleton-loading 1.5s infinite;
      border-radius: var(--radius-sm, 4px);
    }
    
    @keyframes skeleton-loading {
      0% { background-position: 200% 0; }
      100% { background-position: -200% 0; }
    }
    
    .skeleton-text {
      height: 14px;
      margin-bottom: 8px;
    }
    
    .skeleton-text:last-child {
      width: 70%;
    }
    
    .skeleton-title {
      height: 24px;
      width: 60%;
      margin-bottom: 16px;
    }
    
    .skeleton-avatar {
      width: 40px;
      height: 40px;
      border-radius: 50%;
    }
    
    .skeleton-card {
      height: 80px;
      margin-bottom: 12px;
    }
    
    .skeleton-image {
      height: 160px;
      margin-bottom: 12px;
    }
  `;

  @property()
  type: "text" | "title" | "avatar" | "card" | "image" = "text";

  render() {
    switch (this.type) {
      case "title":
        return html`<div class="skeleton skeleton-title"></div>`;
      case "avatar":
        return html`<div class="skeleton skeleton-avatar"></div>`;
      case "card":
        return html`<div class="skeleton skeleton-card"></div>`;
      case "image":
        return html`<div class="skeleton skeleton-image"></div>`;
      default:
        return html`<div class="skeleton skeleton-text"></div>`;
    }
  }
}

// 预制的加载骨架
@customElement("skeleton-list")
export class SkeletonList extends LitElement {
  static styles = css`
    :host {
      display: block;
    }
    
    .skeleton-item {
      display: flex;
      gap: 12px;
      padding: 12px;
      border-bottom: 1px solid var(--border, #27272a);
    }
    
    .skeleton {
      background: linear-gradient(90deg, var(--bg-hover, #262a35) 25%, var(--bg-elevated, #323842) 50%, var(--bg-hover, #262a35) 75%);
      background-size: 200% 100%;
      animation: skeleton-loading 1.5s infinite;
      border-radius: var(--radius-sm, 4px);
    }
    
    @keyframes skeleton-loading {
      0% { background-position: 200% 0; }
      100% { background-position: -200% 0; }
    }
    
    .skeleton-avatar {
      width: 40px;
      height: 40px;
      border-radius: 50%;
      flex-shrink: 0;
    }
    
    .skeleton-content {
      flex: 1;
    }
    
    .skeleton-text {
      height: 12px;
      margin-bottom: 8px;
      width: 100%;
    }
    
    .skeleton-text.short {
      width: 60%;
    }
  `;

  @property({ type: Number })
  count = 3;

  render() {
    return html`
      ${[...Array(this.count)].map(() => html`
        <div class="skeleton-item">
          <div class="skeleton skeleton-avatar"></div>
          <div class="skeleton-content">
            <div class="skeleton skeleton-text"></div>
            <div class="skeleton skeleton-text short"></div>
          </div>
        </div>
      `)}
    `;
  }
}

// 骨架表格
@customElement("skeleton-table")
export class SkeletonTable extends LitElement {
  static styles = css`
    :host {
      display: block;
    }
    
    .skeleton {
      background: linear-gradient(90deg, var(--bg-hover, #262a35) 25%, var(--bg-elevated, #323842) 50%, var(--bg-hover, #262a35) 75%);
      background-size: 200% 100%;
      animation: skeleton-loading 1.5s infinite;
      border-radius: var(--radius-sm, 4px);
    }
    
    @keyframes skeleton-loading {
      0% { background-position: 200% 0; }
      100% { background-position: -200% 0; }
    }
    
    .skeleton-row {
      display: grid;
      gap: 12px;
      padding: 14px 0;
      border-bottom: 1px solid var(--border, #27272a);
      grid-template-columns: repeat(var(--cols, 4), 1fr);
    }
    
    .skeleton-cell {
      height: 16px;
    }
  `;

  @property({ type: Number })
  columns = 4;

  @property({ type: Number })
  rows = 5;

  render() {
    return html`
      <style>
        :host {
          --cols: ${this.columns};
        }
      </style>
      ${[...Array(this.rows)].map(() => html`
        <div class="skeleton-row">
          ${[...Array(this.columns)].map(() => html`
            <div class="skeleton skeleton-cell"></div>
          `)}
        </div>
      `)}
    `;
  }
}
