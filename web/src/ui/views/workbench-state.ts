/**
 * E3 工作台 URL 状态（spec 2026-10-21-e3 §2.1）：
 * `/things?tab=flow&at=<RFC3339>&event=<ledgerId>&device=<thingId>`。
 * 纯函数，便于测试与 popstate 恢复。
 */

export type WorkbenchTab = 'devices' | 'flow' | 'world';

export interface WorkbenchState {
  tab: WorkbenchTab;
  at: string | null;
  event: number | null;
  device: string | null;
}

const TABS: WorkbenchTab[] = ['devices', 'flow', 'world'];

export function parseWorkbenchUrl(search: string): WorkbenchState {
  const params = new URLSearchParams(search.startsWith('?') ? search : `?${search}`);
  const rawTab = params.get('tab');
  const tab = TABS.includes(rawTab as WorkbenchTab) ? (rawTab as WorkbenchTab) : 'devices';
  const rawEvent = params.get('event');
  const event = rawEvent && /^\d+$/.test(rawEvent) ? Number(rawEvent) : null;
  return {
    tab,
    at: params.get('at') || null,
    event,
    device: params.get('device') || null,
  };
}

export function toWorkbenchUrl(state: WorkbenchState): string {
  const params = new URLSearchParams();
  if (state.tab !== 'devices') params.set('tab', state.tab);
  if (state.at) params.set('at', state.at);
  if (state.event != null) params.set('event', String(state.event));
  if (state.device) params.set('device', state.device);
  const qs = params.toString();
  return qs ? `?${qs}` : '';
}

/** 有 at/event/device 任一即历史模式（动作只读）。 */
export function isHistorical(state: WorkbenchState): boolean {
  return Boolean(state.at || state.event != null || state.device);
}

/** 「回到现在」：清空历史锚点并留在当前 tab。 */
export function backToNow(state: WorkbenchState): WorkbenchState {
  return { tab: state.tab === 'world' ? 'world' : 'flow', at: null, event: null, device: null };
}
