/**
 * 应用常量定义
 */

// 分页配置
export const PAGINATION = {
  DEFAULT_PAGE: 1,
  DEFAULT_PAGE_SIZE: 20,
  DEVICE_LIST_PAGE_SIZE: 30,
  MAX_PAGE_SIZE: 100,
} as const

// 显示限制
export const DISPLAY_LIMITS = {
  MAX_VISIBLE_PROPERTIES: 3,
  MAX_VISIBLE_TAGS: 5,
  MAX_DESCRIPTION_LENGTH: 100,
} as const

// 时间格式
export const DATE_FORMATS = {
  FULL: 'YYYY-MM-DD HH:mm:ss',
  DATE_ONLY: 'YYYY-MM-DD',
  TIME_ONLY: 'HH:mm:ss',
  SHORT: 'MM-DD HH:mm',
} as const

// API 配置
export const API_CONFIG = {
  TIMEOUT: 30000, // 30秒
  RETRY_COUNT: 3,
  RETRY_DELAY: 1000, // 1秒
} as const

// 缓存时间（毫秒）
export const CACHE_TIME = {
  SHORT: 1000 * 60, // 1分钟
  MEDIUM: 1000 * 60 * 5, // 5分钟
  LONG: 1000 * 60 * 30, // 30分钟
} as const

// 防抖延迟（毫秒）
export const DEBOUNCE_DELAY = {
  SEARCH: 500,
  FILTER: 500,
  RESIZE: 200,
} as const

// 设备相关
export const DEVICE = {
  MAX_NAME_LENGTH: 50,
  MAX_DESCRIPTION_LENGTH: 200,
  MAX_SN_LENGTH: 50,
} as const

// 文件上传
export const UPLOAD = {
  MAX_FILE_SIZE: 10 * 1024 * 1024, // 10MB
  ALLOWED_IMAGE_TYPES: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'],
  ALLOWED_DOCUMENT_TYPES: ['application/pdf', 'application/msword'],
} as const

// 本地存储键名
export const STORAGE_KEYS = {
  AUTH_TOKEN: 'auth_token',
  USER_INFO: 'user_info',
  THEME: 'theme',
  LANGUAGE: 'language',
} as const

// 路由路径
export const ROUTES = {
  HOME: '/',
  SIGNIN: '/signin',
  DASHBOARD: '/dashboard',
  DEVICES: '/devices',
  DEVICE_DETAIL: (id: string) => `/[deviceId]?id=${id}`,
  ALARMS: '/alarms',
  MONITORING: '/monitoring',
  SETTINGS: '/settings',
  TAGS: '/tags',
  TEMPLATES: '/templates',
} as const
