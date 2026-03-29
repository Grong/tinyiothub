# Marketplace 页面重新设计实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将市场页面改为单栏 Tab 切换布局，使用玻璃拟态风格，API 切换至 marketplace.tinyiothub.com

**Architecture:** 使用 React Tab 组件实现模板/驱动切换，玻璃拟态卡片通过 CSS 类实现，服务层独立配置市场 API 前缀

**Tech Stack:** React, TailwindCSS, React Query, TypeScript

---

## 文件结构

```
web/
├── service/
│   └── marketplace.ts          # 新增独立 API 客户端
├── app/
│   └── marketplace/
│       ├── page.tsx            # 重构为 Tab 布局
│       ├── components/
│       │   ├── marketplace-tabs.tsx       # 新建 Tab 切换器
│       │   ├── marketplace-search.tsx      # 新建 搜索/筛选/排序
│       │   ├── template-grid.tsx          # 新建 模板网格
│       │   ├── driver-grid.tsx            # 新建 驱动网格
│       │   ├── template-card.tsx          # 重构 玻璃拟态模板卡片
│       │   └── driver-card.tsx           # 重构 玻璃拟态驱动卡片
│       └── styles/
│           └── marketplace.css  # 新建 玻璃拟态样式
└── config/
    └── index.ts                # 修改 MARKETPLACE_API_PREFIX
```

---

## Task 1: 创建独立 Marketplace API 客户端

**Files:**
- Create: `web/service/marketplace.ts`
- Modify: `web/config/index.ts` (添加 MARKETPLACE_API_PREFIX 常量)

- [ ] **Step 1: 创建 marketplace API 服务**

```typescript
// web/service/marketplace.ts
import { apiGet, apiPost } from '@/lib/api-client'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'

const MARKETPLACE_API_PREFIX = 'https://marketplace.tinyiothub.com/api/v1'

// 模板类型
export interface TemplateMetadata {
  id: string
  name: string
  version: string
  category: string
  protocol: string
  manufacturer: string
  description: string
  tags: string[]
  author: { name: string; email: string }
  icon?: string
  downloads: number
  rating: number
  reviews: number
  license: string
  fileUrl: string
  checksum: string
  size: number
  createdAt: string
  updatedAt: string
}

// 驱动类型
export interface DriverMetadata {
  id: string
  name: string
  version: string
  protocol: string
  description: string
  tags: string[]
  author: { name: string; email: string }
  icon?: string
  downloads: number
  rating: number
  reviews: number
  license: string
  homepage?: string
  documentation?: string
  platforms: Record<string, { fileUrl: string; checksum: string; size: number }>
  requirements: { minVersion: string }
  createdAt: string
  updatedAt: string
}

// API 函数
const marketplaceApi = {
  getTemplates: () => apiGet<TemplateMetadata[]>(`${MARKETPLACE_API_PREFIX}/v1/templates`),
  getTemplate: (id: string) => apiGet<TemplateMetadata | null>(`${MARKPLACE_API_PREFIX}/v1/templates/${id}`),
  installTemplate: (id: string) => apiPost<string>(`${MARKETPLACE_API_PREFIX}/v1/templates/${id}/install`, {}),
  getDrivers: () => apiGet<DriverMetadata[]>(`${MARKETPLACE_API_PREFIX}/v1/drivers`),
  getDriver: (id: string) => apiGet<DriverMetadata | null>(`${MARKPLACE_API_PREFIX}/v1/drivers/${id}`),
  installDriver: (id: string) => apiPost<string>(`${MARKPLACE_API_PREFIX}/v1/drivers/${id}/install`, {}),
}

// React Query Hooks
export const useMarketplaceTemplates = () =>
  useQuery({
    queryKey: queryKeys.marketplace.templates,
    queryFn: async () => {
      const res = await marketplaceApi.getTemplates()
      return res.result || []
    },
  })

export const useMarketplaceTemplate = (id: string, enabled = true) =>
  useQuery({
    queryKey: queryKeys.marketplace.template(id),
    queryFn: async () => (await marketplaceApi.getTemplate(id)).result,
    enabled: enabled && !!id,
  })

export const useInstallTemplate = () => {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id }: { id: string }) => marketplaceApi.installTemplate(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.templates.all })
      qc.invalidateQueries({ queryKey: queryKeys.marketplace.templates })
    },
  })
}

export const useMarketplaceDrivers = () =>
  useQuery({
    queryKey: queryKeys.marketplace.drivers,
    queryFn: async () => {
      const res = await marketplaceApi.getDrivers()
      return res.result || []
    },
  })

export const useMarketplaceDriver = (id: string, enabled = true) =>
  useQuery({
    queryKey: queryKeys.marketplace.driver(id),
    queryFn: async () => (await marketplaceApi.getDriver(id)).result,
    enabled: enabled && !!id,
  })

export const useInstallDriver = () => {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id }: { id: string }) => marketplaceApi.installDriver(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.drivers.all })
      qc.invalidateQueries({ queryKey: queryKeys.marketplace.drivers })
    },
  })
}
```

