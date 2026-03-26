# 前端分层规范（按复杂度分层）

> ⚠️ **核心原则**：分层是为了解决复杂度，不是为了制造复杂度。选择适合当前功能复杂度的分层。

---

## 场景 A：简单读取（推荐简化）

**特征**：列表页、详情页、基本下拉框  
**复杂度**：低  
**分层**：

```
组件 → api-client（直接）
```

**例子**：
```typescript
// ✅ 允许：简单列表页直接用 api-client
// web/app/devices/page.tsx
const { data } = useQuery({
  queryKey: ['devices'],
  queryFn: () => apiGet('/api/v1/devices'),
});
```

**何时用**：数据不需要复用、业务逻辑简单、不需要跨组件共享

---

## 场景 B：需要复用或业务逻辑（必须完整分层）

**特征**：多处使用、有数据转换、有缓存策略、需要乐观更新  
**复杂度**：中-高  
**分层**：

```
组件 → hooks → service → api-client
```

**例子**：
```typescript
// ✅ 必须：跨组件复用或复杂逻辑
// web/service/devices.ts
export const deviceService = {
  getList: (params) => apiGet('/api/v1/devices', params),
  create: (data) => apiPost('/api/v1/devices', data),
  updateStatus: (id, status) => apiPut(`/api/v1/devices/${id}/status`, { status }),
};

// web/hooks/use-devices.ts
export const useDevices = (params) => useQuery({
  queryKey: queryKeys.devices.list(params),
  queryFn: () => deviceService.getList(params),
});
export const useDeviceMutations = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: deviceService.create,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.devices._ }),
    // ... 乐观更新等复杂逻辑
  });
};

// web/app/devices/page.tsx
const { data } = useDevices(params);
const { mutate } = useDeviceMutations();
```

---

## 场景 C：设备指令下发（必须完整分层 + 错误处理）

**特征**：异步操作、需要状态追踪、有超时和重试  
**复杂度**：高  
**额外要求**：

```typescript
// web/service/device-commands.ts
export const deviceCommandService = {
  send: async (deviceId, command) => {
    // 统一错误处理
    // 统一超时设置
    // 统一重试逻辑
    return apiPost(`/api/v1/devices/${deviceId}/commands`, command);
  },
};
```

---

## 判断标准

| 问题 | 你需要 |
|-----|-------|
| 这个 API 调用只在这里用？ | 场景 A |
| 这个 API 调用在 2+ 个地方用？ | 场景 B |
| 这个调用需要乐观更新/缓存失效/重试？ | 场景 B |
| 这个调用是设备指令/异步任务？ | 场景 C |
| 这个调用有多种状态（pending/success/error）？ | 场景 C |

---

## 绝对禁止

❌ 无论什么场景，都不准在组件里直接写：
```typescript
// ❌ 禁止：任何时候都不准这样
fetch('/api/v1/devices')
axios.get('/api/v1/devices')
useQuery({ queryKey: ['devices'], queryFn: () => apiGet('/api/v1/devices') }) // 在组件里直接用 useQuery
```

❌ 在 hooks 里直接写 API 调用逻辑（应该委托给 service）：
```typescript
// ❌ 禁止
export const useDevices = () => {
  return useQuery({ queryFn: () => apiGet('/api/v1/devices') }); // 错误：逻辑在 hook 里
};
```

✅ 正确：
```typescript
// ✅ 正确：逻辑在 service 层
export const useDevices = () => useQuery({
  queryKey: queryKeys.devices.list(),
  queryFn: () => deviceService.getList(), // service 层处理逻辑
});
```

---

## 快速对照表

| 场景 | 组件直接 apiGet | service | hooks | 何时够用 |
|-----|----------------|---------|-------|---------|
| 简单列表 | ✅ | 可选 | 可选 | 单一页面、无复用、无复杂逻辑 |
| 需要复用 | ❌ | ✅ | ✅ | 2+ 地方使用 |
| 异步/有状态 | ❌ | ✅ | ✅ | 乐观更新、轮询、取消 |
| 设备指令 | ❌ | ✅ | ✅ | 统一错误处理和重试 |

---

**总结**：分层是手段，不是目的。选择能 handle 你当前复杂度的分层，不要over-engineer。
