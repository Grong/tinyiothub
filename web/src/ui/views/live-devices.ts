/**
 * E3 实时设备健康卡（spec §2.2 / 计划 T6）：
 * SSE snapshot 驱动的 worst-first 卡片；watch/job 活动徽标；断线重连横幅。
 */

import { LitElement, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import './live-devices.css';
import { connectLive, type LiveConnection, type SseEvent } from '../../api/live.js';

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

function sorted(devices: LiveDevice[]): LiveDevice[] {
  return [...devices].sort((a, b) => {
    const rank = (HEALTH_RANK[a.health] ?? 2) - (HEALTH_RANK[b.health] ?? 2);
    if (rank !== 0) return rank;
    const zone = a.zone.localeCompare(b.zone, 'zh-CN');
    return zone !== 0 ? zone : a.name.localeCompare(b.name, 'zh-CN');
  });
}

@customElement('live-devices')
export class LiveDevices extends LitElement {
  @property({ type: String }) workspaceId = '';
  @state() private devices: LiveDevice[] = [];
  @state() private status: 'connecting' | 'open' | 'error' | 'closed' = 'connecting';
  @state() private activity: Record<string, Activity> = {};
  private live: LiveConnection | null = null;

  connectedCallback(): void {
    super.connectedCallback();
    this.connect();
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this.live?.close();
    this.live = null;
  }

  private connect(): void {
    if (!this.workspaceId) return;
    this.live?.close();
    this.live = connectLive(this.workspaceId, {
      onStatus: (status) =>
        (this.status = status === 'connecting' ? 'connecting' : status === 'open' ? 'open' : status === 'error' ? 'error' : 'closed'),
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
      this.devices = ((data.devices as LiveDevice[]) ?? []).map((d) => ({
        ...d,
        values: Array.isArray(d.values) ? Object.fromEntries(d.values as Array<[string, string]>) : d.values ?? {},
      }));
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

  private open(thingId: string): void {
    this.dispatchEvent(
      new CustomEvent('device-open', { detail: { thingId }, bubbles: true, composed: true }),
    );
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
                const values = Object.entries(d.values ?? {}).slice(0, 3);
                return html`
                  <article
                    class="ld-card ld-card--${d.health}"
                    tabindex="0"
                    role="button"
                    aria-label="${d.name} ${d.health} ${d.online ? '在线' : '离线'}"
                    @click=${() => this.open(d.thingId)}
                    @keydown=${(e: KeyboardEvent) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        this.open(d.thingId);
                      }
                    }}
                  >
                    <header>
                      <span class="ld-card__dot ld-card__dot--${d.health}" aria-hidden="true"></span>
                      <span class="ld-card__name">${d.name}</span>
                      <span class="ld-card__health">${d.health}</span>
                    </header>
                    <div class="ld-card__zone">${d.zone || '未归属区域'} · ${d.online ? '在线' : '离线'}</div>
                    <dl class="ld-card__values">
                      ${values.map(
                        ([k, v]) => html`<dt>${k}</dt><dd>${v}</dd>`,
                      )}
                    </dl>
                    ${a
                      ? html`<footer class="ld-card__badges">
                          ${a.watch ? html`<span>watch ${a.watch}</span>` : nothing}
                          ${a.job ? html`<span>job ${a.job}</span>` : nothing}
                        </footer>`
                      : nothing}
                  </article>
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
