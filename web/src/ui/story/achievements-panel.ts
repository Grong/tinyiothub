import { LitElement, html } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import {
  EMPTY_ACHIEVEMENTS,
  type AchievementBucket,
  type AchievementStats,
  type AchievementTab,
} from './story-achievements.js';

const TABS: Array<{ id: AchievementTab; label: string }> = [
  { id: 'today', label: '今天' },
  { id: 'week', label: '本周' },
  { id: 'month', label: '本月' },
];

const TAB_TITLE: Record<AchievementTab, string> = {
  today: '今天的功绩',
  week: '本周的功绩',
  month: '本月的功绩',
};

function percent(value: number, max: number): number {
  if (max <= 0 || value <= 0) return 0;
  return Math.max(6, Math.round((value / max) * 100));
}

/**
 * AI 大脑功绩：今天 / 本周 / 本月三个页签，默认今天。
 *
 * 数字全部可追溯到 brain_events 与最近一轮的工具事实；没有“节省人工”
 * 这类不可验证的估算。
 */
@customElement('story-achievements')
export class StoryAchievements extends LitElement {
  createRenderRoot() {
    return this;
  }

  @property({ attribute: false }) stats: AchievementStats = EMPTY_ACHIEVEMENTS;
  @property({ type: Boolean }) loading = false;
  @state() private tab: AchievementTab = 'today';

  render() {
    return html`
      <section class="story-panel story-achievements" aria-label="AI 大脑功绩">
        <header class="story-panel__head">
          <div>
            <div class="story-panel__eyebrow">AI 大脑功绩</div>
            <div class="story-panel__title">${TAB_TITLE[this.tab]}</div>
          </div>
          ${this.loading ? html`<span class="story-panel__meta">更新中…</span>` : null}
        </header>

        <div class="ach-tabs" role="tablist" aria-label="功绩时间范围">
          ${TABS.map(
            (item) => html`
              <button
                class="ach-tab ${this.tab === item.id ? 'ach-tab--active' : ''}"
                role="tab"
                aria-selected=${this.tab === item.id ? 'true' : 'false'}
                @click=${() => (this.tab = item.id)}
              >
                ${item.label}
              </button>
            `,
          )}
        </div>

        <div class="ach-tabpanel" role="tabpanel" data-tab=${this.tab}>
          ${
            this.tab === 'today'
              ? this.renderToday()
              : this.tab === 'week'
                ? this.renderPeriod(
                    '本周',
                    '近 7 天',
                    this.stats.week.closed,
                    this.stats.week.patrol,
                    this.stats.week.needsYou,
                    this.stats.week.days,
                    '近 7 天每日闭环与巡检正常次数',
                  )
                : this.renderPeriod(
                    '本月',
                    `${this.stats.month.buckets.length} 个周段`,
                    this.stats.month.closed,
                    this.stats.month.patrol,
                    this.stats.month.needsYou,
                    this.stats.month.buckets,
                    '本月各周段闭环与巡检正常次数',
                  )
          }
        </div>
      </section>
    `;
  }

  private renderToday() {
    const { latest, today } = this.stats;
    return html`
      <div class="ach-block">
        <div class="ach-block__head">
          <span>最近一轮</span>
          <span class="ach-block__meta"
            >${latest.actions > 0 ? `${latest.actions} 次设备处置` : '只看不改'}</span
          >
        </div>
        <div class="ach-grid">
          ${this.stat('查看设备', latest.devices, '台')}
          ${this.stat('现场画面', latest.vision, '张')}
          ${this.stat('发现问题', latest.findings, '项', latest.findings > 0)}
          ${this.stat('今日闭环', today.closed, '件')}
        </div>
        ${
          latest.devices > latest.inspected
            ? html`<div class="ach-note">
                另有 ${latest.devices - latest.inspected} 台仅出现在设备清单中
              </div>`
            : null
        }
      </div>

      <div class="ach-summary">
        <span>今日巡检正常 <strong>${today.patrol}</strong> 次</span>
        <span>已消化 <strong>${today.digested}</strong> 条事件</span>
      </div>

      ${
        today.needsYou > 0
          ? html`<div class="ach-needs">
              <span class="ach-needs__dot" aria-hidden="true"></span>
              还有 ${today.needsYou} 件需要你确认
              <a class="ach-needs__link" href="/chat">去处理</a>
            </div>`
          : html`<div class="ach-clear">今天没有需要你介入的例外</div>`
      }
    `;
  }

  private renderPeriod(
    title: string,
    meta: string,
    closed: number,
    patrol: number,
    needsYou: number,
    buckets: AchievementBucket[],
    ariaLabel: string,
  ) {
    const max = Math.max(1, ...buckets.map((bucket) => bucket.closed + bucket.patrol));
    return html`
      <div class="ach-block">
        <div class="ach-block__head">
          <span>${title}</span>
          <span class="ach-block__meta">${meta}</span>
        </div>
        <div class="ach-grid ach-grid--three">
          ${this.stat('闭环处理', closed, '件')} ${this.stat('巡检正常', patrol, '次')}
          ${this.stat('转人工', needsYou, '件')}
        </div>
        <div class="ach-bars" role="img" aria-label=${ariaLabel}>
          ${buckets.map(
            (bucket) => html`
              <div
                class="ach-bar"
                title=${`${bucket.key}：闭环 ${bucket.closed} · 巡检正常 ${bucket.patrol} · 需你 ${bucket.needsYou}`}
              >
                <span class="ach-bar__value"
                  >${bucket.closed + bucket.patrol > 0 ? bucket.closed + bucket.patrol : ''}</span
                >
                <span class="ach-bar__track">
                  <span
                    class="ach-bar__fill"
                    style=${`height:${percent(bucket.closed + bucket.patrol, max)}%`}
                  ></span>
                </span>
                <span class="ach-bar__label">${bucket.label}</span>
              </div>
            `,
          )}
        </div>
      </div>
    `;
  }

  private stat(label: string, value: number, unit: string, alert = false) {
    return html`
      <div class="ach-stat ${alert ? 'ach-stat--alert' : ''}">
        <span class="ach-stat__value">${value}</span>
        <span class="ach-stat__unit">${unit}</span>
        <span class="ach-stat__label">${label}</span>
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'story-achievements': StoryAchievements;
  }
}
