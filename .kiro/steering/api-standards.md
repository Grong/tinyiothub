# API 开发规范

## 核心原则

**所有API端点必须使用统一的响应格式，确保前后端数据对接的一致性。**

## 1. 响应格式标准

### 统一响应结构

所有API端点必须返回以下格式：

```rust
Json<ApiResponse<T>>
```

其中 `ApiResponse<T>` 结构为：

```rust
{
    "code": 0,           // 0表示成功，非0表示错误
    "msg": "",           // 错误信息，成功时为空字符串
    "result": T | null   // 实际数据，错误时为null
}
```

### 正确示例

```rust
// ✅ 正确的API函数签名
async fn list_devices(
    Query(params): Query<DeviceQuery>,
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<Device>>> {
    // 业务逻辑
    let devices = get_devices(&params).await?;
    ApiResponseBuilder::success(devices)
}

// ✅ 使用构建器创建响应
use crate::dto::response::builder::ApiResponseBuilder;

// 成功响应
ApiResponseBuilder::success(data)

// 错误响应
ApiResponseBuilder::error("错误信息")

// 带错误码的响应
ApiResponseBuilder::error_with_code(400, "参数错误")
```

### 错误示例

```rust
// ❌ 错误：直接返回数据
async fn bad_endpoint() -> Json<Vec<Device>> {
    Json(devices)
}

// ❌ 错误：返回Result而不是ApiResponse
async fn bad_endpoint() -> Result<Json<Device>, StatusCode> {
    Ok(Json(device))
}

// ❌ 错误：手动构造ApiResponse
async fn bad_endpoint() -> Json<ApiResponse<Device>> {
    Json(ApiResponse {
        code: 0,
        msg: "".to_string(),
        result: Some(device),
    })
}
```

## 2. 字段命名规范

### 后端（Rust）
- 使用 `snake_case` 命名
- 结构体字段：`device_name`, `created_at`, `is_active`

### 前端（TypeScript）
- 使用 `camelCase` 命名
- 接口字段：`deviceName`, `createdAt`, `isActive`

### 自动转换
前端API客户端会自动处理命名格式转换：
- 请求参数：`camelCase` → `snake_case`
- 响应数据：`snake_case` → `camelCase`

## 3. 前端API调用规范

### 统一API客户端

**所有前端API调用必须使用统一的API客户端，不允许直接使用fetch或其他HTTP库。**

```typescript
// ✅ 正确：使用统一API客户端
import { apiGet, apiPost, apiPut, apiDelete } from '@/lib/api-client'

// GET请求
const response = await apiGet<UserList>('users', { page: 1, pageSize: 20 })

// POST请求
const response = await apiPost<User>('users', userData)

// PUT请求
const response = await apiPut<User>(`users/${id}`, updateData)

// DELETE请求
const response = await apiDelete<boolean>(`users/${id}`)
```

```typescript
// ❌ 错误：直接使用fetch
const response = await fetch('/api/users')

// ❌ 错误：直接使用其他HTTP库
const response = await axios.get('/api/users')
```

### Service层结构规范

每个功能模块必须有对应的service文件，统一管理API调用：

```typescript
// web/service/users.ts
import { apiGet, apiPost, apiPut, apiDelete } from '@/lib/api-client'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'

// 1. 定义类型接口
export interface User {
  id: string
  name: string
  email: string
  createdAt: string  // 自动从created_at转换
}

export interface CreateUserRequest {
  name: string
  email: string
  password: string
}

// 2. API调用函数
export const userApi = {
  // 获取用户列表
  getUsers: (params?: { page?: number; pageSize?: number }) => 
    apiGet<User[]>('users', params),
    
  // 获取用户详情
  getUser: (id: string) => 
    apiGet<User>(`users/${id}`),
    
  // 创建用户
  createUser: (data: CreateUserRequest) => 
    apiPost<User>('users', data),
    
  // 更新用户
  updateUser: (id: string, data: Partial<User>) => 
    apiPut<User>(`users/${id}`, data),
    
  // 删除用户
  deleteUser: (id: string) => 
    apiDelete<boolean>(`users/${id}`),
}

// 3. React Query Hooks
export const useUsers = (params?: { page?: number; pageSize?: number }) => {
  return useQuery({
    queryKey: queryKeys.users.list(params || {}),
    queryFn: async () => {
      const response = await userApi.getUsers(params)
      return response.result || []
    },
  })
}

export const useUser = (id: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.users.detail(id),
    queryFn: async () => {
      const response = await userApi.getUser(id)
      return response.result
    },
    enabled: enabled && !!id,
  })
}

export const useCreateUser = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: userApi.createUser,
    onSuccess: (response) => {
      // 刷新用户列表
      queryClient.invalidateQueries({ queryKey: queryKeys.users.all })
      return response.result
    },
  })
}
```

### 组件中的使用规范

