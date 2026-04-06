// 通用类型定义

export type ThemeMode = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

export interface User {
  id: number;
  email: string;
  username: string;
  avatar_url?: string;
}

export interface ApiError {
  message: string;
  code?: string;
  status?: number;
}

export interface PaginationParams {
  page?: number;
  limit?: number;
  offset?: number;
}

export interface PaginatedResponse<T> {
  data: T[];
  total: number;
  page: number;
  limit: number;
  hasMore: boolean;
}

export interface SortParams {
  sortBy?: string;
  sortDir?: 'asc' | 'desc';
}

export interface FilterParams {
  [key: string]: any;
}

export type LoadingState = 'idle' | 'loading' | 'success' | 'error';

export interface AsyncState<T> {
  data: T | null;
  loading: boolean;
  error: string | null;
  state: LoadingState;
}
