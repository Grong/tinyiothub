# TinyIoTHub - Frontend

基于 Next.js 14 的现代化物联网边缘网关管理界面，采用 TypeScript + TailwindCSS 构建。

## 特性

- 🚀 **现代化技术栈** - Next.js 14 + TypeScript + TailwindCSS
- 📱 **响应式设计** - 支持桌面和移动设备
- 🎨 **主题系统** - 支持亮色/暗色主题切换
- 🌐 **国际化支持** - 中英文多语言切换
- 🔄 **实时数据** - React Query + Server-Sent Events
- 🎯 **类型安全** - 完整的 TypeScript 类型定义
- 📊 **数据可视化** - 设备状态监控和数据图表
- 🔐 **安全认证** - JWT 令牌管理和权限控制

## 技术栈

- **框架**: Next.js 14 (App Router)
- **语言**: TypeScript 5+
- **样式**: TailwindCSS 3+ + CSS Variables
- **状态管理**: React Query (TanStack Query)
- **HTTP客户端**: ky + 统一API客户端
- **图标**: Remix Icons
- **包管理**: pnpm 8+
- **构建工具**: Turbopack (开发) / Webpack (生产)

## 快速开始

### 环境要求

- **Node.js**: 18+
- **pnpm**: 8+ (推荐包管理器)
- **浏览器**: Chrome 90+, Firefox 88+, Safari 14+, Edge 90+

### 安装和运行

```bash
# 安装依赖
pnpm install

# 开发运行
pnpm dev

# 构建生产版本
pnpm build

# 启动生产服务器
pnpm start

# 类型检查
pnpm type-check

# 代码检查
pnpm lint

# 代码格式化
pnpm format
```

### 环境配置

创建 `.env.local` 文件：

```env
# API 配置
NEXT_PUBLIC_API_URL=http://localhost:3002
NEXT_PUBLIC_API_PREFIX=/api/v1

# 应用配置
NEXT_PUBLIC_APP_NAME=TinyIoTHub
NEXT_PUBLIC_APP_VERSION=1.0.0

# 开发配置
NODE_ENV=development
```

## 项目结构

```
web/
├── app/                          # Next.js App Router
│   ├── (dashboard)/              # 仪表板布局组
│   │   ├── dashboard/            # 仪表板页面
│   │   ├── devices/              # 设备管理页面
│   │   ├── templates/            # 模板管理页面
│   │   ├── alarms/               # 告警管理页面
│   │   ├── users/                # 用户管理页面
│   │   ├── system/               # 系统管理页面
│   │   └── layout.tsx            # 仪表板布局
│   ├── auth/                     # 认证页面
│   │   ├── login/                # 登录页面
│   │   └── layout.tsx            # 认证布局
│   ├── components/               # React 组件
│   │   ├── base/                 # 基础组件
│   │   │   ├── button.tsx        # 按钮组件
│   │   │   ├── input.tsx         # 输入框组件
│   │   │   ├── modal.tsx         # 模态框组件
│   │   │   └── ...               # 其他基础组件
│   │   ├── devices/              # 设备相关组件
│   │   │   ├── device-list.tsx   # 设备列表
│   │   │   ├── device-card.tsx   # 设备卡片
│   │   │   └── create-device-wizard/  # 设备创建向导
│   │   │       ├── index.tsx     # 主向导组件
│   │   │       ├── template-selection-step.tsx
│   │   │       ├── device-info-step.tsx
│   │   │       └── ...           # 其他步骤组件
│   │   ├── templates/            # 模板相关组件
│   │   │   ├── marketplace/      # 模板市场
│   │   │   ├── card/             # 模板卡片
│   │   │   └── ...               # 其他模板组件
│   │   ├── layout/               # 布局组件
│   │   │   ├── header.tsx        # 页面头部
│   │   │   ├── sidebar.tsx       # 侧边栏
│   │   │   └── ...               # 其他布局组件
│   │   └── providers/            # 上下文提供者
│   │       ├── query-provider.tsx    # React Query 提供者
│   │       ├── theme-provider.tsx    # 主题提供者
│   │       └── i18n-client-provider.tsx  # 国际化提供者
│   ├── globals.css               # 全局样式
│   ├── layout.tsx                # 根布局
│   ├── loading.tsx               # 全局加载组件
│   ├── error.tsx                 # 全局错误组件
│   └── not-found.tsx             # 404 页面
├── lib/                          # 工具库
│   ├── api-client.ts             # 统一API客户端
│   ├── case-converter.ts         # 命名格式转换
│   ├── query-keys.ts             # React Query 键管理
│   ├── utils.ts                  # 通用工具函数
│   └── validations.ts            # 表单验证规则
├── service/                      # API服务层
│   ├── auth.ts                   # 认证服务
│   ├── devices.ts                # 设备服务
│   ├── drivers.ts                # 驱动服务
│   ├── templates.ts              # 模板服务
│   ├── alarms.ts                 # 告警服务
│   ├── users.ts                  # 用户服务
│   ├── system.ts                 # 系统服务
│   ├── dashboard.ts              # 仪表板服务
│   ├── fetch.ts                  # HTTP客户端
│   └── common.ts                 # 通用服务
├── types/                        # TypeScript 类型定义
│   ├── api.ts                    # API 响应类型
│   ├── device.ts                 # 设备相关类型
│   ├── template.ts               # 模板相关类型
│   ├── user.ts                   # 用户相关类型
│   └── common.ts                 # 通用类型
├── utils/                        # 工具函数
│   ├── i18n-template.ts          # 模板国际化处理
│   ├── format.ts                 # 格式化工具
│   ├── validation.ts             # 验证工具
│   └── constants.ts              # 常量定义
├── config/                       # 配置文件
│   ├── index.ts                  # 主配置
│   ├── api.ts                    # API 配置
│   └── theme.ts                  # 主题配置
├── public/                       # 静态资源
│   ├── icons/                    # 图标文件
│   ├── images/                   # 图片文件
│   └── favicon.ico               # 网站图标
├── styles/                       # 样式文件
│   ├── globals.css               # 全局样式
│   └── components.css            # 组件样式
├── tailwind.config.js            # TailwindCSS 配置
├── next.config.js                # Next.js 配置
├── tsconfig.json                 # TypeScript 配置
├── package.json                  # 项目配置
└── README.md                     # 项目文档
```

