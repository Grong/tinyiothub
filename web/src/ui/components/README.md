# 图标系统使用指南

## 快速开始

### 1. 导入图标组件

```typescript
import "./components/icon.js";
import { icon } from "./components/icons.js";
```

### 2. 在模板中使用

```typescript
import { html } from "lit";

// 方式 1: 使用辅助函数（推荐）
render() {
  return html`
    <button class="btn">
      ${icon("Lock")} 锁定
    </button>
  `;
}

// 方式 2: 直接使用组件
render() {
  return html`
    <button class="btn">
      <app-icon name="Lock" size="16"></app-icon> 锁定
    </button>
  `;
}
```

### 3. 自定义样式

```typescript
// 自定义大小
${icon("Lock", { size: 20 })}

// 自定义颜色
${icon("Lock", { color: "var(--accent)" })}

// 指定分类
${icon("Github", { category: "common" })}

// 组合使用
${icon("Lock", { 
  category: "common",
  size: 24,
  color: "#ff5c5c"
})}
```

## 图标分类

图标按功能分类存储在 `ui/src/ui/components/icons/src/public/` 目录下：

- `common/` - 通用图标（Lock, Github, Highlight 等）
- `llm/` - LLM 相关图标
- `billing/` - 计费相关图标
- `model/` - 模型相关图标
- `files/` - 文件相关图标
- `avatar/` - 头像相关图标
- 等等...

## 可用图标列表

### common 分类
- Lock
- Github
- Highlight
- MessageChatSquare
- Dify
- Gdpr
- Iso
- Line3
- 等等...

## 添加新图标

1. 将图标的 JSON 文件放到对应分类目录
2. 确保 JSON 格式符合规范（参考现有图标）
3. 直接使用，无需额外配置

## 性能优化

- 图标会被自动缓存，同一图标只加载一次
- 使用动态 import，按需加载
- SVG 内联渲染，无额外 HTTP 请求

## 注意事项

- 图标默认继承父元素的 `color` 属性
- 默认大小为 16x16，可通过 `size` 属性调整
- 图标会自动垂直居中对齐