- [ ] **Step 2: 提交**

```bash
git add web/service/marketplace.ts
git commit -m "feat(marketplace): add independent marketplace API client"
```

---

## Task 2: 创建玻璃拟态样式

**Files:**
- Create: `web/app/marketplace/styles/marketplace.css`

- [ ] **Step 1: 创建玻璃拟态样式**

```css
/* Glassmorphism styles for marketplace */

/* 玻璃卡片基础 */
.glass-card {
  background: rgba(255, 255, 255, 0.6);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  border: 1px solid rgba(255, 255, 255, 0.4);
  border-radius: 16px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.08), 0 2px 8px rgba(0, 0, 0, 0.04);
  transition: all 0.2s ease;
}

.glass-card:hover {
  background: rgba(255, 255, 255, 0.8);
  transform: translateY(-2px);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.12);
}

/* 暗色模式 */
[data-theme="dark"] .glass-card {
  background: rgba(30, 41, 59, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.1);
}

[data-theme="dark"] .glass-card:hover {
  background: rgba(30, 41, 59, 0.8);
}

/* Tab 按钮 */
.tab-button {
  @apply px-6 py-2.5 rounded-full text-sm font-medium transition-all duration-200;
  background: rgba(255, 255, 255, 0.5);
  border: 1px solid rgba(255, 255, 255, 0.4);
}

.tab-button:hover {
  background: rgba(255, 255, 255, 0.7);
}

.tab-button.active {
  background: linear-gradient(135deg, rgba(59, 130, 246, 0.9), rgba(79, 70, 229, 0.9));
  color: white;
  border-color: transparent;
  box-shadow: 0 4px 16px rgba(59, 130, 246, 0.3);
}

[data-theme="dark"] .tab-button {
  background: rgba(30, 41, 59, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.1);
}

[data-theme="dark"] .tab-button:hover {
  background: rgba(30, 41, 59, 0.8);
}

[data-theme="dark"] .tab-button.active {
  background: linear-gradient(135deg, rgba(59, 130, 246, 0.9), rgba(79, 70, 229, 0.9));
}

/* 搜索框 */
.glass-search {
  background: rgba(255, 255, 255, 0.6);
  backdrop-filter: blur(8px);
  border: 1px solid rgba(255, 255, 255, 0.4);
  @apply rounded-xl px-4 py-2.5 text-sm w-full;
}

.glass-search:focus {
  background: rgba(255, 255, 255, 0.8);
  border-color: rgba(59, 130, 246, 0.5);
  outline: none;
  box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.1);
}

[data-theme="dark"] .glass-search {
  background: rgba(30, 41, 59, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.1);
}

[data-theme="dark"] .glass-search:focus {
  background: rgba(30, 41, 59, 0.8);
}

/* 页面背景 */
.marketplace-bg {
  background: linear-gradient(135deg, #f8fafc 0%, #eff6ff 40%, #eef2ff 100%);
  min-height: 100vh;
}

[data-theme="dark"] .marketplace-bg {
  background: linear-gradient(135deg, #0f172a 0%, #1e293b 50%, #0f172a 100%);
}
```

- [ ] **Step 2: 提交**

```bash
git add web/app/marketplace/styles/marketplace.css
git commit -m "feat(marketplace): add glassmorphism styles"
```

---

## Task 3: 创建 MarketplaceTabs 组件

**Files:**
- Create: `web/app/marketplace/components/marketplace-tabs.tsx`

- [ ] **Step 1: 创建 Tab 组件**

