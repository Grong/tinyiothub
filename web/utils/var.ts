export const basePath = process.env.NEXT_PUBLIC_BASE_PATH || ''

export const isDev = process.env.NODE_ENV === 'development'
export const isProd = process.env.NODE_ENV === 'production'

export const getBaseUrl = () => {
  if (typeof window !== 'undefined') {
    return window.location.origin
  }
  return process.env.NEXT_PUBLIC_WEB_PREFIX || 'http://localhost:3000'
}