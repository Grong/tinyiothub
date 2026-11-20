import { LitElement, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import {
  deviceHealthLabel,
  deviceTypeLabel,
  formatStoryClock,
  type StoryDevice,
} from './story-model.js';

const COLLAPSED_HEIGHT = 234;
const OPEN_ROW = 78;

type DeviceStatus = 'online' | 'offline' | 'warning' | 'unknown';

function statusOf(device: StoryDevice): DeviceStatus {
  if (device.online === false) return 'offline';
  if (device.health === 'bad' || device.health === 'attention') return 'warning';
  if (device.online === true) return 'online';
  return 'unknown';
}

function statusLabel(status: DeviceStatus): string {
  if (status === 'offline') return '离线';
  if (status === 'online') return '在线';
  if (status === 'warning') return '需关注';
  return '未知';
}

/**
 * 本轮工作涉及的设备卡牌。
 *
 * 设备多时重叠成一副牌：最新一台完整显示，其余露出上沿；悬停/聚焦在露出的
 * 边缘上会把对应卡牌提到最前，点“展开全部”则变成可读的紧凑列表。
 * 卡面使用液态玻璃：半透明、折射高光、缓慢的镜面流动。
 */
@customElement('story-device-deck')
export class StoryDeviceDeck extends LitElement {
  createRenderRoot() {
    return this;
  }

  @property({ attribute: false }) devices: StoryDevice[] = [];
  @property({ type: Boolean }) working = false;
  @state() private pinned = false;

  private get open(): boolean {
    return this.pinned;
  }

  render() {
    const count = this.devices.length;
    return html`
      <section class="story-panel story-deck-panel" aria-label="本轮设备">
        <header class="story-panel__head">
          <div>
            <div class="story-panel__eyebrow">本轮设备</div>
            <div class="story-panel__title">${count > 0 ? `${count} 台` : '暂无'}</div>
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
            ? html`<p class="story-panel__empty">最近一轮还没有查看具体设备。</p>`
            : html`
                <div
                  class="story-deck"
                  data-open=${this.open ? 'true' : 'false'}
                  style=${`height:${this.open ? Math.max(count * OPEN_ROW, OPEN_ROW) : COLLAPSED_HEIGHT}px`}
                >
                  ${this.devices.map((device, index) => this.renderCard(device, index))}
                </div>
                <p class="story-deck__hint">
                  ${this.open ? `${count} 台设备，按最近关注排序` : '悬停露出的卡边，查看其它设备'}
                </p>
              `
        }
      </section>
    `;
  }

  private renderCard(device: StoryDevice, index: number) {
    const status = statusOf(device);
    const health = deviceHealthLabel(device.health);
    const sub = [device.zone, deviceTypeLabel(device.type), health].filter(Boolean).join(' · ');
    const working = this.working && index === 0;
    return html`
      <article
        class="story-device ${index === 0 ? 'story-device--top' : ''} ${
          working ? 'story-device--working' : ''
        }"
        style=${`--i:${Math.min(index, 4)}; z-index:${this.devices.length - index}`}
        tabindex="0"
        aria-label=${`${device.name}，${statusLabel(status)}`}
      >
        <div class="story-device__top">
          <span class="story-device__state is-${status}" aria-hidden="true"></span>
          <span class="story-device__name">${device.name}</span>
          <span class="story-device__status is-${status}">${statusLabel(status)}</span>
        </div>
        <div class="story-device__sub">${sub || '位置与状态未上报'}</div>
        <div class="story-device__attrs">
          <div class="story-device__attrs-head">
            <span>属性</span>
            <span>当前值</span>
          </div>
          ${
            device.metrics.length > 0
              ? device.metrics.map(
                  (metric) =>
                    html`<div class="story-device__attr">
                      <span class="story-device__attr-name">${metric.name}</span>
                      <span class="story-device__attr-value">${metric.value}</span>
                    </div>`,
                )
              : html`<div class="story-device__attrs-empty">暂无实时属性上报</div>`
          }
        </div>
        <div class="story-device__foot">
          <span class="story-device__action">${device.lastAction || '最近被关注'}</span>
          <span class="story-device__foot-meta">
            ${device.touches > 1 ? `关注 ${device.touches} 次 · ` : ''}${formatStoryClock(
              device.lastAt,
            )}
          </span>
        </div>
      </article>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'story-device-deck': StoryDeviceDeck;
  }
}
