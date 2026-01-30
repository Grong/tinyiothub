# 前后端命名规范统一解决方案

## 问题背景

在前后端分离的项目中，经常遇到命名规范不一致的问题：
- **后端**：使用 `snake_case` 命名（Rust/Python 等语言的惯例）
- **前端**：使用 `camelCase` 命名（JavaScript/TypeScript 的惯例）

这导致了以下问题：
1. API 响应字段名与前端类型定义不匹配
2. 需要手动转换字段名，容易出错
3. 类型定义重复，维护困难
4. 代码可读性差，增加开发成本

## 解决方案

### 1. 自动转换层（推荐方案）

在 API 客户端层实现自动转换，对开发者透明：

```typescript
// lib/case-converter.ts
export function keysToCamelCase<T>(obj: any): T {
  // 递归转换对象键名为 camelCase
}

export function keysToSnakeCase<T>(obj: any): T {
  // 递归转换对象键名为 snake_case
}
```

```typescript
// lib/api-client.ts
export class ApiClient {
  static async post<T>(endpoint: string, data?: any) {
    // 请求时：camelCase → snake_case
    const snakeCaseData = data ? keysToSnakeCase(data) : undefined
    
    const response = await fetcher(endpoint, { 
      method: 'POST', 
      body: snakeCaseData 
    })
    
    // 响应时：snake_case → camelCase
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result
    }
  }
}
```

### 2. 统一类型定义

创建集中的类型定义，避免重复：

```
web/types/
├── index.ts          # 通用类型和重新导出
├── user.ts           # 用户相关类型
├── device.ts         # 设备相关类型
├── tag.ts            # 标签相关类型
├── alarm.ts          # 告警相关类型
└── system.ts         # 系统相关类型
```

### 3. TypeScript 类型转换

使用 TypeScript 的类型系统确保类型安全：

```typescript
// 自动推导转换后的类型
export type KeysToCamelCase<T> = T extends Array<infer U>
  ? Array<KeysToCamelCase<U>>
  : T extends object
  ? {
      [K in keyof T as CamelCase<string & K>]: KeysToCamelCase<T[K]>
    }
  : T
```

## 实施步骤

### 第一步：创建转换工具

1. 实现 `case-converter.ts` 工具函数
2. 更新 `api-client.ts` 集成自动转换
3. 添加 TypeScript 类型支持

### 第二步：统一类型定义

1. 创建 `types/` 目录结构
2. 定义所有实体类型（使用 camelCase）
3. 更新 `types/index.ts` 重新导出

### 第三步：更新服务层

1. 移除重复的类型定义
2. 导入统一的类型
3. 验证 API 调用正常工作

### 第四步：更新组件

1. 使用统一的类型定义
2. 移除手动字段名转换
3. 测试功能正常

## 最佳实践

### 1. 命名规范

- **后端**：严格使用 `snake_case`
- **前端**：严格使用 `camelCase`
- **转换**：在 API 边界自动处理

### 2. 类型定义

```typescript
// ✅ 好的做法
export interface User {
  id: string
  firstName: string      // camelCase
  lastName: string
  createdAt: string
  isActive: boolean
}

// ❌ 避免的做法
export interface User {
  id: string
  first_name: string     // snake_case 在前端
  lastName: string       // 混合使用
  created_at: string
  isActive: boolean
}
```

### 3. API 调用

```typescript
// ✅ 使用统一的 API 客户端
const user = await apiPost<User>('users', {
  firstName: 'John',     // 前端使用 camelCase
  lastName: 'Doe'
})

// 后端自动接收到：
// {
//   "first_name": "John",  // 自动转换为 snake_case
//   "last_name": "Doe"
// }
```

### 4. 错误处理

```typescript
// 在转换层处理错误
export function keysToCamelCase<T>(obj: any): T {
  try {
    // 转换逻辑
  } catch (error) {
    console.warn('Failed to convert keys to camelCase:', error)
    return obj // 返回原始对象作为降级
  }
}
```

## 优势

1. **开发体验**：开发者只需关注业务逻辑，不用处理命名转换
2. **类型安全**：TypeScript 提供完整的类型检查
3. **维护性**：集中管理类型定义，易于维护
4. **一致性**：强制执行命名规范，避免混乱
5. **可扩展**：易于添加新的转换规则

## 注意事项

1. **性能**：转换会有轻微的性能开销，但通常可以忽略
2. **调试**：需要了解转换机制，便于调试
3. **特殊字段**：某些字段可能需要特殊处理（如 ID 字段）
4. **向后兼容**：逐步迁移，保持向后兼容

## 替代方案

### 方案A：后端适配前端

```rust
// 后端使用 serde 重命名
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub first_name: String,  // 序列化为 firstName
    pub last_name: String,   // 序列化为 lastName
}
```

**优点**：前端无需改动
**缺点**：后端代码不符合 Rust 惯例

### 方案B：前端适配后端

```typescript
// 前端使用 snake_case
interface User {
  first_name: string
  last_name: string
}
```

**优点**：后端无需改动
**缺点**：前端代码不符合 JavaScript 惯例

### 方案C：GraphQL/tRPC

使用 GraphQL 或 tRPC 等工具，提供类型安全的 API 层。

**优点**：端到端类型安全
**缺点**：需要额外的工具和学习成本

## 结论

**推荐使用方案1（自动转换层）**，因为：

1. 保持各端的最佳实践
2. 对开发者透明
3. 易于实施和维护
4. 提供完整的类型安全

这个方案在保持代码质量的同时，解决了前后端命名规范不一致的问题，是最平衡的解决方案。