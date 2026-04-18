# 前端分层规范（Lit 3 + Vite）

> ⚠️ **核心原则**：`web/` 前端基于 Lit 3 + Vite + Web Components，所有代码必须遵循以下规范。

---

## 项目结构

```
web/src/
├── api/           # API 客户端封装
├── ui/            # Lit 组件
│   ├── components/  # 通用组件
│   ├── views/       # 页面视图
│   ├── controllers/ # 状态控制器
│   └── chat/        # AI 聊天 / A2UI
├── i18n/          # 国际化
├── styles/        # CSS 样式
└── stores/        # nanostore 状态管理
```

---

## 场景 A：简单读取（直接用 api-client）

**特征**：列表页、详情页、基本下拉框  
**复杂度**：低  
**分层**：

```
组件 → api/client.ts（直接）
```

**例子**：

```typescript
// ✅ 允许：简单列表页直接用 api-client
// web/src/ui/views/device-list.ts
import { apiGet } from '../../api/client';

async firstUpdated() {
  const response = await apiGet('/devices');
  this.devices = response.result || [];
}
```

**何时用**：数据不需要复用、业务逻辑简单、不需要跨组件共享

---

## 场景 B：需要复用或业务逻辑（完整分层）

**特征**：多处使用、有数据转换、需要缓存或状态管理  
**复杂度**：中-高  
**分层**：

```
组件 → stores → api/client.ts
```

**例子**：

```typescript
// ✅ 必须：跨组件复用或复杂逻辑
// web/src/stores/devices.ts
import { atom, computed } from 'nanostores';
import { apiGet, apiPost } from '../api/client';

export const $devices = atom<Device[]>([]);

export async function loadDevices(params?: DeviceQuery) {
  const response = await apiGet<Device[]>('devices', params);
  $devices.set(response.result || []);
}

export async function updateDeviceStatus(id: string, status: string) {
  await apiPost(`devices/${id}/status`, { status });
  await loadDevices();
}

// web/src/ui/views/device-list.ts
import { $devices, loadDevices } from '../../stores/devices';

@customElement('device-list')
export class DeviceList extends LitElement {
  @state() private devices: Device[] = [];
  private unsubscribe?: () => void;

  connectedCallback() {
    super.connectedCallback();
    this.unsubscribe = $devices.subscribe(devices => {
      this.devices = devices;
    });
    loadDevices();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.unsubscribe?.();
  }
}
```

---

## 场景 C：设备指令下发（完整分层 + 错误处理）

**特征**：异步操作、需要状态追踪、有超时和重试  
**复杂度**：高  

```typescript
// web/src/stores/device-commands.ts
export const $commandStatus = atom<Record<string, CommandStatus>>({});

export async function sendCommand(deviceId: string, command: string, params?: object) {
  $commandStatus.set({ ...$commandStatus.get(), [deviceId]: { state: 'pending' } });
  try {
    const response = await apiPost(`devices/${deviceId}/commands`, { command, params });
    $commandStatus.set({ ...$commandStatus.get(), [deviceId]: { state: 'success', result: response.result } });
    return response;
  } catch (error) {
    $commandStatus.set({ ...$commandStatus.get(), [deviceId]: { state: 'error', error: String(error) } });
    throw error;
  }
}
```

---

## 判断标准

| 问题 | 你需要 |
|-----|-------|
| 这个 API 调用只在这里用？ | 场景 A |
| 这个 API 调用在 2+ 个地方用？ | 场景 B |
| 这个调用需要状态管理/缓存/重试？ | 场景 B |
| 这个调用是设备指令/异步任务？ | 场景 C |
| 这个调用有多种状态（pending/success/error）？ | 场景 C |

---

## 绝对禁止

❌ 在组件里直接写：

```typescript
// ❌ 禁止：任何时候都不准这样
fetch('/api/v1/devices')
```

❌ 不保存 `subscribe()` 返回的 unsubscribe：

```typescript
// ❌ 禁止
connectedCallback() {
  $store.subscribe(v => this.value = v); // 没有保存 unsubscribe
}
```

✅ 正确：

```typescript
// ✅ 正确：保存 unsubscribe 并清理
private unsubscribe?: () => void;

connectedCallback() {
  super.connectedCallback();
  this.unsubscribe = $store.subscribe(v => this.value = v);
}

disconnectedCallback() {
  super.disconnectedCallback();
  this.unsubscribe?.();
}
```

---

## Lit 生命周期规范

- **首次数据加载**用 `firstUpdated()`，不用 `connectedCallback()`（shadow DOM 尚未就绪）
- `updated()` 必须是同步的，Lit 不 await 它
- `disconnectedCallback()` 中必须清理 interval、subscription、event listener
- **禁止** `addEventListener('x', this.handler.bind(this))`，使用箭头函数属性

---

## 快速对照表

| 场景 | 组件直接 apiGet | stores | 何时够用 |
|-----|----------------|--------|---------|
| 简单列表 | ✅ | 可选 | 单一页面、无复用、无复杂逻辑 |
| 需要复用 | ❌ | ✅ | 2+ 地方使用 |
| 异步/有状态 | ❌ | ✅ | 状态管理、轮询、取消 |
| 设备指令 | ❌ | ✅ | 统一错误处理和重试 |

---

**总结**：分层是手段，不是目的。选择能 handle 你当前复杂度的分层，不要 over-engineer。
