# Device Cache — 前端浏览器侧设备数据缓存层

## Context

当前 web-lit 前端每个组件各自创建 SSE 连接、各自维护本地状态，导致：
1. 多个页面/组件同时订阅 SSE 时产生重复 EventSource 连接
2. 页面切换后数据丢失，需重新 fetch
3. SSE 推送的属性变更只能更新当前活跃组件，其他组件看不到

需要一个共享缓存层：SSE 推送维护最新数据，页面从缓存读取，避免重复订阅和重复请求。

## 技术选型

- **存储技术**: `@lit-labs/signals` (TC39 Signals polyfill，已安装 `^0.2.0`，当前未使用)
- **缓存粒度**: Device + Properties（不含属性历史、不含命令详情）
- **连接管理**: 缓存层持有唯一 EventSource 实例

## 设计

### 新文件: `web/src/stores/device-cache.ts`

```
DeviceCache (singleton)
├── Signals
│   ├── $devicesMap: Signal.State<Map<string, Device>>
│   ├── $profilesMap: Signal.State<Map<string, DeviceProfile>>
│   └── $sseStatus: Signal.State<'disconnected' | 'connecting' | 'connected' | 'error'>
├── Computed
│   └── $devicesList: Computed<Device[]>  ← 从 $devicesMap 派生
├── Private
│   ├── eventSource: EventSource | null
│   ├── reconnectTimer: number | null
│   ├── reconnectAttempt: number
│   ├── fetchInProgress: boolean          ← 防止 fetch/SSE 竞态
│   └── pendingSseEvents: Array           ← fetch 期间缓存 SSE 事件
└── Public Methods
    ├── getDevices(): Promise<Device[]>       ← 首次调用 fetch，后续从信号读缓存
    ├── getProfile(deviceId): Promise<DeviceProfile>  ← 按需 fetch，缓存结果
    ├── refreshDevices(): Promise<void>       ← 强制刷新列表
    ├── updateProperty(deviceId, propId, value): void  ← 本地更新 + API 调用
    └── clearCache(): void                    ← 登出时清空
├── Private (auto-managed)
    ├── ensureConnected(): void               ← 首次 getDevices() 自动调用，读取 token 建立 SSE
    └── getToken(): string | null             ← 从 sessionStorage/localStorage 读取 'auth-token'
```

**SSE 自动连接**: `getDevices()` 首次调用时自动触发 `ensureConnected()`，
从 `sessionStorage.getItem('auth-token') || localStorage.getItem('auth-token')` 读取 token。
无需消费方手动调用 connectSSE。

**Fetch/SSE 竞态处理**: `fetchInProgress` 标记 fetch 进行中，
期间 SSE 事件缓存到 `pendingSseEvents`，fetch 完成后合并到结果中再写入信号。

### SSE 事件处理

```
EventSource.onmessage
  ├─ fetchInProgress == true → push event to pendingSseEvents
  └─ fetchInProgress == false → 正常处理:
      ├─ device.status_change / device.connection
      │   └─ 更新 $devicesMap 中对应 device 的 status 字段
      └─ device.property_change
          └─ 更新 $devicesMap 中对应 device 的 properties[i].currentValue
             同步更新 $profilesMap 中对应 profile 的 properties[i].currentValue
```

**Fetch/SSE 竞态处理**:
1. `getDevices()` 开始 fetch 前设 `fetchInProgress = true`
2. fetch 返回后，将结果写入 `$devicesMap`
3. 遍历 `pendingSseEvents`，对每个事件按上述逻辑更新信号
4. 清空 `pendingSseEvents`，设 `fetchInProgress = false`
5. 后续 SSE 事件正常处理

重连策略：指数退避，1s → 2s → 4s → 8s → max 30s，成功后重置。

### 组件消费方式

```ts
import { watch } from '@lit-labs/signals';
import { deviceCache } from '../../stores/device-cache.js';

@customElement('device-list')
class DeviceList extends LitElement {
  // 首次调用 getDevices() 自动 fetch + 建立 SSE 连接
  // 后续调用直接从信号读缓存
  async connectedCallback() {
    await deviceCache.getDevices();
  }

  // watch() 让组件在信号变化时自动 re-render
  render() {
    const devices = watch(deviceCache.$devicesList);
    return html`${devices.map(d => html`<div>${d.name}: ${d.status}</div>`)}`;
  }
}
```

### 修改文件

| 文件 | 改动 |
|------|------|
| `web/src/stores/device-cache.ts` | **新建** — DeviceCache 单例，自动管理 SSE 连接 |
| `web/src/ui/views/devices.ts` | 移除 `connectSSE()`/`disconnectSSE()`/`handleDeviceStatusEvent()`/`handlePropertyChangeEvent()`；改为从 `deviceCache` 读取数据；`connectedCallback` 调用 `deviceCache.getDevices()`（自动触发 SSE 连接） |

### 不做的事

- 不缓存属性历史（按需 fetch，用完即弃）
- 不缓存命令列表（仅在设备详情页加载）
- 不做 localStorage 持久化（纯内存缓存，刷新页面重新加载）
- 不修改其他页面（本次只迁移 devices.ts，其他页面后续按需迁移）

## 验证

1. `pnpm build` 编译通过
2. 打开设备列表页 → 控制台无重复 SSE 连接日志
3. 在 A 页面修改设备属性值 → 切到 B 页面再切回 → 值已更新
4. 断网 5s 后恢复 → SSE 自动重连
5. 登出 → `clearCache()` 清空数据，无残留
