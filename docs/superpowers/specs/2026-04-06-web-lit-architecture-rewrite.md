# web-lit 架构重写设计

## 背景

web-lit 当前架构存在严重的技术债：路由是硬编码 switch-case、状态管理有重复 atoms、页面组件是 500-800 行的巨石、CSS 是 2000+ 行单文件、API 层有绕过逻辑。参考 openclaw/ui 的「单一根组件 + 纯函数视图」模式做彻底重写。

## 核心决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 重构深度 | 彻底重写 | 清除技术债，建立统一规范 |
| 产品定位 | IoT + Agent 双核 | 两个同等重要的功能域 |
| 路由 | Path-based 路由 | 深层链接、可分享 URL |
| 状态 | 单根 @state + AppViewState | 参考 openclaw/ui 验证过的模式 |
| 根组件 | 单根组件 | 简单直接，逻辑拆分到模块 |

## 目标目录结构

```
web-lit/
├── index.html
├── package.json
├── vite.config.ts
├── tsconfig.json
└── src/
    ├── main.ts                    # 入口：import styles + app.ts
    ├── styles.css                 # CSS 总入口：@import 所有 CSS
    ├── styles/
    │   ├── base.css               # 设计令牌、主题、全局 reset
    │   ├── layout.css             # Shell 布局（sidebar + topbar + content）
    │   ├── layout.mobile.css      # 移动端响应式
    │   ├── components.css         # 通用组件样式
    │   └── iot.css                # IoT 专属样式
    ├── ui/
    │   ├── app.ts                 # 根组件 <tinyiothub-app>（~300 行）
    │   ├── app-view-state.ts      # AppViewState 类型定义
    │   ├── app-render.ts          # renderApp(state) 主渲染函数
    │   ├── app-lifecycle.ts       # handleConnected/Disconnected/Updated
    │   ├── app-router.ts          # 声明式路由表 + path 解析
    │   ├── app-defaults.ts        # 默认常量
    │   ├── api-client.ts          # 统一 API 客户端
    │   ├── theme.ts               # 主题管理
    │   ├── icons.ts               # SVG 图标库
    │   ├── types.ts               # 共享领域类型
    │   ├── controllers/           # API 调用层（纯函数）
    │   │   ├── auth.ts
    │   │   ├── devices.ts
    │   │   ├── alarms.ts
    │   │   ├── dashboard.ts
    │   │   ├── workspace.ts
    │   │   ├── agent.ts
    │   │   └── monitoring.ts
    │   ├── views/                 # 纯函数视图
    │   │   ├── home.ts
    │   │   ├── signin.ts
    │   │   ├── register.ts
    │   │   ├── dashboard.ts
    │   │   ├── devices.ts
    │   │   ├── device-detail.ts
    │   │   ├── alarms.ts
    │   │   ├── monitoring.ts
    │   │   ├── agent.ts
    │   │   ├── settings.ts
    │   │   ├── tags.ts
    │   │   ├── templates.ts
    │   │   └── marketplace.ts
    │   └── components/            # 可复用 UI 组件
    │       ├── sidebar.ts
    │       ├── topbar.ts
    │       ├── device-card.ts
    │       ├── device-form.ts
    │       ├── alarm-list.ts
    │       ├── chat-input.ts
    │       ├── chat-thread.ts
    │       └── ...
    ├── i18n/                      # 国际化（保留现有）
    └── lib/
        ├── navigate.ts
        └── local-storage.ts
```

## 核心模式

### 1. 根组件 app.ts

单个 `<tinyiothub-app>` 持有所有应用状态（~30 个 @state 属性）。生命周期逻辑拆分到 `app-lifecycle.ts`，渲染委托给 `renderApp(state)`。

- `createRenderRoot()` 返回 `this`（Light DOM）
- `connectedCallback()` → `handleConnected(this)` + `setupRouter(this)`
- `disconnectedCallback()` → `handleDisconnected(this)` + 取消路由监听
- `render()` → `renderApp(this as AppViewState)`

### 2. AppViewState 类型

