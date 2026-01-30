# 多语言实现规范 (Internationalization Guidelines)

## 概述

本文档定义了 IoT Edge 项目的多语言实现标准和最佳实践，确保所有开发者遵循统一的多语言开发规范。

## 目录结构

```
web/
├── i18n/                          # 多语言资源目录
│   ├── en-US/                     # 英文语言包
│   │   ├── common.ts              # 通用翻译
│   │   ├── layout.ts              # 布局相关翻译
│   │   ├── login.ts               # 登录页面翻译
│   │   ├── device.ts              # 设备管理翻译
│   │   ├── monitoring.ts          # 监控相关翻译
│   │   ├── alarm.ts               # 告警相关翻译
│   │   └── settings.ts            # 设置相关翻译
│   └── zh-Hans/                   # 简体中文语言包
│       ├── common.ts              # 通用翻译
│       ├── layout.ts              # 布局相关翻译
│       ├── login.ts               # 登录页面翻译
│       ├── device.ts              # 设备管理翻译
│       ├── monitoring.ts          # 监控相关翻译
│       ├── alarm.ts               # 告警相关翻译
│       └── settings.ts            # 设置相关翻译
├── i18n-config/                   # 多语言配置
│   ├── index.ts                   # 基础配置
│   ├── i18next-config.ts          # i18next 配置
│   ├── check-i18n-sync.js         # 同步检查脚本
│   └── generate-i18n-types.js     # 类型生成脚本
└── types/
    └── i18n.d.ts                  # 多语言类型定义
```

## 核心原则

### 1. 文件组织原则
- **按功能模块分组**: 每个功能模块有独立的翻译文件
- **保持结构一致**: 所有语言包必须有相同的文件结构
- **避免重复**: 通用翻译放在 `common.ts` 中

### 2. 命名规范
- **文件命名**: 使用 kebab-case，如 `device-management.ts`
- **键名规范**: 使用 camelCase，如 `deviceName`, `createDevice`
- **嵌套结构**: 使用对象嵌套组织相关翻译

### 3. 翻译键结构
```typescript
// ✅ 推荐的结构
const translation = {
  // 页面标题
  pageTitle: '设备管理',
  
  // 操作按钮
  actions: {
    create: '创建',
    edit: '编辑',
    delete: '删除',
    save: '保存',
    cancel: '取消',
  },
  
  // 表单字段
  fields: {
    name: '名称',
    type: '类型',
    status: '状态',
    description: '描述',
  },
  
  // 状态值
  status: {
    online: '在线',
    offline: '离线',
    error: '故障',
    maintenance: '维护中',
  },
  
  // 消息提示
  messages: {
    createSuccess: '创建成功',
    createFailed: '创建失败',
    deleteConfirm: '确定要删除 "{{name}}" 吗？',
  },
}
```

## 文件模板

### 通用翻译文件模板 (common.ts)
```typescript
const translation = {
  // 通用操作
  actions: {
    create: 'Create',
    edit: 'Edit',
    delete: 'Delete',
    save: 'Save',
    cancel: 'Cancel',
    confirm: 'Confirm',
    close: 'Close',
    refresh: 'Refresh',
    search: 'Search',
    filter: 'Filter',
    export: 'Export',
    import: 'Import',
  },
  
  // 通用状态
  status: {
    loading: 'Loading...',
    success: 'Success',
    error: 'Error',
    warning: 'Warning',
    info: 'Info',
  },
  
  // 通用消息
  messages: {
    success: 'Operation completed successfully',
    error: 'Operation failed',
    confirm: 'Are you sure?',
    noData: 'No data available',
    networkError: 'Network error, please try again',
  },
  
  // 分页
  pagination: {
    total: 'Total {{count}} items',
    page: 'Page',
    pageSize: 'Items per page',
    previous: 'Previous',
    next: 'Next',
  },
  
  // 时间
  time: {
    now: 'Now',
    today: 'Today',
    yesterday: 'Yesterday',
    thisWeek: 'This Week',
    thisMonth: 'This Month',
  },
}

export default translation
```

### 功能模块翻译文件模板
```typescript
const translation = {
  // 页面信息
  pageTitle: 'Module Name',
  pageDescription: 'Module description',
  
  // 表格列头
  columns: {
    name: 'Name',
    type: 'Type',
    status: 'Status',
    createdAt: 'Created At',
    updatedAt: 'Updated At',
    actions: 'Actions',
  },
  
  // 表单字段
  form: {
    name: 'Name',
    nameRequired: 'Name is required',
    namePlaceholder: 'Enter name',
    description: 'Description',
    descriptionPlaceholder: 'Enter description',
  },
  
  // 模块特定状态
  status: {
    active: 'Active',
    inactive: 'Inactive',
  },
  
  // 模块特定消息
  messages: {
    createSuccess: 'Created successfully',
    updateSuccess: 'Updated successfully',
    deleteSuccess: 'Deleted successfully',
    deleteConfirm: 'Are you sure you want to delete "{{name}}"?',
  },
}

export default translation
```

