/**
 * 统一错误处理 Hook
 */

import { useCallback } from 'react'
import { useToast } from './use-toast'

interface ErrorHandlerOptions {
  showToast?: boolean
  logToConsole?: boolean
  context?: string
}

export const useErrorHandler = () => {
  const { toast } = useToast()

  const handleError = useCallback(
    (error: unknown, options: ErrorHandlerOptions = {}) => {
      const {
        showToast = true,
        logToConsole = process.env.NODE_ENV === 'development',
        context = 'Unknown',
      } = options

      // 提取错误信息
      let errorMessage = '操作失败'
      if (error instanceof Error) {
        errorMessage = error.message
      } else if (typeof error === 'string') {
        errorMessage = error
      } else if (error && typeof error === 'object' && 'message' in error) {
        errorMessage = String(error.message)
      }

      // 控制台日志（仅开发环境）
      if (logToConsole) {
        console.error(`[Error in ${context}]:`, error)
      }

      // 显示 Toast 提示
      if (showToast) {
        toast.error(errorMessage)
      }

      return errorMessage
    },
    [toast]
  )

  return { handleError }
}

/**
 * API 错误处理 Hook
 */
export const useApiErrorHandler = () => {
  const { handleError } = useErrorHandler()

  const handleApiError = useCallback(
    (error: unknown, context?: string) => {
      // API 错误通常包含更多信息
      if (error && typeof error === 'object' && 'response' in error) {
        const apiError = error as { response?: { data?: { msg?: string } } }
        const message = apiError.response?.data?.msg || '请求失败'
        return handleError(new Error(message), { context })
      }

      return handleError(error, { context })
    },
    [handleError]
  )

  return { handleApiError }
}