```typescript
// ✅ 正确：在组件中使用service hooks
import { useUsers, useCreateUser } from '@/service/users'

const UserList: React.FC = () => {
  const { data: users, isLoading, error } = useUsers({ page: 1, pageSize: 20 })
  const createUserMutation = useCreateUser()
  
  const handleCreateUser = async (userData: CreateUserRequest) => {
    try {
      await createUserMutation.mutateAsync(userData)
      // 成功处理
    } catch (error) {
      // 错误处理
      console.error('创建用户失败:', error)
    }
  }
  
  if (isLoading) return <div>加载中...</div>
  if (error) return <div>加载失败: {error.message}</div>
  
  return (
    <div>
      {users?.map(user => (
        <div key={user.id}>{user.name}</div>
      ))}
    </div>
  )
}
```

```typescript
// ❌ 错误：在组件中直接调用API
const UserList: React.FC = () => {
  const [users, setUsers] = useState([])
  
  useEffect(() => {
    // 不要这样做
    fetch('/api/users')
      .then(res => res.json())
      .then(setUsers)
  }, [])
  
  // ...
}
```

## 4. 错误处理规范

### 成功响应
```rust
ApiResponseBuilder::success(data)
// 结果：{ "code": 0, "msg": "", "result": data }
```

### 错误响应
```rust
ApiResponseBuilder::error("用户不存在")
// 结果：{ "code": -1, "msg": "用户不存在", "result": null }

ApiResponseBuilder::error_with_code(404, "资源未找到")
// 结果：{ "code": 404, "msg": "资源未找到", "result": null }
```

### 前端错误处理
```typescript
// 统一错误处理
try {
  const response = await userApi.getUsers()
  if (response.code === 0) {
    // 成功处理
    return response.result
  } else {
    // 错误处理
    throw new Error(response.msg)
  }
} catch (error) {
  console.error('API调用失败:', error)
  throw error
}
```

## 5. 认证规范

### 需要认证的端点
大部分API端点都需要JWT认证，函数签名中包含 `_claims: Claims` 参数：

```rust
async fn protected_endpoint(
    State(state): State<AppState>,
    _claims: Claims,  // 表示需要认证
) -> Json<ApiResponse<T>> {
    // 业务逻辑
}
```

### 公开端点
不需要认证的端点（如登录、健康检查）：

```rust
async fn public_endpoint(
    State(state): State<AppState>,
) -> Json<ApiResponse<T>> {
    // 业务逻辑
}
```

## 6. 分页规范

### 查询参数
```rust
#[derive(Deserialize)]
pub struct PaginationQuery {
    pub page: Option<u32>,      // 页码，从1开始，默认1
    pub page_size: Option<u32>, // 每页大小，默认20
}
```

### 响应格式
```rust
#[derive(Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}

#[derive(Serialize)]
pub struct PaginationInfo {
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
    pub total_count: u64,
}
```

## 7. 开发检查清单

### 后端API开发
在创建新的API端点时，请检查：

- [ ] 函数返回类型是 `Json<ApiResponse<T>>`
- [ ] 使用 `ApiResponseBuilder` 创建响应
- [ ] 错误处理返回适当的错误信息
- [ ] 字段命名使用 `snake_case`
- [ ] 需要认证的端点包含 `_claims: Claims` 参数
- [ ] 分页接口使用统一的分页结构
- [ ] 添加适当的日志记录

### 前端Service开发
在创建新的service时，请检查：

- [ ] 使用统一的API客户端（`apiGet`, `apiPost`等）
- [ ] 定义清晰的TypeScript接口
- [ ] 创建对应的React Query hooks
- [ ] 使用正确的query keys
- [ ] 实现适当的错误处理
- [ ] 不直接使用fetch或其他HTTP库

### 组件开发
在组件中使用API时，请检查：

- [ ] 使用service层提供的hooks
- [ ] 不在组件中直接调用API
- [ ] 实现loading和error状态处理
- [ ] 使用mutation hooks处理数据修改
- [ ] 正确处理异步操作

## 8. 常见问题和解决方案

### 问题1：驱动下拉框为空
**原因**：后端返回格式不是 `ApiResponse` 包装
**解决**：确保所有API都使用 `ApiResponseBuilder::success(data)`

### 问题2：字段名不匹配
**原因**：后端使用 `snake_case`，前端期望 `camelCase`
**解决**：依赖前端API客户端的自动转换

### 问题3：认证失败
**原因**：API需要JWT token但前端未提供
**解决**：确保前端在请求头中包含 `Authorization: Bearer <token>`

### 问题4：组件中直接调用API
**原因**：没有使用统一的service层
**解决**：创建对应的service文件和hooks

## 9. 工具和自动化

### API格式检查脚本
使用 `scripts/test-api-format.py` 验证API响应格式：

```bash
python3 scripts/test-api-format.py
```

### 代码审查
在代码审查时，重点检查：
1. API响应格式是否统一
2. 是否使用统一的API客户端
3. Service层结构是否规范
4. 错误处理是否完整
5. 字段命名是否规范
6. 认证逻辑是否正确

## 10. 总结

遵循这些规范可以确保：
- 前后端数据格式一致
- API调用方式统一
- 错误处理统一
- 代码可维护性高
- 开发效率提高
- 维护成本降低

**记住：一致性是关键！所有API都必须遵循相同的模式，所有前端调用都必须使用统一的客户端。**