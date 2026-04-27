/**
 * DeviceCache — 浏览器侧设备数据缓存层
 *
 * 单例模式，持有唯一 SSE 连接，所有组件从信号读数据。
 * 缓存从空开始，通过 SSE 推送和设备详情加载逐步填充。
 * 不做全量 fetch，适合大量设备场景。
 */

import { signal, computed } from '@lit-labs/signals';
import { deviceApi } from '../api/devices.js';
import { API_BASE } from '../api/config.js';
import { getAuthToken } from '../api/client.js';
import type { Device, DeviceProperty } from '../types/index.js';

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
  private pendingSseEvents: any[] = [];
  private sseConnecting = false;

  /**
   * 获取当前缓存的设备列表。
   * 同时确保 SSE 连接已建立（静默，不 fetch）。
   */
  async getDevices(): Promise<Device[]> {
    this.ensureConnected();
    return this.$devicesList.get();
  }

  /**
   * 强制刷新：重新建立 SSE 连接（不全量 fetch）。
   */
  async refreshDevices(): Promise<void> {
    this.disconnect();
    this.ensureConnected();
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

    const oldProperties = device.properties;

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
   * 批量更新设备的完整属性（含元数据），用于详情页加载时初始化。
   * 不触发 SSE 事件。
   */
  setDeviceProperties(deviceId: string, properties: DeviceProperty[]): void {
    const map = this.$devicesMap.get();
    const device = map.get(deviceId);
    const updated = new Map(map);
    updated.set(deviceId, {
      ...(device ?? { id: deviceId, name: deviceId, status: 'online' }),
      properties,
    });
    this.$devicesMap.set(updated);
  }

  /**
   * 添加或更新单个设备到缓存。
   */
  setDevice(device: Device): void {
    const map = new Map(this.$devicesMap.get());
    map.set(device.id, device);
    this.$devicesMap.set(map);
  }

  /**
   * 从缓存移除设备。
   */
  removeDevice(deviceId: string): void {
    const map = new Map(this.$devicesMap.get());
    map.delete(deviceId);
    this.$devicesMap.set(map);
  }

  /**
   * 强制触发所有 $devicesMap 的订阅者 re-render。
   */
  touchForRerender(): void {
    const map = this.$devicesMap.get();
    this.$devicesMap.set(map);
  }

  /**
   * 清空缓存，关闭 SSE 连接。登出时调用。
   */
  clearCache(): void {
    this.disconnect();
    this.$devicesMap.set(new Map());
    localStorage.removeItem("workspace-id");
    sessionStorage.removeItem("workspace-id");
    this.pendingSseEvents = [];
  }

  // === Private methods ===

  private disconnect(): void {
    this.eventSource?.close();
    this.eventSource = null;
    if (this.reconnectTimer != null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.$sseStatus.set('disconnected');
    this.reconnectAttempt = 0;
    this.sseConnecting = false;
  }

  private ensureConnected(): void {
    if (this.eventSource != null || this.sseConnecting) return;
    this.sseConnecting = true;

    this.$sseStatus.set('connecting');

    const token = getAuthToken();
    const workspaceId = localStorage.getItem("workspace-id")
      ?? sessionStorage.getItem("workspace-id")
      ?? "default";
    if (!token) {
      this.$sseStatus.set('disconnected');
      this.sseConnecting = false;
      return;
    }

    const url = `${API_BASE}/events/sse?token=${encodeURIComponent(token)}&workspace_id=${encodeURIComponent(workspaceId)}&event_types=device.status_change,device.connection,device.property_change`;

    try {
      this.eventSource = new EventSource(url);
    } catch {
      this.$sseStatus.set('error');
      this.sseConnecting = false;
      this.scheduleReconnect();
      return;
    }

    this.eventSource.onopen = () => {
      this.$sseStatus.set('connected');
      this.reconnectAttempt = 0;
      this.sseConnecting = false;

      // 应用连接期间积压的事件
      const map = this.$devicesMap.get();
      let currentMap = map;
      for (const evt of this.pendingSseEvents) {
        const updated = this.applySseEventToMap(currentMap, evt);
        if (updated) currentMap = updated;
      }
      this.pendingSseEvents = [];
      if (currentMap !== map) {
        this.$devicesMap.set(currentMap);
      }
    };

    this.eventSource.onmessage = async (ev) => {
      try {
        const data = JSON.parse(ev.data);
        await this.handleSseEvent(data);
      } catch {
        // ignore malformed events
      }
    };

    this.eventSource.onerror = () => {
      this.$sseStatus.set('error');
      this.eventSource?.close();
      this.eventSource = null;
      this.sseConnecting = false;

      // SSE 认证失败
      if (this.reconnectAttempt === 0) {
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
      this.sseConnecting = false;
      this.ensureConnected();
    }, delay);
  }

  private async handleSseEvent(data: any): Promise<void> {
    const deviceId: string | undefined = data.device_id;
    const eventType: string = data.event_type ?? '';
    const map = this.$devicesMap.get();

    const updated = this.applySseEventToMap(map, data);
    if (updated) {
      this.$devicesMap.set(updated);
      if (deviceId) {
        window.dispatchEvent(new CustomEvent('device-updated', {
          detail: { deviceId, eventType, data },
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

    let device = map.get(deviceId);

    // 设备不在缓存中，从事件数据构造最小设备
    if (!device) {
      const newDevice: Device = {
        id: deviceId,
        name: data.content?.title?.replace('Property Changed: ', '').split(' - ')[0] ?? deviceId,
        status: 'online',
        properties: [],
      };
      const updated = new Map(map);
      updated.set(deviceId, newDevice);
      map = updated;
      device = newDevice;
    }

    // device.connection / device.status_change
    if (eventType === 'device.connection' || eventType === 'device.status_change') {
      const newStatus = data.status ?? data.content?.status;
      if (newStatus && newStatus !== device.status) {
        const updated = new Map(map);
        updated.set(deviceId, { ...device, status: newStatus });
        return updated;
      }
      return null;
    }

    // device.property_change
    if (eventType === 'device.property_change') {
      const propertyName: string | undefined = data.property_name;
      const newValue: string | undefined = data.new_value;
      if (!propertyName) return null;

      const props = device.properties ?? [];
      const propIndex = props.findIndex((p) => p.name === propertyName);

      let updatedProps: typeof props;
      if (propIndex >= 0) {
        updatedProps = props.map((p, i) =>
          i === propIndex
            ? { ...p, currentValue: newValue, updatedAt: data.timestamp ?? new Date().toISOString() }
            : p,
        );
      } else {
        updatedProps = [
          ...props,
          {
            id: data.property_id ?? `${deviceId}:${propertyName}`,
            deviceId,
            name: propertyName,
            value: newValue,
            currentValue: newValue,
            dataType: 'unknown',
            updatedAt: data.timestamp ?? new Date().toISOString(),
          },
        ];
      }

      const updated = new Map(map);
      updated.set(deviceId, { ...device, properties: updatedProps });
      return updated;
    }

    return null;
  }
}

export const deviceCache = new DeviceCache();
