/**
 * DeviceCache — 浏览器侧设备数据缓存层
 *
 * 单例模式，持有唯一 SSE 连接，所有组件从信号读数据。
 * 首次 getDevices() 自动 fetch + 建立 SSE 连接，后续调用直接返回缓存。
 */

import { signal, computed } from '@lit-labs/signals';
import { deviceApi } from '../api/devices.js';
import { API_BASE } from '../api/config.js';
import { getAuthToken } from '../api/client.js';
import type { Device } from '../types/index.js';

type SseStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

class DeviceCache {
  // === Signals ===
  $devicesMap = signal(new Map<string, Device>());
  $sseStatus = signal<SseStatus>('disconnected');

  // Computed: 从 Map 派生有序数组
  $devicesList = computed(() => Array.from(this.$devicesMap.get().values()));

  // === Private ===
  private eventSource: EventSource | null = null;
  private reconnectTimer: number | null = null;
  private reconnectAttempt = 0;
  private fetchInProgress = false;
  private pendingSseEvents: any[] = [];
  private initialized = false;
  private hasConnectedOnce = false;

  /**
   * 获取设备列表。首次调用触发 fetch + SSE 自动连接，后续直接返回缓存。
   */
  async getDevices(): Promise<Device[]> {
    if (this.initialized) {
      return this.$devicesList.get();
    }
    this.initialized = true;

    await this.fetchAndPopulate();
    this.ensureConnected();
    return this.$devicesList.get();
  }

  /**
   * 强制刷新设备列表。
   */
  async refreshDevices(): Promise<void> {
    if (!this.initialized) return; // nothing to refresh if never initialized
    await this.fetchAndPopulate();
  }

  /**
   * 乐观更新属性值，同时异步调用 API。
   * API 失败时 rollback。
   */
  async updateProperty(
    deviceId: string,
    propertyName: string,
    value: any,
  ): Promise<void> {
    const map = this.$devicesMap.get();
    const device = map.get(deviceId);
    if (!device || !device.properties) return;

    // 保存旧值用于 rollback
    const oldProperties = device.properties;

    // 乐观更新
    const updatedProperties = device.properties.map((p) =>
      p.name === propertyName ? { ...p, currentValue: value, updatedAt: new Date().toISOString() } : p,
    );
    const updatedMap = new Map(map);
    updatedMap.set(deviceId, { ...device, properties: updatedProperties });
    this.$devicesMap.set(updatedMap);

    try {
      await deviceApi.updateDeviceProperty(deviceId, propertyName, value);
    } catch (err) {
      // Rollback
      const rollbackMap = this.$devicesMap.get();
      const current = rollbackMap.get(deviceId);
      if (current) {
        const rbMap = new Map(rollbackMap);
        rbMap.set(deviceId, { ...current, properties: oldProperties });
        this.$devicesMap.set(rbMap);
      }
      throw err;
    }
  }

