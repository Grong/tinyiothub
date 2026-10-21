/**
 * E3 Flow 时间机器（spec 2026-10-21-e3 §3）。
 *
 * 账本 → 泳道时间轴 → 检查器。历史模式只读；缺口显式；提供列表回退
 * （读屏/手机）与键盘控制（slider 语义）。
 */

import { LitElement, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import './flow.css';
import { ledgerApi, type LedgerDeviceState, type LedgerEntry, type LedgerGap } from '../../api/ledger.js';
import { buildFlow, chipPosition, type FlowChip, type FlowModel } from './flow-model.js';

const WINDOW_MS = 40 * 60 * 1000;

function fmt(ts: string): string {
  const d = new Date(ts);
  return Number.isNaN(d.getTime()) ? ts : d.toLocaleTimeString('zh-CN', { hour12: false });
}

@customElement('flow-timeline')
export class FlowTimeline extends LitElement {
  @property({ type: String }) workspaceId = '';
  /** RFC3339；null = 实时窗口。 */
  @property({ type: String }) at: string | null = null;
  @property({ type: Number }) eventId: number | null = null;
  /** 父组件禁用动作（历史模式）。 */
  @property({ type: Boolean }) historical = false;

  @state() private loading = true;
  @state() private error = '';
  @state() private model: FlowModel = { lanes: [], chains: [], gaps: [] };
  @state() private entries: LedgerEntry[] = [];
  @state() private devices: LedgerDeviceState[] = [];
  @state() private selectedId: number | null = null;
  @state() private listMode = false;
  @state() private playing = false;
  @state() private speed = 1;
  @state() private scrubIndex = 0;
  @state() private precisionUnknown: string[] = [];

  private playTimer: number | null = null;
  private stateTimer: number | null = null;
  private loadSeq = 0;
  private stateSeq = 0;

  connectedCallback(): void {
    super.connectedCallback();
    void this.load();
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this.stopPlay();
  }

  updated(changed: Map<string, unknown>): void {
    if (changed.has('at') || changed.has('workspaceId')) {
      void this.load();
    }
  }

  private async load(): Promise<void> {
    if (!this.workspaceId) return;
    const seq = ++this.loadSeq;
    this.loading = true;
    this.error = '';
    const anchor = this.at ? new Date(this.at).getTime() : Date.now();
    const from = new Date(anchor - WINDOW_MS).toISOString();
    const to = new Date(anchor).toISOString();
    try {
      const [page, stateAt] = await Promise.all([
        ledgerApi.query(this.workspaceId, { from, to, limit: 500 }),
        ledgerApi.stateAt(this.workspaceId, to),
      ]);
      if (seq !== this.loadSeq) return;
      this.entries = page.entries;
      this.model = buildFlow(page.entries, page.coverage.gaps);
      this.devices = stateAt.devices;
      this.precisionUnknown = stateAt.unknown;
      const preferred =
        page.entries.find((e) => e.id === this.eventId) ??
        page.entries[page.entries.length - 1] ??
        null;
      this.selectedId = preferred?.id ?? null;
      this.scrubIndex = Math.max(0, page.entries.length - 1);
      this.loading = false;
    } catch (e) {
      if (seq !== this.loadSeq) return;
      this.loading = false;
      this.error = e instanceof Error ? e.message : String(e);
    }
  }

  private get selected(): LedgerEntry | null {
    return this.entries.find((e) => e.id === this.selectedId) ?? null;
  }

  private select(id: number): void {
    this.selectedId = id;
    const idx = this.entries.findIndex((e) => e.id === id);
    if (idx >= 0) this.scrubIndex = idx;
    const ts = this.entries[idx]?.ts;
    if (ts) this.scheduleStateAt(ts);
  }

  private seekTo(index: number): void {
    if (!this.entries.length) return;
    const i = Math.max(0, Math.min(this.entries.length - 1, index));
    this.scrubIndex = i;
    this.selectedId = this.entries[i].id;
    this.scheduleStateAt(this.entries[i].ts);
  }

  /** 拖动/播放时按 150ms 去抖拉取该时刻状态快照。 */
  private scheduleStateAt(ts: string): void {
    if (this.stateTimer != null) clearTimeout(this.stateTimer);
    this.stateTimer = window.setTimeout(() => {
      void this.loadStateAt(ts);
    }, 150);
  }

  private async loadStateAt(ts: string): Promise<void> {
    if (!this.workspaceId) return;
    const seq = ++this.stateSeq;
    try {
      const stateAt = await ledgerApi.stateAt(this.workspaceId, ts);
      if (seq !== this.stateSeq) return;
      this.devices = stateAt.devices;
      this.precisionUnknown = stateAt.unknown;
    } catch {
      // 状态快照失败不阻断时间轴（检查器降级为「历史状态不可得」）。
    }
  }

  private togglePlay(): void {
    if (this.playing) {
      this.stopPlay();
      return;
    }
    if (!this.entries.length) return;
    this.playing = true;
    this.playTimer = window.setInterval(() => {
      if (this.scrubIndex >= this.entries.length - 1) {
        this.stopPlay();
        return;
      }
      this.seekTo(this.scrubIndex + 1);
    }, 800 / this.speed);
  }

  private stopPlay(): void {
    this.playing = false;
    if (this.playTimer != null) {
      clearInterval(this.playTimer);
      this.playTimer = null;
    }
  }

  private setSpeed(speed: number): void {
    this.speed = speed;
    if (this.playing) {
      this.stopPlay();
      this.togglePlay();
    }
  }

  private onKeydown(e: KeyboardEvent): void {
    if (e.key === 'ArrowRight') {
      e.preventDefault();
      this.seekTo(this.scrubIndex + (e.shiftKey ? 10 : 1));
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      this.seekTo(this.scrubIndex - (e.shiftKey ? 10 : 1));
    } else if (e.key === ' ') {
      e.preventDefault();
      this.togglePlay();
    } else if (e.key === 'Home') {
      e.preventDefault();
      this.seekTo(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      this.seekTo(this.entries.length - 1);
    }
  }

  private chipClass(chip: FlowChip): string {
    return `flow-chip flow-chip--${chip.kind}${chip.status ? ` flow-chip--${chip.status}` : ''}`;
  }

  private renderChip(chip: FlowChip): unknown {
    return html`
      <button
        class=${this.chipClass(chip)}
        style="left:${this.chipLeft(chip.id)}%"
        title="${fmt(chip.ts)} ${chip.label}"
        aria-label="${fmt(chip.ts)} ${chip.label}"
        @click=${() => this.select(chip.id)}
      >
        ${chip.label}
      </button>
    `;
  }

  private chipLeft(id: number): number {
    if (!this.entries.length) return 0;
    const min = this.entries[0].id;
    const max = this.entries[this.entries.length - 1].id;
    return chipPosition(id, min, max);
  }

  private renderInspector(): unknown {
    const entry = this.selected;
    if (!entry) {
      return html`<div class="flow-inspector__empty">选中一个事件查看详情</div>`;
    }
    const device = this.devices.find((d) => d.thingId === entry.thingId);
    return html`
      <div class="flow-inspector__head">
        <span class="flow-inspector__time">${fmt(entry.ts)}</span>
        <span class="flow-inspector__kind">${entry.kind}</span>
      </div>
      <dl class="flow-inspector__list">
        <dt>设备</dt><dd>${entry.thingId ?? '工作区'}</dd>
        <dt>账本 ID</dt><dd>${entry.id}</dd>
      </dl>
      <pre class="flow-inspector__payload">${JSON.stringify(entry.payload, null, 2)}</pre>
      <h4>该时刻状态</h4>
      ${
        device
          ? html`<pre class="flow-inspector__payload">${JSON.stringify(device.state, null, 2)}</pre>
              <div class="flow-inspector__meta">
                最后变更 ${fmt(device.lastChangeAt)}
                ${device.online == null ? '' : device.online ? ' · 在线' : ' · 离线'}
              </div>`
          : html`<div class="flow-inspector__empty">该设备在此时刻没有历史状态</div>`
      }
      ${
        this.precisionUnknown.length
          ? html`<div class="flow-inspector__note">
              历史不可得：${this.precisionUnknown.join('、')}（账本精度=状态变更）
            </div>`
          : nothing
      }
    `;
  }

  private renderList(): unknown {
    return html`
      <ol class="flow-list" aria-label="事件列表">
        ${this.entries.map(
          (e) => html`
            <li
              class="flow-list__item ${e.id === this.selectedId ? 'is-selected' : ''}"
              tabindex="0"
              @click=${() => this.select(e.id)}
              @keydown=${(ev: KeyboardEvent) => {
                if (ev.key === 'Enter' || ev.key === ' ') {
                  ev.preventDefault();
                  this.select(e.id);
                }
              }}
            >
              <span class="flow-list__time">${fmt(e.ts)}</span>
              <span class="flow-list__kind">${e.kind}</span>
              <span class="flow-list__thing">${e.thingId ?? '工作区'}</span>
              <span class="flow-list__summary">${JSON.stringify(e.payload)}</span>
            </li>
          `,
        )}
      </ol>
    `;
  }

  render() {
    if (this.loading) {
      return html`<div class="flow-state">正在加载账本…</div>`;
    }
    if (this.error) {
      return html`
        <div class="flow-state flow-state--error">
          <span>时间轴加载失败：${this.error}</span>
          <button class="btn btn--primary btn--sm" @click=${() => this.load()}>重试</button>
        </div>
      `;
    }
    if (!this.entries.length) {
      return html`
        <div class="flow-state flow-state--empty">
          <div>该时段没有事件。</div>
          <div class="flow-state__hint">可以放宽时间窗，或回到现在继续观察。</div>
        </div>
      `;
    }

    const gaps: LedgerGap[] = this.model.gaps;
    const selectedTime = this.selected?.ts ?? this.entries[this.scrubIndex]?.ts ?? '';
    const rangeLabel = `${fmt(this.entries[0].ts)} – ${fmt(this.entries[this.entries.length - 1].ts)}`;

    return html`
      <div class="flow">
        <div class="flow-toolbar">
          <span class="flow-toolbar__range">${rangeLabel}</span>
          <span class="flow-toolbar__count">${this.entries.length} 个事件</span>
          <div class="flow-toolbar__spacer"></div>
          <button
            class="btn btn--ghost btn--sm"
            @click=${() => {
              this.listMode = !this.listMode;
            }}
          >
            ${this.listMode ? '时间轴视图' : '列表视图'}
          </button>
        </div>

        ${gaps.length
          ? html`<div class="flow-gap-banner" role="status">
              回放不完整：${gaps.map((g) => `${fmt(g.from)}–${fmt(g.to)}（${g.reason}）`).join('；')}
            </div>`
          : nothing}

        ${this.listMode
          ? this.renderList()
          : html`
              <div class="flow-body">
                <div
                  class="flow-timeline"
                  role="slider"
                  tabindex="0"
                  aria-label="事件时间轴"
                  aria-valuemin="0"
                  aria-valuemax="${Math.max(0, this.entries.length - 1)}"
                  aria-valuenow="${this.scrubIndex}"
                  aria-valuetext="${fmt(selectedTime)} ${this.selected?.kind ?? ''}"
                  @keydown=${(e: KeyboardEvent) => this.onKeydown(e)}
                >
                  ${this.model.lanes.map(
                    (lane) => html`
                      <div class="flow-lane ${lane.chain ? 'flow-lane--chain' : ''}">
                        <div class="flow-lane__label" title=${lane.label}>${lane.label}</div>
                        <div class="flow-lane__track">
                          ${lane.chips.map((chip) => this.renderChip(chip))}
                        </div>
                      </div>
                    `,
                  )}
                  ${this.model.chains.map(
                    (chain) => html`
                      <div
                        class="flow-chain"
                        style="left:${this.chipLeft(chain.startId)}%;right:${100 - this.chipLeft(chain.endId)}%"
                        aria-hidden="true"
                      ></div>
                    `,
                  )}
                </div>
                <aside class="flow-inspector" aria-live="polite">${this.renderInspector()}</aside>
              </div>
            `}

        <div class="flow-scrubber">
          <button class="btn btn--ghost btn--sm" @click=${() => this.togglePlay()} aria-label=${this.playing ? '暂停' : '播放'}>
            ${this.playing ? '⏸' : '▶'}
          </button>
          <select
            class="select flow-scrubber__speed"
            .value=${String(this.speed)}
            @change=${(e: Event) => this.setSpeed(Number((e.target as HTMLSelectElement).value))}
            aria-label="播放速度"
          >
            <option value="1">1x</option>
            <option value="4">4x</option>
          </select>
          <input
            class="flow-scrubber__range"
            type="range"
            min="0"
            max="${Math.max(0, this.entries.length - 1)}"
            .value=${String(this.scrubIndex)}
            @input=${(e: Event) => this.seekTo(Number((e.target as HTMLInputElement).value))}
            aria-label="回放时间"
          />
          <span class="flow-scrubber__time">${fmt(selectedTime)}</span>
        </div>
        <div class="flow-disclaimer">
          回放精度 = 状态变更粒度；遥测毛刺不保证回放。
          ${this.historical ? '历史模式为只读。' : ''}
        </div>
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'flow-timeline': FlowTimeline;
  }
}