```tsx
'use client'

import React from 'react'

export type TabType = 'templates' | 'drivers'

interface MarketplaceTabsProps {
  activeTab: TabType
  onTabChange: (tab: TabType) => void
}

export default function MarketplaceTabs({ activeTab, onTabChange }: MarketplaceTabsProps) {
  const tabs: { key: TabType; label: string }[] = [
    { key: 'templates', label: '设备模板' },
    { key: 'drivers', label: '驱动程序' },
  ]

  return (
    <div className="flex items-center gap-2 p-1.5 rounded-2xl glass-card w-fit">
      {tabs.map((tab) => (
        <button
          key={tab.key}
          onClick={() => onTabChange(tab.key)}
          className={`tab-button ${activeTab === tab.key ? 'active' : ''}`}
        >
          {tab.label}
        </button>
      ))}
    </div>
  )
}
```

- [ ] **Step 2: 提交**

```bash
git add web/app/marketplace/components/marketplace-tabs.tsx
git commit -m "feat(marketplace): add marketplace tabs component"
```

---

## Task 4: 创建 MarketplaceSearch 组件

**Files:**
- Create: `web/app/marketplace/components/marketplace-search.tsx`

- [ ] **Step 1: 创建搜索组件**

```tsx
'use client'

import React from 'react'
import { MagnifyingGlassIcon } from '@heroicons/react/24/outline'

interface MarketplaceSearchProps {
  searchQuery: string
  onSearchChange: (query: string) => void
  filterOptions: { value: string; label: string }[]
  sortOptions: { value: string; label: string }[]
  activeFilter: string
  activeSort: string
  onFilterChange: (filter: string) => void
  onSortChange: (sort: string) => void
}

export default function MarketplaceSearch({
  searchQuery,
  onSearchChange,
  filterOptions,
  sortOptions,
  activeFilter,
  activeSort,
  onFilterChange,
  onSortChange,
}: MarketplaceSearchProps) {
  return (
    <div className="glass-card p-4 mb-6">
      <div className="flex flex-col lg:flex-row gap-4">
        {/* 搜索框 */}
        <div className="relative flex-1">
          <MagnifyingGlassIcon className="absolute left-3 top-1/2 -translate-y-1/2 h-5 w-5 text-gray-400" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder="搜索模板..."
            className="glass-search pl-10"
          />
        </div>

        {/* 筛选和排序 */}
        <div className="flex gap-3">
          <select
            value={activeFilter}
            onChange={(e) => onFilterChange(e.target.value)}
            className="glass-search w-auto min-w-[120px]"
          >
            {filterOptions.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>

          <select
            value={activeSort}
            onChange={(e) => onSortChange(e.target.value)}
            className="glass-search w-auto min-w-[120px]"
          >
            {sortOptions.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: 提交**

```bash
git add web/app/marketplace/components/marketplace-search.tsx
git commit -m "feat(marketplace): add marketplace search component"
```

---

## Task 5: 重构 TemplateCard 玻璃拟态样式

**Files:**
- Modify: `web/app/components/marketplace/template-marketplace/template-card.tsx`

- [ ] **Step 1: 更新模板卡片样式**

将原有的模板卡片改为玻璃拟态风格，主要变更：
1. 添加 `.glass-card` 类
2. 更新悬浮效果
3. 调整图标和文字颜色以适配玻璃背景

```tsx
// 关键样式变更
<div className="glass-card p-5 hover:shadow-xl transition-all duration-200">
  {/* 图标容器 */}
  <div className="w-12 h-12 rounded-xl bg-gradient-to-br from-blue-500/20 to-indigo-500/20 flex items-center justify-center mb-4">
    {/* icon */}
  </div>
  {/* 文字内容 */}
  <h3 className="text-base font-semibold text-gray-900 mb-2">{name}</h3>
  <p className="text-sm text-gray-600 line-clamp-2">{description}</p>
  {/* 标签 */}
  <div className="flex flex-wrap gap-2 mt-4">
    {tags.map((tag) => (
      <span key={tag} className="glass-badge px-2 py-1 text-xs rounded-full">
        {tag}
      </span>
    ))}
  </div>
