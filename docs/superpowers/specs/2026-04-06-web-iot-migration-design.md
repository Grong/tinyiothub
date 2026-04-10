# Web IoT Migration Design

> 将 web-old (React/Next.js) 的 TinyIoTHub IoT 平台业务迁移到 web (Lit/Vite)，替换现有的 OpenHub SaaS 功能。

## Context

- **web-old/**: 完整的 IoT 设备管理平台（React 19 + Next.js 15），包含 18 个业务模块
- **web/**: OpenHub SaaS 平台（Lit 3 + Vite 7），包含 LLM channels、billing 等
- **目标**: 用 IoT 功能完全替换 web/ 中的 SaaS 功能，UI 用 Lit Web Components 重写
- **API 前缀**: `/api/v1/`（IoT 后端格式，`{ code: 0, msg: "", result: T }`）

## Decisions

1. **Clean Slate**: 清空 web/src 中的 SaaS 代码，保留 Lit 基础设施（base, toast, theme, i18n），从 web-old 参考重写
2. **Lit + Light DOM**: 所有组件使用 light DOM（`createRenderRoot() { return this; }`），复用全局样式
3. **API Client 改造**: 适配 `{ code, msg, result }` 响应格式，支持 snake_case/camelCase 转换
4. **无外部状态库**: 用 Lit `@state()` + localStorage，不需要 React Query 等价物
5. **分批实现**: 基础设施 → 登录 → 仪表盘 → 设备管理 → 告警 → 监控 → 其他模块

## Architecture

### Directory Structure

```
web/src/
├── main.ts                     # 入口（更新 import）
├── styles.css                  # 根样式表
├── base.css / layout.css / components.css / ...
├── api/                        # API 层（全部重写）
│   ├── client.ts               # 统一请求，/api/v1 前缀，{code,msg,result} 格式
│   ├── config.ts               # API_BASE = '/api/v1'
│   ├── case-converter.ts       # snake_case ↔ camelCase 转换
│   ├── auth.ts                 # 登录/登出/用户信息
│   ├── devices.ts              # 设备 CRUD + Profile + 指令
│   ├── alarms.ts               # 告警实例 + 规则 + 批量操作
│   ├── dashboard.ts            # 仪表盘统计数据
│   ├── monitoring.ts           # 设备监控 + 性能 + 追踪
│   ├── templates.ts            # 设备模板 CRUD
│   ├── drivers.ts              # 驱动管理
│   ├── events.ts               # 事件系统
│   ├── tags.ts                 # 标签管理
│   ├── users.ts                # 用户管理
│   ├── system.ts               # 系统管理
│   └── marketplace.ts          # 市场
├── types/                      # 类型定义（从 web-old 适配）
│   ├── index.ts                # 重导出
│   ├── device.ts
│   ├── alarm.ts
│   ├── dashboard.ts
│   ├── user.ts
│   ├── tag.ts
│   ├── system.ts
│   └── template.ts
├── i18n/                       # 保留现有 i18n 基础设施
├── ui/
│   ├── app.ts                  # 重写 — IoT 导航、路由、布局
│   ├── components/
│   │   ├── base.ts             # 保留 — BaseComponent
│   │   ├── toast.ts            # 保留
│   │   ├── theme-toggle.ts     # 保留
│   │   ├── skeleton.ts         # 保留
│   │   ├── modal.ts            # 新增 — 弹窗
│   │   ├── data-table.ts       # 新增 — 可排序/筛选/分页表格
│   │   ├── stat-card.ts        # 新增 — 统计卡片
│   │   ├── status-badge.ts     # 新增 — 状态徽章
│   │   ├── empty-state.ts      # 新增 — 空状态占位
│   │   ├── confirm-dialog.ts   # 新增 — 确认对话框
│   │   ├── pagination.ts       # 新增 — 分页组件
│   │   └── search-input.ts     # 新增 — 搜索输入框
│   └── views/
│       ├── login.ts            # 登录（用户名密码）
│       ├── dashboard.ts        # IoT 仪表盘
│       ├── devices.ts          # 设备列表
│       ├── device-detail.ts    # 设备详情（属性/指令/事件）
│       ├── alarms.ts           # 告警管理
│       ├── alarm-rules.ts      # 告警规则
│       ├── monitoring.ts       # 系统监控
│       ├── templates.ts        # 设备模板
│       ├── drivers.ts          # 驱动管理
│       ├── events.ts           # 事件系统
│       ├── tags.ts             # 标签管理
│       ├── users.ts            # 用户管理
│       ├── settings.ts         # 系统设置
│       └── marketplace.ts      # 市场
```

### API Client

**响应格式变更**: 从 `{ success: boolean, data: T }` 改为 `{ code: number, msg: string, result: T }`

```typescript
// api/client.ts — 核心改造
interface ApiResponse<T> {
  code: number    // 0 = 成功，非 0 = 失败
  msg: string
  result: T | null
}

async function apiRequest<T>(endpoint: string, options?: RequestInit): Promise<T> {
  const url = `${API_BASE}${endpoint}`
  // Bearer token from localStorage
  // snake_case 请求 → camelCase 响应
  // code !== 0 抛出 ApiError
  // 401 清除 token 跳转登录
}
```

### Navigation (app.ts)

```
主菜单:
  概览        → /dashboard
  设备管理    → /devices
  告警中心    → /alarms
  事件日志    → /events
  系统监控    → /monitoring

配置:
  设备模板    → /templates
  驱动管理    → /drivers
  标签管理    → /tags

管理 (admin):
  用户管理    → /users
  系统设置    → /settings
  市场       → /marketplace
```

### View Architecture (每个视图)

```typescript
// 每个 view 的标准结构
@customElement('view-devices')
export class DevicesView extends BaseComponent {
  @state() devices: Device[] = []
  @state() loading = true
  @state() error: string | null = null
  @state() searchQuery = ''
  @state() currentPage = 1
  @state() totalPages = 1

  async connectedCallback() {
    super.connectedCallback()
    await this.loadDevices()
  }

  async loadDevices() {
    this.loading = true
    try {
      const response = await deviceApi.getDevices({
        page: this.currentPage,
        pageSize: 20,
        name: this.searchQuery || undefined,
      })
      if (response.code === 0 && response.result) {
        this.devices = response.result.data
        this.totalPages = response.result.pagination.totalPages
      }
    } catch (e) {
      this.error = e instanceof Error ? e.message : '加载失败'
    } finally {
      this.loading = false
    }
  }

  render() { ... }
}
```

## Implementation Phases

### Phase 1: Infrastructure (基础层)
- `api/config.ts` — API_BASE = '/api/v1'
- `api/case-converter.ts` — snake_case ↔ camelCase
- `api/client.ts` — 统一请求，{code, msg, result} 格式
- `types/` — 所有类型定义
- 保留: base.ts, toast.ts, theme-toggle.ts, skeleton.ts, i18n/

### Phase 2: Auth (认证)
- `api/auth.ts` — 登录/登出/获取用户信息
- `ui/views/login.ts` — 登录页面
- `ui/app.ts` — 路由守卫、用户状态

### Phase 3: Dashboard (仪表盘)
- `api/dashboard.ts` — 统计数据 API
- `ui/views/dashboard.ts` — 设备统计、状态分布、系统指标
- `ui/components/stat-card.ts` — 统计卡片

### Phase 4: Devices (设备管理)
- `api/devices.ts` — 设备 CRUD + Profile + 指令
- `ui/views/devices.ts` — 设备列表（搜索、筛选、分页）
- `ui/views/device-detail.ts` — 设备详情（属性、指令、事件）
- `ui/components/data-table.ts` — 通用表格
- `ui/components/status-badge.ts` — 状态徽章

### Phase 5: Alarms (告警)
- `api/alarms.ts` — 告警实例 + 规则 + 批量操作
- `ui/views/alarms.ts` — 告警列表（确认、解决）
- `ui/views/alarm-rules.ts` — 告警规则管理

### Phase 6: Monitoring + Events (监控+事件)
- `api/monitoring.ts` — 设备监控 + 性能 + 追踪
- `api/events.ts` — 事件系统
- `ui/views/monitoring.ts` — 系统监控
- `ui/views/events.ts` — 事件日志

### Phase 7: Remaining Modules (其余模块)
- `api/templates.ts` + `ui/views/templates.ts`
- `api/drivers.ts` + `ui/views/drivers.ts`
- `api/tags.ts` + `ui/views/tags.ts`
- `api/users.ts` + `ui/views/users.ts`
- `api/system.ts` + `ui/views/settings.ts`
- `api/marketplace.ts` + `ui/views/marketplace.ts`

## Cleanup

迁移完成后删除：
- `web/src/api/channels.ts` — SaaS 供应商密钥
- `web/src/api/channel_groups.ts` — SaaS 渠道组
- `web/src/api/compute-pool.ts` — SaaS 算力池
- `web/src/api/llm.ts` — SaaS LLM 模型
- `web/src/api/messages.ts` — SaaS 消息
- `web/src/api/publish.ts` — SaaS 发布
- `web/src/api/user.ts` — 替换为 auth.ts
- 所有 SaaS views（home.ts 保留但改造为 IoT 登录后的 dashboard）
- `web/src/ui/components/welcome-modal.ts` — SaaS 欢迎弹窗

## Spec Self-Review

- [x] No placeholders or TBD sections
- [x] Architecture matches feature descriptions
- [x] Focused scope — single migration project
- [x] No ambiguous requirements — Lit rewrite, /api/v1 prefix, clean slate all confirmed
