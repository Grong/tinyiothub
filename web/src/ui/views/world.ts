/**
 * E3 World（spec 2026-10-21-e3 §4）：zone 层级聚类。
 * 只做真实数据能支撑的空间语义（zone=祖先链），不做坐标/地图帧。
 */

import { LitElement, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import './world.css';
import { connectLive, type LiveConnection, type SseEvent } from '../../api/live.js';

interface WorldDevice {
  thingId: string;
  name: string;
  health: string;
  zone: string;
  online: boolean;
  values: Array<[string, string]> | Record<string, string>;
}

interface ZoneNode {
  path: string;
  label: string;
  devices: WorldDevice[];
  children: Map<string, ZoneNode>;
  worst: string;
}

const HEALTH_RANK: Record<string, number> = { bad: 0, attention: 1, unknown: 2, ok: 3 };

export function buildZones(devices: WorldDevice[]): ZoneNode {
  const root: ZoneNode = { path: '', label: '全部', devices: [], children: new Map(), worst: 'ok' };
  for (const device of devices) {
    const parts = (device.zone || '未归属区域').split('/').filter(Boolean);
    let node = root;
    let path = '';
    for (const part of parts) {
      path = path ? `${path}/${part}` : part;
      let child = node.children.get(part);
      if (!child) {
        child = { path, label: part, devices: [], children: new Map(), worst: 'ok' };
        node.children.set(part, child);
      }
      node = child;
    }
    node.devices.push(device);
  }
  const decorate = (node: ZoneNode): string => {
    let worst = 'ok';
    for (const d of node.devices) {
      if ((HEALTH_RANK[d.health] ?? 2) < (HEALTH_RANK[worst] ?? 2)) worst = d.health;
    }
    for (const child of node.children.values()) {
      const childWorst = decorate(child);
      if ((HEALTH_RANK[childWorst] ?? 2) < (HEALTH_RANK[worst] ?? 2)) worst = childWorst;
    }
    node.worst = worst;
    return worst;
  };
  decorate(root);
  return root;
}

function findZone(root: ZoneNode, path: string): ZoneNode {
  if (!path) return root;
  let node = root;
  for (const part of path.split('/')) {
    const child = node.children.get(part);
    if (!child) return node;
    node = child;
  }
  return node;
}

@customElement('world-view')
export class WorldView extends LitElement {
  @property({ type: String }) workspaceId = '';
  @state() private status: 'connecting' | 'open' | 'error' | 'closed' = 'connecting';
  @state() private devices: WorldDevice[] = [];
  @state() private path = '';
  @state() private selected: WorldDevice | null = null;
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
      onStatus: (status) => {
        this.status = status === 'connecting' ? 'connecting' : status === 'open' ? 'open' : status === 'error' ? 'error' : 'closed';
      },
      onEvent: (ev: SseEvent) => {
        if (ev.event === 'snapshot') {
          try {
            const data = JSON.parse(ev.data) as { devices: WorldDevice[] };
            this.devices = data.devices ?? [];
          } catch {
            // 坏帧忽略
          }
        }
      },
    });
  }

  private renderZone(node: ZoneNode, depth = 0): unknown {
    const children = [...node.children.values()].sort((a, b) => {
      const r = (HEALTH_RANK[a.worst] ?? 2) - (HEALTH_RANK[b.worst] ?? 2);
      return r !== 0 ? r : a.label.localeCompare(b.label, 'zh-CN');
    });
    return html`
      <div class="world-zone" data-depth=${depth}>
        ${children.map(
          (child) => html`
            <button class="world-zone__card" @click=${() => { this.path = child.path; this.selected = null; }}>
              <span class="world-zone__name">${child.label}</span>
              <span class="world-zone__health world-zone__health--${child.worst}">${child.worst}</span>
              <span class="world-zone__count">${child.devices.length + [...child.children.values()].reduce((n, c) => n + c.devices.length, 0)} 台</span>
            </button>
          `,
        )}
        ${node.devices.length
          ? html`<div class="world-devices">
              ${node.devices.map(
                (d) => html`
                  <button class="world-device" @click=${() => { this.selected = d; }}>
                    <span class="world-device__dot world-device__dot--${d.health}"></span>
                    <span class="world-device__name">${d.name}</span>
                    <span class="world-device__meta">${d.online ? '在线' : '离线'}</span>
                  </button>
                `,
              )}
            </div>`
          : nothing}
      </div>
    `;
  }

  render() {
    if (this.status === 'connecting') {
      return html`<div class="world-state">正在连接实时投影…</div>`;
    }
    if (this.status === 'error' || this.status === 'closed') {
      return html`
        <div class="world-state world-state--error">
          实时连接中断。
          <button class="btn btn--primary btn--sm" @click=${() => this.connect()}>重连</button>
        </div>
      `;
    }
    if (!this.devices.length) {
      return html`
        <div class="world-state world-state--empty">
          <div>还没有空间层级。</div>
          <div class="world-state__hint">在设备详情里设置上级空间，或使用场景包创建园区层级。</div>
        </div>
      `;
    }

    const root = buildZones(this.devices);
    const zone = findZone(root, this.path);
    const crumbs = this.path ? this.path.split('/') : [];

    return html`
      <div class="world">
        <nav class="world-breadcrumb" aria-label="空间层级">
          <button class="world-crumb" @click=${() => { this.path = ''; }}>全部</button>
          ${crumbs.map(
            (c, i) => html`
              <span class="world-crumb__sep">/</span>
              <button
                class="world-crumb"
                @click=${() => { this.path = crumbs.slice(0, i + 1).join('/'); }}
              >${c}</button>
            `,
          )}
        </nav>
        <div class="world-body">
          <div class="world-canvas">${this.renderZone(zone)}</div>
          <aside class="world-inspector" aria-live="polite">
            ${this.selected
              ? html`
                  <h3>${this.selected.name}</h3>
                  <div class="world-inspector__meta">${this.selected.zone} · ${this.selected.online ? '在线' : '离线'}</div>
                  <div class="world-inspector__health world-inspector__health--${this.selected.health}">${this.selected.health}</div>
                  <pre class="world-inspector__values">${JSON.stringify(this.selected.values ?? {}, null, 2)}</pre>
                `
              : html`<div class="world-inspector__empty">选择一个设备查看摘要</div>`}
          </aside>
        </div>
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'world-view': WorldView;
  }
}