  /**
   * 清空缓存，关闭 SSE 连接。登出时调用。
   */
  clearCache(): void {
    this.eventSource?.close();
    this.eventSource = null;
    if (this.reconnectTimer != null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.$devicesMap.set(new Map());
    this.$sseStatus.set('disconnected');
    this.reconnectAttempt = 0;
    this.fetchInProgress = false;
    this.pendingSseEvents = [];
    this.initialized = false;
    this.hasConnectedOnce = false;
  }

  // === Private methods ===

  private async fetchAndPopulate(): Promise<void> {
    this.fetchInProgress = true;
    this.pendingSseEvents = [];

    try {
      const response = await deviceApi.getDevices({ page: 1, pageSize: 100 });
      const devices = response.result?.data ?? [];

      const map = new Map<string, Device>();
      for (const d of devices) {
        map.set(d.id, d);
      }

      // 合并 fetch 期间积压的 SSE 事件
      for (const evt of this.pendingSseEvents) {
        this.applySseEventToMap(map, evt);
      }

      this.$devicesMap.set(map);
    } catch {
      // fetch 失败: 对旧数据也应用新事件（这些事件比旧缓存更新）
      const map = this.$devicesMap.get();
      for (const evt of this.pendingSseEvents) {
        const updated = this.applySseEventToMap(map, evt);
        if (updated) this.$devicesMap.set(updated);
      }
    } finally {
      this.pendingSseEvents = [];
      this.fetchInProgress = false;
    }
  }

  private ensureConnected(): void {
    if (this.eventSource != null) return;

    this.$sseStatus.set('connecting');

    const token =
      getAuthToken();
    if (!token) {
      this.$sseStatus.set('disconnected');
      return;
    }

    const url = `${API_BASE}/events/sse?token=${encodeURIComponent(token)}&event_types=device.status_change,device.connection,device.property_change`;

    try {
      this.eventSource = new EventSource(url);
    } catch {
      this.$sseStatus.set('error');
      this.scheduleReconnect();
      return;
    }

    this.eventSource.onopen = () => {
      this.$sseStatus.set('connected');
      this.reconnectAttempt = 0;
      this.hasConnectedOnce = true;
    };

    this.eventSource.onmessage = (ev) => {
      try {
        const data = JSON.parse(ev.data);
        this.handleSseEvent(data);
      } catch {
        // ignore malformed events
      }
    };

    this.eventSource.onerror = () => {
      this.$sseStatus.set('error');
      this.eventSource?.close();
      this.eventSource = null;

      // 首次连接失败（从未成功过）→ 可能是 token 过期
      if (!this.hasConnectedOnce) {
        window.dispatchEvent(
          new CustomEvent('auth-error', { detail: { message: 'SSE 认证失败' } }),
        );
        return;
      }

      this.scheduleReconnect();
    };
  }

  private scheduleReconnect(): void {
    const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempt), 30000);
    this.reconnectAttempt++;
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      this.ensureConnected();
    }, delay);
  }

  private handleSseEvent(data: any): void {
    if (this.fetchInProgress) {
      this.pendingSseEvents.push(data);
      return;
    }

    const map = this.$devicesMap.get();
    const updated = this.applySseEventToMap(map, data);
    if (updated) {
      this.$devicesMap.set(updated);

      // 通知详情页需要刷新
      const deviceId = data.device_id;
      if (deviceId) {
        window.dispatchEvent(new CustomEvent('device-updated', {
          detail: { deviceId, eventType: data.event_type, data },
        }));
      }
    }
  }

  /**
   * 将 SSE 事件应用到 Map 上，返回新 Map（若有变更）或 null。
   */
  private applySseEventToMap(
    map: Map<string, Device>,
    data: any,
  ): Map<string, Device> | null {
    const eventType: string = data.event_type ?? '';
    const deviceId: string | undefined = data.device_id;

    if (!deviceId) return null;
    const device = map.get(deviceId);
    if (!device) return null;

    // device.connection / device.status_change
    if (eventType === 'device.connection' || eventType === 'device.status_change') {
      const newStatus = data.status ?? data.content?.status;
      if (newStatus && newStatus !== device.status) {
        const updated = new Map(map);
        updated.set(deviceId, { ...device, status: newStatus });
        return updated;
      }
    }

    // device.property_change
    if (eventType === 'device.property_change') {
      const propertyName: string | undefined = data.property_name;
      const newValue: string | undefined = data.new_value;
      if (!propertyName || !device.properties) return null;

      const prop = device.properties.find((p) => p.name === propertyName);
      if (prop && prop.currentValue !== newValue) {
        const updatedProps = device.properties.map((p) =>
          p.name === propertyName
            ? { ...p, currentValue: newValue, updatedAt: data.timestamp ?? new Date().toISOString() }
            : p,
        );
        const updated = new Map(map);
        updated.set(deviceId, { ...device, properties: updatedProps });
        return updated;
      }
    }

    return null;
  }
}

export const deviceCache = new DeviceCache();
