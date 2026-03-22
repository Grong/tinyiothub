import { useEffect } from 'react'
import { useRouter } from 'next/navigation'
import { useAuthStore } from '@/store/provider'

/**
 * Authentication guard hook
 * Redirects to login page if user is not authenticated
 */
const ALLOWED_REDIRECT_PATHS = ['/dashboard', '/devices', '/monitoring', '/settings', '/tenant/dashboard']
const isAllowedRedirect = (redirect: string): boolean =>
  ALLOWED_REDIRECT_PATHS.some(path => redirect.startsWith(path))

export const useAuthGuard = (redirectTo?: string) => {
  const router = useRouter()
  const { isAuthenticated, isLoading } = useAuthStore()

  useEffect(() => {
    // Don't redirect while still loading
    if (isLoading) return

    // If not authenticated, redirect to login with return URL
    if (!isAuthenticated) {
      const currentPath = window.location.pathname
      // Validate redirect URL against whitelist to prevent open redirect vulnerability
      const validatedRedirect = redirectTo && isAllowedRedirect(redirectTo)
        ? redirectTo
        : currentPath
      const loginUrl = `/signin?redirect=${encodeURIComponent(validatedRedirect)}`

      router.push(loginUrl)
    }
  }, [isAuthenticated, isLoading, router, redirectTo])

  return {
    isAuthenticated,
    isLoading,
    shouldRender: !isLoading && isAuthenticated
  }
}