## 开发规范

### 1. 添加新翻译的流程
1. **确定翻译文件**: 根据功能模块选择合适的翻译文件
2. **添加英文翻译**: 先在 `en-US` 目录下添加英文翻译
3. **添加中文翻译**: 在 `zh-Hans` 目录下添加对应的中文翻译
4. **保持结构一致**: 确保两个语言包的键结构完全一致
5. **运行检查脚本**: 使用 `npm run check-i18n` 检查同步性

### 2. 使用翻译的规范
```typescript
// ✅ 正确使用方式
import { useTranslation } from 'react-i18next'

const MyComponent = () => {
  const { t } = useTranslation('device') // 指定命名空间
  
  return (
    <div>
      <h1>{t('pageTitle')}</h1>
      <button>{t('actions.create')}</button>
      <p>{t('messages.deleteConfirm', { name: deviceName })}</p>
    </div>
  )
}

// ❌ 错误使用方式
const MyComponent = () => {
  return (
    <div>
      <h1>Device Management</h1> {/* 硬编码文本 */}
      <button>Create</button>
    </div>
  )
}
```

### 3. 翻译键命名规范
```typescript
// ✅ 推荐的命名方式
{
  // 使用 camelCase
  deviceName: 'Device Name',
  createDevice: 'Create Device',
  
  // 使用嵌套结构组织相关翻译
  device: {
    name: 'Name',
    type: 'Type',
    status: 'Status',
  },
  
  // 动作使用动词
  actions: {
    create: 'Create',
    edit: 'Edit',
    delete: 'Delete',
  },
  
  // 消息使用描述性名称
  messages: {
    createSuccess: 'Device created successfully',
    deleteConfirm: 'Are you sure you want to delete this device?',
  }
}

// ❌ 避免的命名方式
{
  // 避免使用下划线
  device_name: 'Device Name',
  
  // 避免过深的嵌套
  device: {
    form: {
      fields: {
        basic: {
          name: 'Name'
        }
      }
    }
  },
  
  // 避免无意义的前缀
  txtDeviceName: 'Device Name',
  lblStatus: 'Status',
}
```

## 质量保证

### 1. 自动化检查
- **同步性检查**: 确保所有语言包有相同的键结构
- **类型检查**: 生成 TypeScript 类型定义
- **缺失检查**: 检查是否有未翻译的键

### 2. 检查脚本
```bash
# 检查多语言同步性
npm run check-i18n

# 生成类型定义
npm run generate-i18n-types

# 检查缺失的翻译
npm run check-missing-translations
```

### 3. 代码审查清单
- [ ] 所有硬编码文本都已提取为翻译键
- [ ] 英文和中文翻译都已添加
- [ ] 翻译键命名符合规范
- [ ] 使用了正确的命名空间
- [ ] 参数化翻译正确使用插值语法
- [ ] 通过了自动化检查

## 最佳实践

### 1. 翻译内容指南
- **保持简洁**: 翻译应该简洁明了
- **上下文一致**: 相同概念在不同地方使用相同翻译
- **用户友好**: 使用用户容易理解的术语
- **避免技术术语**: 面向用户的文本避免使用技术术语

### 2. 参数化翻译
```typescript
// ✅ 正确的参数化翻译
{
  welcome: 'Welcome, {{username}}!',
  itemCount: 'Total {{count}} items',
  deleteConfirm: 'Are you sure you want to delete "{{name}}"?',
}

// 使用方式
t('welcome', { username: 'John' })
t('itemCount', { count: 10 })
t('deleteConfirm', { name: deviceName })
```

### 3. 复数形式处理
```typescript
// 英文复数形式
{
  item: 'item',
  item_plural: 'items',
  deviceCount: '{{count}} device',
  deviceCount_plural: '{{count}} devices',
}

// 中文不需要复数形式
{
  item: '项目',
  deviceCount: '{{count}} 个设备',
}
```

## 故障排除

### 常见问题及解决方案

1. **翻译不显示**
   - 检查是否正确导入了翻译文件
   - 确认命名空间是否正确
   - 检查翻译键是否存在

2. **类型错误**
   - 运行 `npm run generate-i18n-types` 重新生成类型
   - 检查翻译键是否在类型定义中

3. **同步性错误**
   - 运行 `npm run check-i18n` 查看具体错误
   - 确保所有语言包有相同的键结构

## 维护指南

### 定期维护任务
1. **每周检查**: 运行同步性检查脚本
2. **每月审查**: 审查翻译质量和一致性
3. **版本发布前**: 完整的多语言测试

### 添加新语言
1. 在 `i18n-config/index.ts` 中添加新语言代码
2. 创建新语言目录和翻译文件
3. 复制现有翻译文件结构
4. 翻译所有文本内容
5. 更新类型定义

---

**重要提醒**: 任何修改多语言相关代码时，都必须确保所有语言包保持同步，并通过自动化检查。