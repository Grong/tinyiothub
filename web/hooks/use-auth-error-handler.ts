'use client'

import { useEffect } from 'react'
import { useAuthStore } from '@/store/provider'

/**
 * Global authentication error handler
 * Listens for auth errors and redirects to login page
 */
export const useAuthErrorHandler = () => {
  const { logout } = useAuthStore()

  useEffect(() => {
    const handleAuthError = (event: CustomEvent) => {
      // Clear auth state
      logout()
      
      // Get current path for redirect after login
      const currentPath = window.location.pathname
      const redirectParam = currentPath !== '/signin' ? `?redirect=${encodeURIComponent(currentPath)}` : ''
      
      // Redirect to login page - 使用 window.location.href 以支持静态导出
      window.location.href = `/signin${redirectParam}`
    }

    // Listen for auth error events
    window.addEventListener('auth-error', handleAuthError as EventListener)

    return () => {
      window.removeEventListener('auth-error', handleAuthError as EventListener)
    }
  }, [logout])
}