## API 集成规范

### 统一API客户端

所有API调用必须使用统一的API客户端：

```typescript
// ✅ 正确：使用统一API客户端
import { apiGet, apiPost, apiPut, apiDelete } from '@/lib/api-client'

// GET请求
const response = await apiGet<DeviceList>('devices', { page: 1, pageSize: 20 })

// POST请求
const response = await apiPost<Device>('devices', deviceData)

// PUT请求
const response = await apiPut<Device>(`devices/${id}`, updateData)

// DELETE请求
const response = await apiDelete<boolean>(`devices/${id}`)
```

### Service层结构

每个功能模块都有对应的service文件：

```typescript
// service/devices.ts
import { apiGet, apiPost } from '@/lib/api-client'
import { useQuery, useMutation } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'

// 1. 定义类型接口
export interface Device {
  id: string
  name: string
  status: string
  createdAt: string  // 自动从created_at转换
}

// 2. API调用函数
export const deviceApi = {
  getDevices: (params?: DeviceQueryParams) => 
    apiGet<Device[]>('devices', params),
  createDevice: (data: CreateDeviceRequest) => 
    apiPost<Device>('devices', data),
}

// 3. React Query Hooks
export const useDevices = (params?: DeviceQueryParams) => {
  return useQuery({
    queryKey: queryKeys.devices.list(params || {}),
    queryFn: async () => {
      const response = await deviceApi.getDevices(params)
      return response.result || []
    },
  })
}
```

### 组件中的使用

```typescript
// components/devices/device-list.tsx
import { useDevices } from '@/service/devices'

const DeviceList: React.FC = () => {
  const { data: devices, isLoading, error } = useDevices({ page: 1 })
  
  if (isLoading) return <div>加载中...</div>
  if (error) return <div>加载失败: {error.message}</div>
  
  return (
    <div>
      {devices?.map(device => (
        <div key={device.id}>{device.name}</div>
      ))}
    </div>
  )
}
```

## 组件开发规范

### 基础组件

基础组件位于 `app/components/base/` 目录，提供统一的UI元素：

```typescript
// components/base/button.tsx
interface ButtonProps {
  variant?: 'primary' | 'secondary' | 'danger'
  size?: 'sm' | 'md' | 'lg'
  disabled?: boolean
  loading?: boolean
  onClick?: () => void
  children: React.ReactNode
}

const Button: React.FC<ButtonProps> = ({
  variant = 'primary',
  size = 'md',
  disabled = false,
  loading = false,
  onClick,
  children,
}) => {
  // 组件实现
}
```

### 业务组件

业务组件按功能模块组织，使用统一的命名规范：

```typescript
// components/devices/create-device-wizard/index.tsx
interface CreateDeviceWizardProps {
  isOpen: boolean
  onClose: () => void
  onSuccess?: (device: Device) => void
}

const CreateDeviceWizard: React.FC<CreateDeviceWizardProps> = ({
  isOpen,
  onClose,
  onSuccess,
}) => {
  // 向导逻辑
}
```

### 样式规范

使用 TailwindCSS 和 CSS Variables 实现主题系统：

```css
/* globals.css */
:root {
  /* 颜色变量 */
  --color-primary: 59 130 246;
  --color-secondary: 107 114 128;
  --color-success: 34 197 94;
  --color-warning: 245 158 11;
  --color-danger: 239 68 68;
  
  /* 组件变量 */
  --components-panel-bg: 255 255 255;
  --components-panel-border: 229 231 235;
  --text-primary: 17 24 39;
  --text-secondary: 75 85 99;
}

[data-theme="dark"] {
  --components-panel-bg: 31 41 55;
  --components-panel-border: 75 85 99;
  --text-primary: 243 244 246;
  --text-secondary: 209 213 219;
}
```

```typescript
// 在组件中使用
<div className="bg-components-panel-bg border border-components-panel-border text-text-primary">
  内容
</div>
```

