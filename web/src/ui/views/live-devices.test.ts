/**
 * E4b 设备卡接入 Hub 订阅流：
 * - 选中卡片 → 按驱动声明的 source 打开一条 Hub 流
 * - reading 合并、snapshot 替换；连接状态展示
 * - 切换选中/卸载 → 关闭上一条连接
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const liveMock = vi.hoisted(() => ({ connect: vi.fn() }));
const hubMock = vi.hoisted(() => ({ connect: vi.fn(), fetchSources: vi.fn() }));
const thingMock = vi.hoisted(() => ({ get: vi.fn() }));

vi.mock('../../api/live.js', () => ({ connectLive: liveMock.connect }));
vi.mock('../../api/hub-stream.js', () => ({
  connectHubStream: hubMock.connect,
  fetchDeclaredSources: hubMock.fetchSources,
}));
vi.mock('../../api/things.js', () => ({ thingApi: { get: thingMock.get } }));

import './live-devices.js';
import type { LiveDevices } from './live-devices.js';

const mounted: LiveDevices[] = [];

beforeEach(() => {
  vi.clearAllMocks();
  liveMock.connect.mockImplementation(() => ({ close: vi.fn(), pause: vi.fn(), resume: vi.fn() }));
  hubMock.connect.mockImplementation(() => ({ close: vi.fn(), pause: vi.fn(), resume: vi.fn() }));
  hubMock.fetchSources.mockResolvedValue(['telemetry', 'position']);
  thingMock.get.mockResolvedValue({ result: { driverName: 'demo' } });
});

afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
});

async function mount(): Promise<LiveDevices> {
  const el = document.createElement('live-devices') as LiveDevices;
  el.workspaceId = 'ws1';
  document.body.appendChild(el);
  mounted.push(el);
  await el.updateComplete;
  const liveHandlers = liveMock.connect.mock.calls[0][1];
  liveHandlers.onEvent({
    event: 'snapshot',
    data: JSON.stringify({
      v: 1,
      devices: [
        {
          thingId: 't1',
          name: '电表1',
          health: 'ok',
          zone: 'A',
          online: true,
          values: { temp: '21' },
        },
        {
          thingId: 't2',
          name: '电表2',
          health: 'ok',
          zone: 'A',
          online: true,
          values: { temp: '19' },
        },
      ],
    }),
  });
  await el.updateComplete;
  return el;
}

function cards(el: LiveDevices): HTMLElement[] {
  return [...(el.shadowRoot?.querySelectorAll<HTMLElement>('.ld-card') ?? [])];
}

describe('live-devices × Hub 订阅流', () => {
  it('选中卡片后按声明 source 订阅，并用 reading 合并、snapshot 替换', async () => {
    const el = await mount();
    cards(el)[0].click();
    await vi.waitFor(() => expect(hubMock.connect).toHaveBeenCalledTimes(1));

    expect(hubMock.connect.mock.calls[0][0]).toBe('ws1');
    expect(hubMock.connect.mock.calls[0][1]).toEqual(['t1']);
    expect(hubMock.connect.mock.calls[0][2]).toEqual(['telemetry', 'position']);

    const hubHandlers = hubMock.connect.mock.calls[0][3];
    hubHandlers.onStatus('live');
    hubHandlers.onSnapshot({
      v: 1,
      devices: [
        {
          thingId: 't1',
          name: '电表1',
          sources: [
            { source: 'telemetry', values: { temp: '22', hum: '50' }, lastHeartbeatAt: null },
          ],
        },
      ],
    });
    await el.updateComplete;
    const text = el.shadowRoot?.textContent ?? '';
    expect(text).toContain('实时');
    expect(text).toContain('telemetry · temp');
    expect(text).toContain('22');
    expect(text).toContain('50');

    hubHandlers.onReading({
      v: 1,
      ts: '2026-10-21T00:00:00Z',
      thingId: 't1',
      source: 'telemetry',
      values: { temp: '23' },
    });
    await el.updateComplete;
    const updated = el.shadowRoot?.textContent ?? '';
    expect(updated).toContain('23');
    expect(updated).toContain('50');
    expect(updated).toContain('实时');

    hubHandlers.onStatus('reconnecting');
    await el.updateComplete;
    expect(el.shadowRoot?.textContent).toContain('重连中');
  });

  it('切换选中与卸载都会关闭上一条连接', async () => {
    const el = await mount();
    cards(el)[0].click();
    await vi.waitFor(() => expect(hubMock.connect).toHaveBeenCalledTimes(1));
    const firstClose = hubMock.connect.mock.results[0].value.close;

    cards(el)[1].click();
    await vi.waitFor(() => expect(hubMock.connect).toHaveBeenCalledTimes(2));
    expect(firstClose).toHaveBeenCalledTimes(1);
    expect(hubMock.connect.mock.calls[1][1]).toEqual(['t2']);

    const secondClose = hubMock.connect.mock.results[1].value.close;
    el.remove();
    expect(secondClose).toHaveBeenCalledTimes(1);
  });

  it('键盘 Enter/Space 可选中卡片；打开详情发出 device-open 且不被卡片抢走', async () => {
    const el = await mount();
    const first = cards(el)[0];
    first.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await vi.waitFor(() => expect(hubMock.connect).toHaveBeenCalledTimes(1));
    expect(hubMock.connect.mock.calls[0][1]).toEqual(['t1']);

    cards(el)[1].dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true }));
    await vi.waitFor(() => expect(hubMock.connect).toHaveBeenCalledTimes(2));
    expect(hubMock.connect.mock.calls[1][1]).toEqual(['t2']);

    const opened: string[] = [];
    el.addEventListener('device-open', (e) =>
      opened.push((e as CustomEvent<{ thingId: string }>).detail.thingId),
    );
    const button = el.shadowRoot!.querySelector<HTMLButtonElement>('.ld-live__open')!;
    expect(button).toBeTruthy();
    // 详情按钮不在 role=button 卡片内部，避免交互控件嵌套。
    expect(cards(el)[1].contains(button)).toBe(false);

    // 冒泡到卡片的 Enter 不应被卡片 keydown 抢走（target 守卫）。
    button.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    expect(hubMock.connect).toHaveBeenCalledTimes(2);
    button.click();
    expect(opened).toEqual(['t2']);
  });

  it('A→B→A 时丢弃第一次 A 的过期声明结果', async () => {
    const el = await mount();
    let resolveFirst!: (sources: string[]) => void;
    const firstPending = new Promise<string[]>((resolve) => {
      resolveFirst = resolve;
    });
    hubMock.fetchSources
      .mockImplementationOnce(() => firstPending)
      .mockResolvedValue(['telemetry']);

    cards(el)[0].click(); // A(t1)：声明挂起
    await vi.waitFor(() => expect(hubMock.fetchSources).toHaveBeenCalledTimes(1));

    cards(el)[1].click(); // B(t2)
    await vi.waitFor(() => expect(hubMock.connect).toHaveBeenCalledTimes(1));
    expect(hubMock.connect.mock.calls[0][1]).toEqual(['t2']);

    cards(el)[0].click(); // A(t1) 再次选中
    await vi.waitFor(() => expect(hubMock.connect).toHaveBeenCalledTimes(2));
    expect(hubMock.connect.mock.calls[1][1]).toEqual(['t1']);

    resolveFirst(['position']); // 第一次 A 的过期结果
    await Promise.resolve();
    await el.updateComplete;
    expect(hubMock.connect).toHaveBeenCalledTimes(2);
    expect(hubMock.connect.mock.calls[1][2]).toEqual(['telemetry']);
  });

  it('驱动未声明 source 时显示离线且不建流', async () => {
    hubMock.fetchSources.mockResolvedValue([]);
    const el = await mount();
    cards(el)[0].click();
    await vi.waitFor(() => expect(hubMock.fetchSources).toHaveBeenCalledWith('demo'));
    await el.updateComplete;
    expect(hubMock.connect).not.toHaveBeenCalled();
    expect(el.shadowRoot?.textContent).toContain('驱动未声明数据源');
  });
});
