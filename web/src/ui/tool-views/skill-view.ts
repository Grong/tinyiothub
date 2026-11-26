import { LitElement, css, html } from 'lit';
import { customElement, property } from 'lit/decorators.js';

/** Renders skill identity from event metadata; it never resolves names itself. */
@customElement('skill-view')
export class SkillView extends LitElement {
  static styles = css`
    :host {
      display: inline-flex;
      align-items: baseline;
      gap: 6px;
      min-width: 0;
    }
    .title {
      color: var(--text);
      font-size: 12px;
      white-space: nowrap;
    }
    .description {
      min-width: 0;
      overflow: hidden;
      color: var(--muted);
      font-size: 11px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  `;

  @property({ attribute: false }) name = '';
  @property({ attribute: false }) title = '';
  @property({ attribute: false }) description = '';

  render() {
    return html`<span class="title">${this.title || this.name || '作业规范'}</span>
      ${this.description ? html`<span class="description">${this.description}</span>` : ''}`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'skill-view': SkillView;
  }
}