将所有 @state 属性映射为一个 TypeScript 接口，按功能域分组：

- 全局状态：connected, currentRoute, routeParams, token, user, themeMode, navCollapsed
- Device 域：devices[], devicesLoading, currentDevice, devicesPage
- Alarm 域：alarms[], alarmsLoading
- Agent 域：chatMessages[], streamingContent, isStreaming, sessionId
- Dashboard 域：dashboardData, dashboardLoading
- ...

### 3. 声明式路由

路由表为 `RouteConfig[]` 数组，每个条目包含：

- `path`: URL 模式（支持 `:param` 动态段，如 `/devices/:id`）
- `component`: Route 联合类型
- `public?: boolean`（是否需要认证）
- `params?: string[]`（动态参数名）

`matchRoute(pathname)` 解析 URL 返回 `{ route, params }`，`setupRouter(app)` 监听 popstate 并更新 `app.currentRoute` 和 `app.routeParams`。

路由表（14 条）：

| Path | Route | Public |
|------|-------|--------|
| `/` | home | yes |
| `/signin` | signin | yes |
| `/register` | register | yes |
| `/dashboard` | dashboard | - |
| `/devices` | devices | - |
| `/devices/:id` | device-detail | - |
| `/alarms` | alarms | - |
| `/monitoring` | monitoring | - |
| `/agent` | agent | - |
| `/settings` | settings | - |
| `/tags` | tags | - |
| `/templates` | templates | - |
| `/marketplace` | marketplace | - |
| `/marketplace/installed` | marketplace-installed | - |

### 4. 纯函数视图

每个视图是一个导出函数 `renderXxx(state: AppViewState)`，返回 Lit `html` 模板。

- 只读取 state，不直接修改（状态修改由 controller 处理）
- 事件处理通过 dispatch 事件或调用 controller 函数
- 无类、无生命周期、无 @state —— 纯渲染逻辑

### 5. Controller（API 调用层）

每个 controller 是纯函数，接受 state 引用并调用 ApiClient：

```ts
export async function loadDevices(state: AppViewState) {
  state.devicesLoading = true
  const res = await ApiClient.get('/devices', { page: state.devicesPage })
  state.devices = res.devices
  state.devicesLoading = false
}
```

Agent controller 统一使用 ApiClient（不再绕过），SSE 流式通过 ApiClient 的专用方法处理。

### 6. CSS 拆分

从现有 3 个文件重组为 5 个：

- `base.css`：保留现有（设计令牌 + 主题，732 行）
- `layout.css`：从现有 layout.css 重构（只保留 shell 布局）
- `layout.mobile.css`：新增，移动端响应式
- `components.css`：从现有 components.css 拆分通用组件
- `iot.css`：新增，IoT 专属样式（设备卡片、告警、监控图表）

styles.css 作为总入口 `@import` 所有文件。

### 7. 主渲染函数 app-render.ts

```ts
renderApp(state)
  ├─ auth guard → renderSignin if not authenticated
  ├─ auth pages → renderRoute (no chrome)
  └─ full layout
      ├─ <app-sidebar>
      ├─ <app-topbar>
      └─ renderRoute(state) → switch on state.currentRoute
```

## 需要清理的问题

1. 删除 `@nanostores/react`、`@lit-labs/router`、`ky` 依赖
2. 删除 `views/base-page.ts`（死代码）
3. 删除 `stores/` 整个目录（替换为 @state）
4. 删除 `pages/` 整个目录（替换为 views/）
5. 合并重复的 `$sidebarCollapsed` / `$navCollapsed`
6. 统一 agent.ts 不再绕过 ApiClient
7. 删除 services/ 和 types/ 中的重复类型定义
8. 删除 `@nanostores/lit` 依赖

## 保留的现有代码

- `styles/base.css`：设计令牌和主题系统完好，直接保留
- `i18n/`：国际化基础设施完好，直接保留
- `lib/navigate.ts`：保留并增强（加 path 参数支持）
- `lib/local-storage.ts`：直接保留
- 部分 components：device-card、chat-input 等可复用组件迁移到新结构