## 国际化支持

### 多语言配置

```typescript
// utils/i18n-template.ts
export const useLocalizedText = () => {
  const locale = 'zh' // 从上下文获取

  return (textObj: Record<string, string>, fallback?: string) => {
    if (typeof textObj === 'string') return textObj
    if (!textObj || typeof textObj !== 'object') return fallback || ''
    
    return textObj[locale] || textObj['zh'] || textObj['en'] || fallback || ''
  }
}
```

### 在组件中使用

```typescript
const MyComponent: React.FC = () => {
  const getLocalizedText = useLocalizedText()
  
  const template = {
    displayName: {
      zh: "温度传感器",
      en: "Temperature Sensor"
    }
  }
  
  return (
    <div>
      {getLocalizedText(template.displayName)}
    </div>
  )
}
```

## 状态管理

### React Query 配置

```typescript
// app/components/providers/query-provider.tsx
'use client'

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 1000 * 60 * 5, // 5分钟
      retry: 3,
      refetchOnWindowFocus: false,
    },
    mutations: {
      retry: 1,
    },
  },
})

export const QueryProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  return (
    <QueryClientProvider client={queryClient}>
      {children}
      <ReactQueryDevtools initialIsOpen={false} />
    </QueryClientProvider>
  )
}
```

### Query Keys 管理

```typescript
// lib/query-keys.ts
export const queryKeys = {
  devices: {
    all: ['devices'] as const,
    lists: () => [...queryKeys.devices.all, 'list'] as const,
    list: (filters: Record<string, any>) => [...queryKeys.devices.lists(), { filters }] as const,
    details: () => [...queryKeys.devices.all, 'detail'] as const,
    detail: (id: string) => [...queryKeys.devices.details(), id] as const,
  },
  
  templates: {
    all: ['templates'] as const,
    lists: () => [...queryKeys.templates.all, 'list'] as const,
    list: (filters: Record<string, any>) => [...queryKeys.templates.lists(), { filters }] as const,
  },
}
```

## 开发工具

### 代码质量

```bash
# ESLint 检查
pnpm lint

# 自动修复
pnpm lint:fix

# Prettier 格式化
pnpm format

# TypeScript 类型检查
pnpm type-check
```

### 开发服务器

```bash
# 启动开发服务器
pnpm dev

# 指定端口
pnpm dev -- --port 3001

# 启用 Turbopack
pnpm dev --turbo
```

### 构建和部署

```bash
# 构建生产版本
pnpm build

# 分析构建包大小
pnpm build:analyze

# 启动生产服务器
pnpm start

# 导出静态文件
pnpm export
```

## 性能优化

### 代码分割

```typescript
// 动态导入组件
import dynamic from 'next/dynamic'

const CreateDeviceWizard = dynamic(
  () => import('./create-device-wizard'),
  { 
    loading: () => <div>加载中...</div>,
    ssr: false 
  }
)
```

### 图片优化

```typescript
import Image from 'next/image'

<Image
  src="/images/device.png"
  alt="设备图片"
  width={200}
  height={150}
  priority={false}
  placeholder="blur"
/>
```

### 缓存策略

```typescript
// React Query 缓存配置
export const useDevices = () => {
  return useQuery({
    queryKey: queryKeys.devices.all,
    queryFn: deviceApi.getDevices,
    staleTime: 1000 * 60 * 5, // 5分钟内不重新请求
    cacheTime: 1000 * 60 * 30, // 30分钟后清除缓存
  })
}
```

## 测试

### 单元测试

```bash
# 运行测试
pnpm test

# 监听模式
pnpm test:watch

# 覆盖率报告
pnpm test:coverage
```

### E2E 测试

```bash
# 运行 E2E 测试
pnpm test:e2e

# 交互模式
pnpm test:e2e:ui
```

## 部署

### 环境变量

```env
# 生产环境配置
NODE_ENV=production
NEXT_PUBLIC_API_URL=https://api.example.com
NEXT_PUBLIC_API_PREFIX=/api/v1
```

## 故障排除

### 常见问题

1. **API 调用失败**
   - 检查后端服务是否启动
   - 确认 API 地址配置正确
   - 查看浏览器网络面板

2. **样式不生效**
   - 检查 TailwindCSS 配置
   - 确认 CSS 变量定义正确
   - 清除浏览器缓存

3. **类型错误**
   - 运行 `pnpm type-check`
   - 检查类型定义文件
   - 更新依赖版本

4. **构建失败**
   - 检查 Node.js 版本
   - 清除 `.next` 目录
   - 重新安装依赖

### 调试工具

- **React Developer Tools**: 组件调试
- **React Query Devtools**: 状态管理调试
- **Next.js DevTools**: 性能分析
- **浏览器开发者工具**: 网络和控制台调试

## 贡献指南

1. 遵循代码规范和命名约定
2. 使用统一的API客户端
3. 编写类型安全的代码
4. 添加适当的错误处理
5. 编写必要的测试用例
6. 更新相关文档

## 许可证

MIT License - 详见根目录 [license](../license) 文件