</div>
```

- [ ] **Step 2: 提交**

```bash
git add web/app/components/marketplace/template-marketplace/template-card.tsx
git commit -m "refactor(marketplace): apply glassmorphism to template card"
```

---

## Task 6: 重构 DriverCard 玻璃拟态样式

**Files:**
- Modify: `web/app/components/marketplace/driver-marketplace/driver-card.tsx`

- [ ] **Step 1: 更新驱动卡片样式**

同 Task 5，应用相同的玻璃拟态样式到驱动卡片。

- [ ] **Step 2: 提交**

```bash
git add web/app/components/marketplace/driver-marketplace/driver-card.tsx
git commit -m "refactor(marketplace): apply glassmorphism to driver card"
```

---

## Task 7: 创建 TemplateGrid 和 DriverGrid 组件

**Files:**
- Create: `web/app/marketplace/components/template-grid.tsx`
- Create: `web/app/marketplace/components/driver-grid.tsx`

- [ ] **Step 1: 创建 TemplateGrid**

```tsx
'use client'

import React from 'react'
import TemplateCard from '@/app/components/marketplace/template-marketplace/template-card'
import type { TemplateMetadata } from '@/service/marketplace'

interface TemplateGridProps {
  templates: TemplateMetadata[]
  isLoading?: boolean
}

export default function TemplateGrid({ templates, isLoading }: TemplateGridProps) {
  if (isLoading) {
    return (
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
        {[...Array(8)].map((_, i) => (
          <div key={i} className="glass-card h-48 animate-pulse" />
        ))}
      </div>
    )
  }

  if (templates.length === 0) {
    return (
      <div className="glass-card p-12 text-center">
        <p className="text-gray-500">暂无模板</p>
      </div>
    )
  }

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
      {templates.map((template) => (
        <TemplateCard key={template.id} template={template} />
      ))}
    </div>
  )
}
```

- [ ] **Step 2: 创建 DriverGrid**

```tsx
'use client'

import React from 'react'
import DriverCard from '@/app/components/marketplace/driver-marketplace/driver-card'
import type { DriverMetadata } from '@/service/marketplace'

interface DriverGridProps {
  drivers: DriverMetadata[]
  isLoading?: boolean
}

export default function DriverGrid({ drivers, isLoading }: DriverGridProps) {
  if (isLoading) {
    return (
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
        {[...Array(8)].map((_, i) => (
          <div key={i} className="glass-card h-48 animate-pulse" />
        ))}
      </div>
    )
  }

  if (drivers.length === 0) {
    return (
      <div className="glass-card p-12 text-center">
        <p className="text-gray-500">暂无驱动</p>
      </div>
    )
  }

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
      {drivers.map((driver) => (
        <DriverCard key={driver.id} driver={driver} />
      ))}
    </div>
  )
}
```

- [ ] **Step 3: 提交**

```bash
git add web/app/marketplace/components/template-grid.tsx web/app/marketplace/components/driver-grid.tsx
git commit -m "feat(marketplace): add template and driver grid components"
```

---

## Task 8: 重构 MarketplacePage 主页面

**Files:**
- Modify: `web/app/marketplace/page.tsx`

- [ ] **Step 1: 重构页面为 Tab 布局**

```tsx
'use client'

import React, { useState, useEffect } from 'react'
import { ArrowRightIcon } from '@heroicons/react/24/outline'
import MarketplaceTabs, { TabType } from './components/marketplace-tabs'
import MarketplaceSearch from './components/marketplace-search'
import TemplateGrid from './components/template-grid'
import DriverGrid from './components/driver-grid'
import {
  useMarketplaceTemplates,
  useMarketplaceDrivers,
} from '@/service/marketplace'
import './styles/marketplace.css'

