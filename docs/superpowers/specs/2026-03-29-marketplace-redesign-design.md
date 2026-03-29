# Marketplace 页面重新设计

**日期**: 2026-03-29
**状态**: 已确认

## 1. 概述

重新设计市场页面，采用单栏 Tab 切换布局和玻璃拟态风格，API 切换至独立市场服务。

## 2. 设计变更

### 布局变更

| 项目 | 当前 | 变更后 |
|------|------|--------|
| 布局 | 双栏（模板+驱动并排） | 单栏 + Tab 切换 |
| 风格 | 普通卡片 | 玻璃拟态卡片 |
| 动效 | 无 | 简洁轻量（hover 上浮 + 光晕） |

### 页面结构

```
┌─────────────────────────────────────────────────────┐
│  Navigation (保持现有)                               │
├─────────────────────────────────────────────────────┤
│                                                     │
│   ┌─────────────────────────────────────────────┐  │
│   │  [设备模板]  [驱动程序]   ← Tab 切换          │  │
│   └─────────────────────────────────────────────┘  │
│                                                     │
│   ┌─────────────────────────────────────────────┐  │
│   │  🔍 搜索        分类/协议筛选 │ 排序          │  │
│   └─────────────────────────────────────────────┘  │
│                                                     │
│   ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐   │
│   │ Glass  │  │ Glass  │  │ Glass  │  │ Glass  │   │
│   │  卡片  │  │  卡片  │  │  卡片  │  │  卡片  │   │
│   └────────┘  └────────┘  └────────┘  └────────┘   │
│                                                     │
└─────────────────────────────────────────────────────┘
```

## 3. API 变更

### API 前缀

```
https://marketplace.tinyiothub.com/api/v1
```

### 接口清单

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/v1/templates` | 模板列表 |
| GET | `/v1/templates/{id}` | 模板详情 |
| POST | `/v1/templates/{id}/install` | 安装模板 |
| GET | `/v1/drivers` | 驱动列表 |
| GET | `/v1/drivers/{id}` | 驱动详情 |
| POST | `/v1/drivers/{id}/install` | 安装驱动 |

### 响应格式

```json
{
  "code": 0,
  "msg": "",
  "result": [...]
}
```

## 4. 组件重构

### 组件列表

| 组件 | 路径 | 职责 |
|------|------|------|
| `MarketplacePage` | `app/marketplace/page.tsx` | 根组件，Tab 状态管理 |
| `MarketplaceTabs` | 新建 | Tab 切换器 |
| `MarketplaceSearch` | 新建 | 搜索、筛选、排序 |
| `TemplateGrid` | 新建 | 模板卡片网格 |
| `DriverGrid` | 新建 | 驱动卡片网格 |
| `TemplateCard` | 重构 | 玻璃拟态模板卡片 |
| `DriverCard` | 重构 | 玻璃拟态驱动卡片 |

### 服务层

| 文件 | 变更 |
|------|------|
| `service/marketplace.ts` | 新增独立 API 客户端，指向 marketplace.tinyiothub.com |

## 5. 设计规格

### 玻璃拟态样式

```css
/* 玻璃卡片 */
.glass-marketplace {
  background: rgba(255, 255, 255, 0.6);
  backdrop-filter: blur(16px);
  border: 1px solid rgba(255, 255, 255, 0.4);
  border-radius: 16px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.08);
}

/* 悬浮效果 */
.glass-marketplace:hover {
  background: rgba(255, 255, 255, 0.8);
  transform: translateY(-2px);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.12);
}
```

### 背景渐变

```css
background: linear-gradient(135deg, #f8fafc 0%, #eff6ff 40%, #eef2ff 100%);
```

### 暗色模式支持

使用现有 `data-theme="dark"` 适配，半透明深色背景。

## 6. 实现任务

1. 创建 `service/marketplace.ts` 独立 API 客户端
2. 重构 `app/marketplace/page.tsx` 为 Tab 布局
3. 新建 `MarketplaceTabs` 组件
4. 新建 `MarketplaceSearch` 组件
5. 重构 `TemplateCard` 玻璃拟态样式
6. 重构 `DriverCard` 玻璃拟态样式
7. 新建 `TemplateGrid` / `DriverGrid` 组件
8. 更新环境变量配置

## 7. 依赖项

无新增依赖，使用现有：
- TailwindCSS
- shadcn/ui
- React Query
- Heroicons
