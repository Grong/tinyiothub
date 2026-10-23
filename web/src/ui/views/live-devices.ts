/**
 * E3 实时设备健康卡（spec §2.2 / 计划 T6）：
 * SSE snapshot 驱动的 worst-first 卡片；watch/job 活动徽标；断线重连横幅。
 *
 * E4b：选中卡片改为订阅 Hub 订阅流（`/hub/stream`），按该设备驱动声明的
 * source 接收 reading/snapshot，卡片内展示实时读数与连接状态；
 * 健康汇总仍来自 `/live/stream`，Flow 未受影响。
 */

import { LitElement, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import './live-devices.css';
import { connectLive, type LiveConnection, type SseEvent } from '../../api/live.js';
import {
  connectHubStream,
  fetchDeclaredSources,
  type HubConnection,
  type HubReading,
  type HubSnapshot,
  type HubStreamStatus,
} from '../../api/hub-stream.js';
import { thingApi } from '../../api/things.js';

interface LiveDevice {
  thingId: string;
  name: string;
  health: string;
  zone: string;
  online: boolean;
  values: Record<string, string>;
}

interface Activity {
  watch: number;
  job: number;
}

const HEALTH_RANK: Record<string, number> = { bad: 0, attention: 1, unknown: 2, ok: 3 };

const HUB_STATUS_LABEL: Record<HubStreamStatus, string> = {
  connecting: '连接中',
  live: '实时',
  reconnecting: '重连中',
  offline: '离线',
};

function sorted(devices: LiveDevice[]): LiveDevice[] {
  return [...devices].sort((a, b) => {
    const rank = (HEALTH_RANK[a.health] ?? 2) - (HEALTH_RANK[b.health] ?? 2);
    if (rank !== 0) return rank;
    const zone = a.zone.localeCompare(b.zone, 'zh-CN');
    return zone !== 0 ? zone : a.name.localeCompare(b.name, 'zh-CN');
  });
}

function displayValue(value: unknown): string {
  if (value == null) return '—';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

@customElement('live-devices')
export class LiveDevices extends LitElement {
  @property({ type: String }) workspaceId = '';
  @state() private devices: LiveDevice[] = [];
  @state() private status: 'connecting' | 'open' | 'error' | 'closed' = 'connecting';
  @state() private activity: Record<string, Activity> = {};
  @state() private selectedThingId: string | null = null;
  @state() private hubStatus: HubStreamStatus = 'offline';
  @state() private hubSources: string[] = [];
  @state() private hubReadings: Record<string, Record<string, string>> = {};
  @state() private hubNote = '';
  private live: LiveConnection | null = null;
  private hub: HubConnection | null = null;
  /** 选择代际：异步解析驱动声明期间切换设备时丢弃旧结果。 */
  private selectSeq = 0;

  connectedCallback(): void {
    super.connectedCallback();
    this.connect();
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this.live?.close();
    this.live = null;
    this.closeHub();
    this.selectSeq += 1;
  }

  private connect(): void {
    if (!this.workspaceId) return;
    this.live?.close();
    this.live = connectLive(this.workspaceId, {
      onStatus: (status) =>
        (this.status =
          status === 'connecting'
            ? 'connecting'
            : status === 'open'
              ? 'open'
              : status === 'error'
                ? 'error'
                : 'closed'),
      onEvent: (ev: SseEvent) => this.onEvent(ev),
    });
  }

  private onEvent(ev: SseEvent): void {
    let data: Record<string, unknown>;
    try {
      data = JSON.parse(ev.data) as Record<string, unknown>;
    } catch {
      return;
    }
    if (ev.event === 'snapshot') {
      const devices = ((data.devices as LiveDevice[]) ?? []).map((d) => ({
        ...d,
        values: Array.isArray(d.values)
          ? Object.fromEntries(d.values as Array<[string, string]>)
          : (d.values ?? {}),
      }));
      this.devices = devices;
      if (this.selectedThingId && !devices.some((d) => d.thingId === this.selectedThingId)) {
        this.closeHub();
        this.selectedThingId = null;
      }
      return;
    }
    const thingId = typeof data.thingId === 'string' ? data.thingId : null;
    if (!thingId) return;
    if (ev.event === 'state_change') {
      const payload = (data.payload ?? {}) as Record<string, unknown>;
      const field = typeof payload.field === 'string' ? payload.field : null;
      if (!field || field.startsWith('__')) return;
      this.devices = this.devices.map((d) =>
        d.thingId === thingId
          ? { ...d, values: { ...d.values, [field]: String(payload.new ?? '') } }
          : d,
      );
    } else if (ev.event === 'watch' || ev.event === 'job') {
      const current = this.activity[thingId] ?? { watch: 0, job: 0 };
      this.activity = {
        ...this.activity,
        [thingId]: {
          watch: current.watch + (ev.event === 'watch' ? 1 : 0),
          job: current.job + (ev.event === 'job' ? 1 : 0),
        },
      };
    }
  }

  private closeHub(): void {
    this.hub?.close();
    this.hub = null;
    this.hubStatus = 'offline';
    this.hubSources = [];
    this.hubReadings = {};
    this.hubNote = '';
  }

  /** 选中卡片：关闭上一条订阅流，按该设备声明的 source 新开一条。 */
  private select(thingId: string): void {
    if (this.selectedThingId === thingId && this.hub) return;
    this.hub?.close();
    this.hub = null;
    this.hubSources = [];
    this.hubReadings = {};
    this.hubNote = '';
    this.selectedThingId = thingId;
    this.hubStatus = 'connecting';
    const seq = ++this.selectSeq;
    void this.openHub(thingId, seq);
  }

  private async openHub(thingId: string, seq: number): Promise<void> {
    let driverName = '';
    try {
      const res = await thingApi.get(thingId);
      driverName = res.result?.driverName ?? '';
    } catch {
      // 设备详情不可用：下方会以离线状态提示。
    }
    const sources = driverName ? await fetchDeclaredSources(driverName) : [];
    if (seq !== this.selectSeq || this.selectedThingId !== thingId) return;
    if (!this.workspaceId) {
      this.hubStatus = 'offline';
      this.hubNote = '工作空间未就绪';
      return;
    }
    if (sources.length === 0) {
      this.hubStatus = 'offline';
      this.hubNote = driverName ? '驱动未声明数据源' : '设备未绑定驱动';
      return;
    }
    this.hubSources = sources;
    this.hub = connectHubStream(this.workspaceId, [thingId], sources, {
      onStatus: (status) => {
        if (seq === this.selectSeq) this.hubStatus = status;
      },
      onSnapshot: (snapshot) => {
        if (seq === this.selectSeq) this.applySnapshot(thingId, snapshot);
      },
      onReading: (reading) => {
        if (seq === this.selectSeq && reading.thingId === thingId) this.applyReading(reading);
      },
      onResync: () => {
        if (seq !== this.selectSeq) return;
        this.hubReadings = {};
        this.hubNote = '数据源落后，正在重新同步…';
      },
    });
  }

  private applySnapshot(thingId: string, snapshot: HubSnapshot): void {
    const device = snapshot.devices.find((d) => d.thingId === thingId);
    const readings: Record<string, Record<string, string>> = {};
    for (const source of device?.sources ?? []) {
      readings[source.source] = Object.fromEntries(
        Object.entries(source.values).map(([key, value]) => [key, displayValue(value)]),
      );
    }
    this.hubReadings = readings;
    this.hubNote = '';
    this.syncCardValues(thingId);
  }

  private applyReading(reading: HubReading): void {
    const values = Object.fromEntries(
      Object.entries(reading.values).map(([key, value]) => [key, displayValue(value)]),
    );
    this.hubReadings = {
      ...this.hubReadings,
      [reading.source]: { ...(this.hubReadings[reading.source] ?? {}), ...values },
    };
    this.syncCardValues(reading.thingId);
  }

  /** 把订阅读数合并进卡片标题值（健康卡仍由 /live/stream 驱动）。 */
  private syncCardValues(thingId: string): void {
    const merged = Object.values(this.hubReadings).reduce<Record<string, string>>(
      (acc, values) => ({ ...acc, ...values }),
      {},
    );
    this.devices = this.devices.map((d) =>
      d.thingId === thingId ? { ...d, values: { ...d.values, ...merged } } : d,
    );
  }

  private open(thingId: string): void {
    this.dispatchEvent(
      new CustomEvent('device-open', { detail: { thingId }, bubbles: true, composed: true }),
    );
  }

  private renderHubPanel(device: LiveDevice) {
    const sources = Object.entries(this.hubReadings);
    return html`
      <div
        class="ld-live ld-live--${this.hubStatus}"
        role="group"
        aria-label="${device.name} 订阅实时读数"
      >
        <div class="ld-live__status" role="status">
          <span class="ld-live__dot" aria-hidden="true"></span>
          <span>${HUB_STATUS_LABEL[this.hubStatus]}</span>
          ${this.hubSources.length
            ? html`<span class="ld-live__sources">${this.hubSources.join(' · ')}</span>`
            : nothing}
        </div>
        ${this.hubNote ? html`<div class="ld-live__note">${this.hubNote}</div>` : nothing}
        ${sources.length
          ? html`<dl class="ld-live__values">
              ${sources.map(([source, values]) =>
                Object.entries(values).map(
                  ([key, value]) =>
                    html`<dt>${source} · ${key}</dt>
                      <dd>${value}</dd>`,
                ),
              )}
            </dl>`
          : nothing}
        <button
          class="btn btn--ghost btn--sm ld-live__open"
          @click=${() => this.open(device.thingId)}
        >
          打开详情
        </button>
      </div>
    `;
  }

  render() {
    if (this.status === 'connecting' && !this.devices.length) {
      return html`<div class="ld-state">正在连接实时投影…</div>`;
    }
    const offline = this.status === 'error' || this.status === 'closed';
    return html`
      ${offline
        ? html`<div class="ld-banner" role="status">
            实时连接中断，卡片可能不是最新。
            <button class="btn btn--primary btn--sm" @click=${() => this.connect()}>重连</button>
          </div>`
        : nothing}
      ${this.devices.length === 0
        ? html`
            <div class="ld-state ld-state--empty">
              <div>还没有设备实时数据。</div>
              <div class="ld-state__hint">接入网关或等待设备上报后，这里会出现健康卡片。</div>
            </div>
          `
        : html`
            <div class="ld-grid">
              ${sorted(this.devices).map((d) => {
                const a = this.activity[d.thingId];
                const selected = this.selectedThingId === d.thingId;
                const values = Object.entries(d.values ?? {}).slice(0, 3);
                return html`
                  <div class="ld-cell">
                    <article
                      class="ld-card ld-card--${d.health} ${selected ? 'ld-card--selected' : ''}"
                      tabindex="0"
                      role="button"
                      aria-pressed=${selected ? 'true' : 'false'}
                      aria-label="${d.name} ${d.health} ${d.online ? '在线' : '离线'}"
                      @click=${() => this.select(d.thingId)}
                      @keydown=${(e: KeyboardEvent) => {
                        // 子元素（如实时面板里的按钮）的键盘事件需自行处理，
                        // 不能被卡片的 Enter/Space 抢走并 preventDefault。
                        if (e.target !== e.currentTarget) return;
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault();
                          this.select(d.thingId);
                        }
                      }}
                    >
                      <header>
                        <span
                          class="ld-card__dot ld-card__dot--${d.health}"
                          aria-hidden="true"
                        ></span>
                        <span class="ld-card__name">${d.name}</span>
                        <span class="ld-card__health">${d.health}</span>
                      </header>
                      <div class="ld-card__zone">
                        ${d.zone || '未归属区域'} · ${d.online ? '在线' : '离线'}
                      </div>
                      <dl class="ld-card__values">
                        ${values.map(
                          ([k, v]) =>
                            html`<dt>${k}</dt>
                              <dd>${v}</dd>`,
                        )}
                      </dl>
                      ${a
                        ? html`<footer class="ld-card__badges">
                            ${a.watch ? html`<span>watch ${a.watch}</span>` : nothing}
                            ${a.job ? html`<span>job ${a.job}</span>` : nothing}
                          </footer>`
                        : nothing}
                    </article>
                    ${selected ? this.renderHubPanel(d) : nothing}
                  </div>
                `;
              })}
            </div>
          `}
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'live-devices': LiveDevices;
  }
}
