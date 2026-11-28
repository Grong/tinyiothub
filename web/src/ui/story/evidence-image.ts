import { LitElement, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { fetchEvidenceBlob } from '../../api/media.js';

/**
 * 带工作区鉴权的证据画面。
 *
 * 父组件只给 evidenceId；鉴权、Blob URL 生命周期和过期/失败兜底都在这里，
 * 避免在 `<img src>` 上拼 token。
 */
@customElement('evidence-image')
export class EvidenceImage extends LitElement {
  createRenderRoot() {
    return this;
  }

  @property({ type: String }) evidenceId = '';
  @property({ type: String }) workspaceId = '';
  @property({ type: String }) alt = '现场画面';

  @state() private url = '';
  @state() private failed = false;

  private controller: AbortController | null = null;
  private loadedKey = '';
  private observer: IntersectionObserver | null = null;

  connectedCallback(): void {
    super.connectedCallback();
    this.observe();
  }

  disconnectedCallback(): void {
    this.observer?.disconnect();
    this.observer = null;
    this.cancel();
    this.loadedKey = '';
    super.disconnectedCallback();
  }

  protected updated(changed: Map<string, unknown>): void {
    if (changed.has('evidenceId') || changed.has('workspaceId')) {
      this.loadedKey = '';
      this.observe();
    }
  }

  /** 进入视口附近才拉取证据，避免一次性请求所有历史画面。 */
  private observe(): void {
    if (!this.evidenceId) return;
    if (typeof IntersectionObserver === 'undefined') {
      void this.load();
      return;
    }
    this.observer?.disconnect();
    this.observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        this.observer?.disconnect();
        this.observer = null;
        void this.load();
      },
      { rootMargin: '200px' },
    );
    this.observer.observe(this);
  }

  private cancel(): void {
    this.controller?.abort();
    this.controller = null;
    if (this.url) {
      URL.revokeObjectURL(this.url);
      this.url = '';
    }
  }

  private async load(): Promise<void> {
    const evidenceId = this.evidenceId;
    if (!evidenceId) return;
    const key = `${this.workspaceId}:${evidenceId}`;
    if (key === this.loadedKey) return;
    this.loadedKey = key;
    this.cancel();
    this.failed = false;
    const controller = new AbortController();
    this.controller = controller;
    try {
      const blob = await fetchEvidenceBlob(evidenceId, this.workspaceId, controller.signal);
      if (controller.signal.aborted) return;
      this.url = URL.createObjectURL(blob);
    } catch {
      if (controller.signal.aborted) return;
      this.failed = true;
    }
  }

  render() {
    if (this.url) {
      return html`<img class="evidence-image" src=${this.url} alt=${this.alt} loading="lazy" />`;
    }
    if (this.failed) {
      return html`<div
        class="evidence-image evidence-image--failed"
        role="img"
        aria-label="画面不可用"
      >
        <span>画面不可用</span>
      </div>`;
    }
    if (!this.evidenceId) return nothing;
    return html`<div
      class="evidence-image evidence-image--loading"
      role="img"
      aria-label="画面加载中"
    >
      <span class="evidence-image__spinner" aria-hidden="true"></span>
    </div>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'evidence-image': EvidenceImage;
  }
}
