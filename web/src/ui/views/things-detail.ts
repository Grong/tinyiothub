// Extracted from things.ts (eng-review T14 god-file split).
// Render helpers take the host view instance; behavior unchanged.
import { html, nothing } from "lit";
import { deviceCache } from "../../stores/device-cache.js";
import { icons } from "../icons.js";
import type { DeviceProperty, DeviceEvent, Tag } from "../../types/index.js";
import type { DevicesView } from "./things.js";

export function renderHistoryDialog(host: DevicesView) {
    if (!host.showHistoryDialog) return nothing;

    const ranges = [
      { key: "30m", label: "30分钟" },
      { key: "1h", label: "1小时" },
      { key: "5h", label: "5小时" },
      { key: "24h", label: "24小时" },
      { key: "custom", label: "自定义" },
    ];

    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label="历史曲线" @click=${host.closeHistoryDialog} @keydown=${(e: KeyboardEvent) => host.handleModalKeydown(e, host.closeHistoryDialog)}>
        <div class="modal modal--wide" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <span>${host.historyPropertyName}${host.historyPropertyUnit ? ` (${host.historyPropertyUnit})` : ""} — 历史曲线</span>
            <button class="btn btn--icon" aria-label="关闭" @click=${host.closeHistoryDialog}>×</button>
          </div>
          <div class="modal-body history-modal-body">
            <!-- Time range selector -->
            <div class="time-range-bar">
              ${ranges.map(r => html`
                <button
                  class="time-range-btn ${host.historyRange === r.key ? 'time-range-btn--active' : ''}"
                  @click=${() => host.onHistoryRangeChange(r.key)}
                >${r.label}</button>
              `)}
            </div>
            ${host.historyRange === "custom" ? html`
              <div class="time-range-inputs">
                <label>开始</label>
                <input type="datetime-local"
                  .value=${host.historyCustomStart}
                  @change=${(e: Event) => { host.historyCustomStart = (e.target as HTMLInputElement).value; }}
                />
                <label>结束</label>
                <input type="datetime-local"
                  .value=${host.historyCustomEnd}
                  @change=${(e: Event) => { host.historyCustomEnd = (e.target as HTMLInputElement).value; }}
                />
                <button class="btn time-range-query-btn"
                  @click=${host.onHistoryCustomTimeApply}
                >查询</button>
              </div>
            ` : nothing}
            <!-- Chart -->
            ${host.historyLoading
              ? html`<div class="history-chart-placeholder">加载中...</div>`
              : host.historyData.length === 0
                ? html`<div class="history-chart-placeholder">暂无历史数据</div>`
                : html`<div id="history-chart-container" class="history-chart-container">
                    <canvas id="history-chart"></canvas>
                  </div>`
            }
          </div>
        </div>
      </div>
    `;
  }


export function drawHistoryChart(host: DevicesView) {
    const canvas = host.querySelector("#history-chart") as HTMLCanvasElement;
    if (!canvas || host.historyData.length === 0) return;

    const container = host.querySelector("#history-chart-container") as HTMLElement;
    if (!container) return;

    const dpr = window.devicePixelRatio || 1;
    const w = container.clientWidth;
    const h = container.clientHeight;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    canvas.style.width = w + "px";
    canvas.style.height = h + "px";

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.scale(dpr, dpr);

    const data = host.historyData;
    const padding = { top: 24, right: 20, bottom: 36, left: 56 };
    const chartW = w - padding.left - padding.right;
    const chartH = h - padding.top - padding.bottom;

    const values = data.map(d => d.value);
    let minVal = Math.min(...values);
    let maxVal = Math.max(...values);
    if (minVal === maxVal) { minVal -= 1; maxVal += 1; }
    const range = maxVal - minVal;

    const cs = getComputedStyle(document.documentElement);
    const textColor = cs.getPropertyValue("--muted").trim() || "#888";
    const lineColor = cs.getPropertyValue("--accent").trim() || "#6366f1";
    const gridColor = cs.getPropertyValue("--border").trim() || "#e5e7eb";

    ctx.clearRect(0, 0, w, h);

    // Grid lines + Y labels
    ctx.strokeStyle = gridColor;
    ctx.lineWidth = 0.5;
    ctx.fillStyle = textColor;
    ctx.font = "11px system-ui, sans-serif";
    ctx.textAlign = "right";
    const yTicks = 5;
    for (let i = 0; i <= yTicks; i++) {
      const y = padding.top + (chartH / yTicks) * i;
      const val = maxVal - (range / yTicks) * i;
      ctx.beginPath();
      ctx.moveTo(padding.left, y);
      ctx.lineTo(w - padding.right, y);
      ctx.stroke();
      ctx.fillText(val.toFixed(1), padding.left - 6, y + 4);
    }

    // X labels
    ctx.textAlign = "center";
    const xLabelCount = Math.min(data.length, 6);
    const xStep = Math.max(1, Math.floor(data.length / xLabelCount));
    for (let i = 0; i < data.length; i += xStep) {
      const x = padding.left + (chartW / (data.length - 1)) * i;
      const label = data[i].time.slice(5, 16);
      ctx.fillText(label, x, h - padding.bottom + 16);
    }

    // Line
    ctx.strokeStyle = lineColor;
    ctx.lineWidth = 2;
    ctx.lineJoin = "round";
    ctx.beginPath();
    for (let i = 0; i < data.length; i++) {
      const x = padding.left + (chartW / (data.length - 1)) * i;
      const y = padding.top + chartH - ((data[i].value - minVal) / range) * chartH;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    // Dots
    ctx.fillStyle = lineColor;
    for (let i = 0; i < data.length; i++) {
      const x = padding.left + (chartW / (data.length - 1)) * i;
      const y = padding.top + chartH - ((data[i].value - minVal) / range) * chartH;
      ctx.beginPath();
      ctx.arc(x, y, 3, 0, Math.PI * 2);
      ctx.fill();
    }
  }


export function renderDeviceDetail(host: DevicesView) {
    const profile = host.selectedDevice;
    if (!profile) return nothing;
    const d = profile.device;
    const ov = profile.overview;
    const deviceTags: Tag[] = (d as any).tags || [];

    return html`
      <!-- Header: name, status, type, tags, edit -->
      <div class="card detail-header">
        <div class="detail-header__row">
          <div class="detail-header__main">
            <button class="btn btn--ghost btn--sm detail-header__back" @click=${host.backToList}>
              &larr; 返回
            </button>
            <h2 class="detail-header__title">${d.displayName || d.name}</h2>
            <span class="status-badge status-badge--subtle">
              <span class="status-dot status-dot--sm" style="background: ${host.statusColor(d.status)};"></span>
              <span class="status-badge__label">${host.statusLabel(d.status)}</span>
            </span>
            ${d.deviceType ? html`
              <span class="type-tag">${d.deviceType}</span>
            ` : nothing}
          </div>
          <button class="btn btn--ghost btn--sm" @click=${() => host.openEdit(d)}>编辑</button>
        </div>
        ${deviceTags.length > 0 ? html`
          <div class="detail-header__tags">
            ${deviceTags.map((t: Tag) => html`
              <span class="tag-pill">${t.name}</span>
            `)}
          </div>
        ` : nothing}
      </div>

      <!-- Mini stat grid -->
      <div class="detail-stat-grid">
        <div class="stat">
          <div class="stat-label">属性总数</div>
          <div class="stat-value">${ov.totalProperties}</div>
        </div>
        <div class="stat">
          <div class="stat-label">在线属性</div>
          <div class="stat-value ok">${ov.onlineProperties}</div>
        </div>
        <div class="stat">
          <div class="stat-label">命令数</div>
          <div class="stat-value">${ov.totalCommands}</div>
        </div>
        <div class="stat">
          <div class="stat-label">活跃告警</div>
          <div class="stat-value ${ov.activeAlarms > 0 ? 'warn' : ''}">${ov.activeAlarms}</div>
        </div>
      </div>

      <!-- Tab bar -->
      <div class="detail-tabs">
        <button class="detail-tab ${host.detailTab === 'properties' ? 'active' : ''}" @click=${() => host.switchDetailTab('properties')}>${icons.barChart} 属性</button>
        <button class="detail-tab ${host.detailTab === 'commands' ? 'active' : ''}" @click=${() => host.switchDetailTab('commands')}>${icons.zap} 命令</button>
        <button class="detail-tab ${host.detailTab === 'events' ? 'active' : ''}" @click=${() => host.switchDetailTab('events')}>${icons.scrollText} 事件</button>
        <button class="detail-tab ${host.detailTab === 'alarms' ? 'active' : ''}" @click=${() => host.switchDetailTab('alarms')}>${icons.bug} 告警</button>
        <button class="detail-tab ${host.detailTab === 'knowledge' ? 'active' : ''}" @click=${() => host.switchDetailTab('knowledge')}>${icons.fileText} 知识</button>
      </div>

      <!-- Tab content -->
      ${host.detailTab === 'properties' ? host.renderDetailProperties() : nothing}
      ${host.detailTab === 'commands' ? host.renderDetailCommands() : nothing}
      ${host.detailTab === 'events' ? host.renderDetailEvents() : nothing}
      ${host.detailTab === 'alarms' ? host.renderDetailAlarms() : nothing}
      ${host.detailTab === 'knowledge' ? host.renderDetailKnowledge() : nothing}
      ${host.showModal ? host.renderModal() : nothing}
      ${host.showHistoryDialog ? host.renderHistoryDialog() : nothing}
      ${host.showResourceModal ? host.renderResourceModal() : nothing}
    `;
  }


export function renderDetailProperties(host: DevicesView) {
    const profile = host.selectedDevice;
    if (!profile) return html`<div class="card empty-center">暂无属性数据</div>`;

    // 从缓存读取（SSE 推送的实时数据），用 profile.properties 的元数据补充缺失字段
    const cached = deviceCache.$devicesMap.get().get(profile.device.id);
    let properties: DeviceProperty[] = [];

    if (cached?.properties?.length) {
      // 有缓存：用 API 属性元数据 + 缓存实时值
      const apiMap = new Map((profile.properties ?? []).map(p => [p.name, p]));
      properties = cached.properties.map(cachedProp => {
        const apiProp = apiMap.get(cachedProp.name);
        return apiProp
          ? { ...apiProp, currentValue: cachedProp.currentValue ?? cachedProp.value, updatedAt: cachedProp.updatedAt }
          : cachedProp;
      });
    } else if (profile.properties?.length) {
      // 无缓存：用 API 属性
      properties = profile.properties;
    }

    if (properties.length === 0) {
      return html`<div class="card empty-center">暂无属性数据</div>`;
    }

    return html`
      <div class="card prop-table-wrap">
        <table class="data-table--compact">
          <thead>
            <tr>
              <th>属性</th>
              <th>名称</th>
              <th>当前值</th>
              <th></th>
              <th>类型</th>
              <th class="cell-actions">读写</th>
              <th>更新时间</th>
            </tr>
          </thead>
          <tbody>
            ${properties.map((p: DeviceProperty) => html`
              <tr>
                <td>${p.name}</td>
                <td>${p.displayName || p.name}</td>
                <td>
                  <span class="prop-value">${p.currentValue ?? p.value ?? "-"}</span>
                  ${p.unit ? html`<span class="prop-unit">${p.unit}</span>` : nothing}
                </td>
                <td class="cell-actions">
                  ${host.isNumericType(p.dataType) ? html`
                    <button
                      class="btn btn--icon btn--xs"
                      title="曲线"
                      aria-label="历史曲线"
                      @click=${() => host.openPropertyHistory(p.name, p.unit || "")}
                    >${icons.trendingUp}</button>
                  ` : nothing}
                </td>
                <td class="prop-type">${p.dataType}</td>
                <td class="cell-actions">
                  <span class="${p.isReadOnly ? 'prop-ro-badge' : 'prop-rw-badge'}">
                    ${p.isReadOnly ? '只读' : '读写'}
                  </span>
                </td>
                <td class="prop-type">${p.updatedAt?.slice(0, 16) || "-"}</td>
              </tr>
            `)}
          </tbody>
        </table>
      </div>
    `;
  }


export function renderDetailCommands(host: DevicesView) {
    const profile = host.selectedDevice;
    if (!profile) return nothing;
    const d = profile.device;

    if (profile.commands.length === 0) {
      return html`<div class="card empty-center">暂无命令</div>`;
    }

    return html`
      <div class="card command-list-wrap">
        <div class="command-list">
          ${profile.commands.map(c => html`
            <div class="command-item">
              <div>
                <div class="command-item__name">${c.name}</div>
                <div class="command-item__desc">${c.description || "无描述"}</div>
              </div>
              <button
                class="btn btn--primary btn--sm"
                ?disabled=${host.executingCommand === c.name}
                @click=${() => host.executeCommand(d.id, c.name)}
              >
                ${host.executingCommand === c.name ? "执行中..." : "执行"}
              </button>
            </div>
          `)}
        </div>
      </div>
    `;
  }


export function renderDetailEvents(host: DevicesView) {
    const profile = host.selectedDevice;
    if (!profile) return nothing;

    const events = profile.recentEvents || [];
    if (events.length === 0) {
      return html`<div class="card empty-center">暂无事件记录</div>`;
    }

    const levelClass = (level: string) => {
      switch (level) {
        case 'info': return 'event-badge--info';
        case 'warning': return 'event-badge--warning';
        case 'error': return 'event-badge--error';
        case 'critical': return 'event-badge--critical';
        default: return 'event-badge--info';
      }
    };

    const levelLabel = (level: string) => {
      switch (level) {
        case 'info': return '信息';
        case 'warning': return '警告';
        case 'error': return '错误';
        case 'critical': return '严重';
        default: return level;
      }
    };

    return html`
      <div class="card events-list-wrap">
        ${events.map((ev: DeviceEvent) => html`
          <div class="event-item">
            <span class="event-badge ${levelClass(ev.level)}">${levelLabel(ev.level)}</span>
            <div class="event-item__body">
              <div class="event-item__title">${ev.title}</div>
              ${ev.message ? html`<div class="event-item__message">${ev.message}</div>` : nothing}
            </div>
            <span class="event-item__time">${ev.createdAt?.slice(0, 16)}</span>
          </div>
        `)}
      </div>
    `;
  }


export function renderDetailAlarms(host: DevicesView) {
    const profile = host.selectedDevice;
    if (!profile) return nothing;
    const properties = profile.properties || [];

    return html`
      <!-- Active alarms — real-time display -->
      <div class="card" style="margin-top: var(--space-4);">
        <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: var(--space-3);">
          <div>
            <div class="alarm-rules-card__title">实时告警</div>
            <div class="alarm-rules-card__sub">当前活跃的告警，恢复正常后自动消失</div>
          </div>
          <button class="btn btn--ghost btn--xs" @click=${host.loadDeviceAlarms} ?disabled=${host.alarmsLoading}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14" style="vertical-align: -2px;">
              <polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
            </svg>
          </button>
        </div>
        ${host.alarmsLoading
          ? html`<div class="alarm-rules-card__loading"><span class="loading-spinner"></span> 加载中...</div>`
          : host.deviceAlarms.length === 0
            ? html`<div class="alarm-rules-empty">
                <div class="alarm-rules-empty__text" style="color: var(--success);">🎉 无活跃告警</div>
                <div class="alarm-rules-empty__hint">一切正常</div>
              </div>`
            : html`
              <div class="alarm-rules-list">
                ${host.deviceAlarms.map((a: any) => html`
                  <div class="alarm-rule-item" style="animation: ruleFadeIn 0.35s var(--ease-out) both;">
                    <div class="alarm-rule-item__main">
                      <div class="alarm-rule-item__header">
                        <span class="alarm-rule-badge alarm-rule-badge--${(a.alarmLevel || '').toLowerCase()}">${host.levelLabel2(a.alarmLevel || '')}</span>
                        <span class="alarm-rule-item__name">${a.message}</span>
                      </div>
                      <div class="alarm-rule-item__meta">
                        <span>${(a.alarmTime || a.createdAt || '').slice(0, 16)}</span>
                      </div>
                    </div>
                  </div>
                `)}
              </div>
            `
        }
      </div>

      <!-- Alarm rules section -->
      <div class="card alarm-rules-card">
        <div class="alarm-rules-card__header">
          <div>
            <div class="alarm-rules-card__title">告警规则</div>
            <div class="alarm-rules-card__sub">管理物的自动告警规则</div>
          </div>
          <button class="btn btn--primary btn--sm" @click=${host.openNewRule}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14" class="btn__icon-left">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            添加规则
          </button>
        </div>

        ${host.rulesLoading
          ? html`<div class="empty-center alarm-rules-card__loading"><span class="loading-spinner"></span> 加载中...</div>`
          : host.alarmRules.length === 0
            ? html`
              <div class="alarm-rules-empty">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="40" height="40" class="alarm-rules-empty__icon">
                  <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
                  <line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
                </svg>
                <div class="alarm-rules-empty__text">暂无告警规则</div>
                <div class="alarm-rules-empty__hint">添加规则后，物数据变化将自动触发告警</div>
              </div>
            `
            : html`
              <div class="alarm-rules-list">
                ${host.alarmRules.map(rule => {
                  const condSummary = host.formatCondition(rule.condition);
                  const propName = properties.find(p => p.id === rule.propertyId)?.displayName || properties.find(p => p.name === rule.propertyId)?.displayName || rule.propertyId || "—";
                  return html`
                    <div class="alarm-rule-item ${rule.isEnabled ? '' : 'alarm-rule-item--disabled'}" style="animation: ruleFadeIn 0.35s var(--ease-out) both; animation-delay: ${Math.min(host.alarmRules.indexOf(rule) * 50, 300)}ms;">
                      <div class="alarm-rule-item__main">
                        <div class="alarm-rule-item__header">
                          <span class="alarm-rule-item__name">${rule.name}</span>
                          <span class="alarm-rule-badge alarm-rule-badge--${rule.alarmLevel.toLowerCase()}">${host.levelLabel2(rule.alarmLevel)}</span>
                          ${rule.notificationConfig?.enabled
                            ? html`<span class="alarm-rule-item__notify-icon" title="通知已开启">🔔</span>`
                            : nothing
                          }
                        </div>
                        <div class="alarm-rule-item__meta">
                          <span>属性: ${propName}</span>
                          <span>条件: ${condSummary}</span>
                        </div>
                      </div>
                      <div class="alarm-rule-item__actions">
                        <label class="toggle-switch" title=${rule.isEnabled ? "已启用" : "已禁用"}>
                          <input type="checkbox" .checked=${rule.isEnabled} @change=${() => host.toggleRule(rule)} />
                          <span class="toggle-switch__slider"></span>
                        </label>
                        <button class="btn btn--ghost btn--xs" @click=${() => host.openEditRule(rule)}>编辑</button>
                        <button class="btn btn--ghost btn--xs btn--danger-text" @click=${() => host.deleteRule(rule)}>删除</button>
                      </div>
                    </div>
                  `;
                })}
              </div>
            `
        }
      </div>

      <!-- Rule editor modal -->
      ${host.showRuleModal ? host.renderRuleModal(profile.device.id, properties) : nothing}
    `;
  }


export function renderDetailKnowledge(host: DevicesView) {
    const profile = host.selectedDevice;
    if (!profile) return nothing;
    const docs = (profile as any).knowledgeDocs || [];

    return html`
      <div class="card" style="margin-top: var(--space-4);">
        <div class="kb-header">
          <div>
            <div class="kb-header__title">知识文档</div>
            <div class="kb-header__sub">${docs.length ? `${docs.length} 个文档` : '上传手册、图纸、数据表等文档'}</div>
          </div>
          <button class="btn btn--ghost btn--sm" @click=${host.openAddResourceModal}>
            <span style="margin-right:4px;">${icons.plus}</span>添加
          </button>
        </div>
        ${docs.length === 0 ? html`
          <div class="kb-empty">
            <div class="kb-empty__icon">
              <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="1" width="48" height="48" opacity="0.3">
                <rect x="8" y="4" width="32" height="40" rx="2" /><line x1="16" y1="16" x2="32" y2="16" /><line x1="16" y1="22" x2="28" y2="22" /><line x1="16" y1="28" x2="24" y2="28" />
              </svg>
            </div>
            <div class="kb-empty__title">还没有文档</div>
            <div class="kb-empty__hint">点击上方「添加」上传文件或关联已有资源</div>
          </div>
        ` : html`
          <div class="model-grid">
            ${docs.map((doc: any) => {
              const docTags = (typeof doc.tags === 'string' ? JSON.parse(doc.tags || '[]') : doc.tags) || [];
              const visibleTags = docTags.slice(0, 3);
              const hiddenCount = docTags.length - 3;
              return html`
              <div class="device-card__wrap" style="overflow:visible;">
                <div class="card device-card" style="overflow:visible;contain:none;">
                  <div class="device-card__header">
                    <div class="device-card__header-left">
                      <span class="device-card__title" title="${doc.name}">${doc.name || doc.filePath || '未命名'}</span>
                      ${doc.createdAt ? html`<span class="device-card__gateway-tag">${doc.createdAt.slice(0, 10)}</span>` : nothing}
                    </div>
                    <div class="device-card__actions">
                      <button class="btn btn--ghost btn--sm device-card__action-btn btn--danger-text" title="移除" @click=${(e: Event) => { e.stopPropagation(); host.removeKnowledgeDoc(doc); }}>${icons.trash2}</button>
                    </div>
                  </div>
                  <div class="device-card__body" @click=${(e: Event) => { e.stopPropagation(); if (host.editingDescId === doc.id) return; host.editingDescId = doc.id; host._editDescValue = doc.description || ''; host.requestUpdate(); }}>
                    <div class="device-card__info">
                      ${host.editingDescId === doc.id ? html`
                        <input class="kb-card__edit-input" .value=${host._editDescValue} placeholder="添加描述…" @input=${(e: Event) => { host._editDescValue = (e.target as HTMLInputElement).value; }}
                          @keydown=${(e: KeyboardEvent) => { if (e.key === 'Enter') { e.preventDefault(); host.saveDocDesc(doc); } }}
                          @blur=${() => host.saveDocDesc(doc)} />
                      ` : html`
                        ${doc.description ? html`<div class="device-card__info-row"><span class="device-card__info-value">${doc.description}</span></div>`
                          : html`<div class="device-card__info-row"><span class="device-card__info-value" style="color:var(--muted);font-style:italic;">点击添加描述</span></div>`}
                      `}
                      <div class="device-card__info-row">
                        <span class="device-card__info-label">类型</span>
                        <span class="device-card__info-value">${doc.resourceType || 'file'}</span>
                      </div>
                    </div>
                  </div>
                  <div class="device-card__footer" style="cursor:pointer;min-height:28px;position:relative;" @click=${(e: Event) => { e.stopPropagation(); if (host.editingDocId === doc.id) { host.editingDocId = null; } else { host.startEditDoc(doc); } }}>
                    ${visibleTags.map((t: any) => html`<span class="tag-pill">${typeof t === 'string' ? t : t.name || t}</span>`)}
                    ${hiddenCount > 0 ? html`<span class="tag-pill tag-pill--muted" title="${docTags.slice(3).map((t: any) => typeof t === 'string' ? t : t.name || t).join(', ')}">+${hiddenCount}</span>` : nothing}
                    ${docTags.length === 0 ? html`<span class="inline-muted" style="font-size: 12px;">添加标签</span>` : nothing}
                    ${host.editingDocId === doc.id ? host.renderDocTagPopover() : nothing}
                  </div>
                </div>
              </div>
            `; })}
          </div>
        `}
      </div>
    `;
  }

