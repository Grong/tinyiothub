'use client'

import { useEffect } from 'react'
import { useAuthStore } from '@/store/provider'

export function useAuth() {
  const { user, isAuthenticated, isLoading, initialize } = useAuthStore()

  useEffect(() => {
    initialize()
  }, [initialize])

  return {
    user,
    isAuthenticated,
    isLoading,
  }
}