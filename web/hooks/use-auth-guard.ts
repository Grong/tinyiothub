import { useEffect } from 'react'
import { useRouter } from 'next/navigation'
import { useAuthStore } from '@/store/provider'

/**
 * Authentication guard hook
 * Redirects to login page if user is not authenticated
 */
export const useAuthGuard = (redirectTo?: string) => {
  const router = useRouter()
  const { isAuthenticated, isLoading } = useAuthStore()

  useEffect(() => {
    // Don't redirect while still loading
    if (isLoading) return

    // If not authenticated, redirect to login with return URL
    if (!isAuthenticated) {
      const currentPath = window.location.pathname
      const loginUrl = redirectTo 
        ? `/signin?redirect=${encodeURIComponent(redirectTo)}`
        : `/signin?redirect=${encodeURIComponent(currentPath)}`
      
      router.push(loginUrl)
    }
  }, [isAuthenticated, isLoading, router, redirectTo])

  return {
    isAuthenticated,
    isLoading,
    shouldRender: !isLoading && isAuthenticated
  }
}