export default function MarketplacePage() {
  const [activeTab, setActiveTab] = useState<TabType>('templates')
  const [searchQuery, setSearchQuery] = useState('')
  const [activeFilter, setActiveFilter] = useState('all')
  const [activeSort, setActiveSort] = useState('popular')

  const { data: templates = [], isLoading: templatesLoading } = useMarketplaceTemplates()
  const { data: drivers = [], isLoading: driversLoading } = useMarketplaceDrivers()

  useEffect(() => {
    document.title = 'TinyIoTHub | 智能物联网平台'
  }, [])

  // 筛选和排序逻辑
  const filteredTemplates = templates.filter((t) =>
    t.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    t.description.toLowerCase().includes(searchQuery.toLowerCase())
  )

  const filteredDrivers = drivers.filter((d) =>
    d.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    d.description.toLowerCase().includes(searchQuery.toLowerCase())
  )

  const templateFilters = [
    { value: 'all', label: '全部分类' },
    { value: 'sensor', label: '传感器' },
    { value: 'actuator', label: '执行器' },
  ]

  const driverFilters = [
    { value: 'all', label: '全部协议' },
    { value: 'modbus', label: 'Modbus' },
    { value: 'onvif', label: 'ONVIF' },
    { value: 'snmp', label: 'SNMP' },
    { value: 'mqtt', label: 'MQTT' },
  ]

  const sortOptions = [
    { value: 'popular', label: '最受欢迎' },
    { value: 'recent', label: '最新' },
    { value: 'rating', label: '评分最高' },
  ]

  return (
    <div className="marketplace-bg">
      {/* Navigation */}
      <nav className="sticky top-0 z-50 glass-nav border-b border-white/30">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="flex h-16 items-center justify-between">
            <div className="flex items-center gap-8">
              <a href="/" className="flex items-center gap-2 group">
                <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-blue-600 to-blue-700 text-white shadow-lg shadow-blue-600/30">
                  <ArrowRightIcon className="h-5 w-5" />
                </div>
                <span className="text-xl font-bold text-gray-900">TinyIoTHub</span>
              </a>
              <div className="hidden lg:flex items-center gap-8">
                <a href="/marketplace" className="text-sm font-medium text-blue-600">市场</a>
                <a href="https://docs.tinyiothub.com" className="text-sm font-medium text-gray-600">文档</a>
              </div>
            </div>
            <div className="flex items-center gap-3">
              <a href="/signin" className="text-sm font-medium text-gray-600">登录</a>
              <a href="/signin" className="rounded-lg bg-blue-600 px-5 py-2.5 text-sm font-semibold text-white">免费试用</a>
            </div>
          </div>
        </div>
      </nav>

      {/* Main Content */}
      <div className="px-6 lg:px-8 py-8 max-w-7xl mx-auto">
        {/* Tabs */}
        <div className="flex justify-center mb-8">
          <MarketplaceTabs activeTab={activeTab} onTabChange={setActiveTab} />
        </div>

        {/* Search */}
        <MarketplaceSearch
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          filterOptions={activeTab === 'templates' ? templateFilters : driverFilters}
          sortOptions={sortOptions}
          activeFilter={activeFilter}
          activeSort={activeSort}
          onFilterChange={setActiveFilter}
          onSortChange={setActiveSort}
        />

        {/* Grid */}
        <div className="transition-all duration-300">
          {activeTab === 'templates' ? (
            <TemplateGrid templates={filteredTemplates} isLoading={templatesLoading} />
          ) : (
            <DriverGrid drivers={filteredDrivers} isLoading={driversLoading} />
          )}
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: 提交**

```bash
git add web/app/marketplace/page.tsx
git commit -m "refactor(marketplace): redesign page with tab layout and glassmorphism"
```

---

## Task 9: 更新环境变量配置

**Files:**
- Modify: `web/.env`
- Modify: `web/.env.example`

- [ ] **Step 1: 添加 MARKETPLACE_API_PREFIX 环境变量**

```bash
# .env
NEXT_PUBLIC_MARKETPLACE_API_PREFIX=https://marketplace.tinyiothub.com/api/v1

# .env.example
NEXT_PUBLIC_MARKETPLACE_API_PREFIX=https://marketplace.tinyiothub.com/api/v1
```

- [ ] **Step 2: 提交**

```bash
git add web/.env web/.env.example
git commit -m "feat(marketplace): add marketplace API prefix env var"
```

---

## 总结

| Task | 文件 | 状态 |
|------|------|------|
| 1 | `web/service/marketplace.ts` | ⬜ |
| 2 | `web/app/marketplace/styles/marketplace.css` | ⬜ |
| 3 | `web/app/marketplace/components/marketplace-tabs.tsx` | ⬜ |
| 4 | `web/app/marketplace/components/marketplace-search.tsx` | ⬜ |
| 5 | `web/app/components/marketplace/template-marketplace/template-card.tsx` | ⬜ |
| 6 | `web/app/components/marketplace/driver-marketplace/driver-card.tsx` | ⬜ |
| 7 | `web/app/marketplace/components/template-grid.tsx` | ⬜ |
| 7 | `web/app/marketplace/components/driver-grid.tsx` | ⬜ |
| 8 | `web/app/marketplace/page.tsx` | ⬜ |
| 9 | `web/.env`, `web/.env.example` | ⬜ |
