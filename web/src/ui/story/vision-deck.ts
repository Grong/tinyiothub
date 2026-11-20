import { LitElement, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { formatStoryClock, type StoryVision } from './story-model.js';
import './evidence-image.js';

const COLLAPSED_HEIGHT = 268;
const OPEN_ROW = 84;

/**
 * 本次工作看到的现场画面。
 *
 * 和手机相册的卡牌堆一样：最新一张完整显示，悬停/聚焦露出边缘可预览其它
 * 画面；“展开全部”变成一行一张的紧凑列表。
 */
@customElement('story-vision-deck')
export class StoryVisionDeck extends LitElement {
  createRenderRoot() {
    return this;
  }

  @property({ attribute: false }) visions: StoryVision[] = [];
  @property({ type: String }) workspaceId = '';
  @property({ type: Boolean }) working = false;
  @state() private pinned = false;

  private get open(): boolean {
    return this.pinned;
  }

  render() {
    const count = this.visions.length;
    return html`
      <section class="story-panel story-deck-panel" aria-label="本轮现场画面">
        <header class="story-panel__head">
          <div>
            <div class="story-panel__eyebrow">本轮画面</div>
            <div class="story-panel__title">${count > 0 ? `${count} 张` : '暂无'}</div>
          </div>
          ${
            count > 1
              ? html`<button
                  class="story-panel__action"
                  aria-expanded=${this.pinned ? 'true' : 'false'}
                  @click=${() => (this.pinned = !this.pinned)}
                >
                  ${this.pinned ? '收起' : '展开全部'}
                </button>`
              : nothing
          }
        </header>
        ${
          count === 0
            ? html`<p class="story-panel__empty">最近一轮还没有调取摄像头画面。</p>`
            : html`
                <div
                  class="story-deck story-deck--vision"
                  data-open=${this.open ? 'true' : 'false'}
                  style=${`height:${this.open ? Math.max(count * OPEN_ROW, OPEN_ROW) : COLLAPSED_HEIGHT}px`}
                >
                  ${this.visions.map((vision, index) => this.renderCard(vision, index))}
                </div>
                <p class="story-deck__hint">
                  ${this.open ? `${count} 张画面，按时间倒序` : '悬停露出的卡边，查看其它画面'}
                </p>
              `
        }
      </section>
    `;
  }

  private renderCard(vision: StoryVision, index: number) {
    return html`
      <article
        class="story-vision ${index === 0 ? 'story-vision--top' : ''} ${
          this.working && index === 0 ? 'story-vision--working' : ''
        }"
        style=${`--i:${Math.min(index, 4)}; z-index:${this.visions.length - index}`}
        tabindex="0"
        aria-label=${`${vision.deviceName} ${vision.caption}`}
      >
        <div class="story-vision__media">
          ${
            index === 0 || this.open
              ? html`<evidence-image
                  .evidenceId=${vision.evidenceId}
                  .workspaceId=${this.workspaceId}
                  .alt=${`${vision.deviceName} ${vision.caption}`}
                ></evidence-image>`
              : html`<span class="story-vision__placeholder" aria-hidden="true"></span>`
          }
          <span class="story-vision__badge">${vision.caption}</span>
        </div>
        <div class="story-vision__body">
          <div class="story-vision__head">
            <strong>${vision.deviceName}</strong>
            <time>${formatStoryClock(vision.at)}</time>
          </div>
          <p class=${vision.finding ? 'story-vision__finding' : 'story-vision__note'}>
            ${vision.finding || 'AI 已查看这张画面'}
          </p>
        </div>
      </article>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'story-vision-deck': StoryVisionDeck;
  }
}
