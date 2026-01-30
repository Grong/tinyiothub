'use client'

import type { FC, PropsWithChildren } from 'react'
import { useEffect } from 'react'
import { useAuthStore } from '@/store/provider'

const BrowserInitializer: FC<PropsWithChildren> = ({ children }) => {
  const { initialize } = useAuthStore()

  useEffect(() => {
    // Initialize browser-specific features
    if (typeof window !== 'undefined') {
      // Initialize auth store
      initialize()

      // Set up global error handling
      window.addEventListener('unhandledrejection', (event) => {
        console.error('Unhandled promise rejection:', event.reason)
      })

      // Set up performance monitoring
      if ('performance' in window && 'mark' in window.performance) {
        window.performance.mark('app-start')
      }
    }
  }, [initialize])

  return <>{children}</>
}

export default BrowserInitializer