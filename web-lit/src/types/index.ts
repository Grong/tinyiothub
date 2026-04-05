// 通用类型定义

export interface ApiResponse<T = any> {
  code: number
  msg: string
  result: T | null
}

export interface PaginationParams {
  page?: number
  pageSize?: number
  sortBy?: string
  sortOrder?: 'asc' | 'desc'
}

export interface PaginatedResponse<T> {
  data: T[]
  pagination: {
    page: number
    pageSize: number
    totalPages: number
    totalCount: number
  }
}

// 重新导出所有类型
export * from './user'
export * from './device'
export * from './tag'
export * from './alarm'
export * from './system'
export * from './dashboard'
export * from './template'