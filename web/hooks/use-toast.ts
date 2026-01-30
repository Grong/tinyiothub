/**
 * 简单的 Toast Hook
 * 提供基本的消息提示功能
 */

import { useState, useCallback } from 'react'

export interface ToastMessage {
  id: string
  type: 'success' | 'error' | 'warning' | 'info'
  message: string
  duration?: number
}

export const useToast = () => {
  const [toasts, setToasts] = useState<ToastMessage[]>([])

  const addToast = useCallback((toast: Omit<ToastMessage, 'id'>) => {
    const id = Math.random().toString(36).substring(2, 9)
    const newToast: ToastMessage = {
      id,
      duration: 3000,
      ...toast,
    }

    setToasts(prev => [...prev, newToast])

    // 自动移除 toast
    if (newToast.duration && newToast.duration > 0) {
      setTimeout(() => {
        setToasts(prev => prev.filter(t => t.id !== id))
      }, newToast.duration)
    }

    return id
  }, [])

  const removeToast = useCallback((id: string) => {
    setToasts(prev => prev.filter(t => t.id !== id))
  }, [])

  const success = useCallback((message: string, duration?: number) => {
    return addToast({ type: 'success', message, duration })
  }, [addToast])

  const error = useCallback((message: string, duration?: number) => {
    return addToast({ type: 'error', message, duration })
  }, [addToast])

  const warning = useCallback((message: string, duration?: number) => {
    return addToast({ type: 'warning', message, duration })
  }, [addToast])

  const info = useCallback((message: string, duration?: number) => {
    return addToast({ type: 'info', message, duration })
  }, [addToast])

  const clear = useCallback(() => {
    setToasts([])
  }, [])

  return {
    toasts,
    toast: {
      success,
      error,
      warning,
      info,
      remove: removeToast,
      clear,
    },
  }
}