'use client'

import { useEffect } from 'react'
import { useAuthStore } from '@/store/provider'
import { useAuthErrorHandler } from '@/hooks/use-auth-error-handler'
import Header from '@/app/components/header'
import ErrorBoundary from '@/app/components/error-boundary'

export default function CommonLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const { isAuthenticated, isLoading } = useAuthStore()
  
  // 启用全局认证错误处理
  useAuthErrorHandler()

  // 认证检查
  useEffect(() => {
    if (!isLoading && !isAuthenticated) {
      window.location.href = '/signin'
    }
  }, [isAuthenticated, isLoading])

  if (isLoading) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600"></div>
      </div>
    )
  }

  if (!isAuthenticated) {
    return null
  }

  return (
    <ErrorBoundary>
      <div className="flex h-screen flex-col bg-background-body">
        <div className="sticky left-0 right-0 top-0 z-[30] flex min-h-[56px] shrink-0 grow-0 basis-auto flex-col border-b border-divider-regular">
          <Header />
        </div>
        <main className="relative flex flex-1 flex-col overflow-auto bg-background-body">
          {children}
        </main>
      </div>
    </ErrorBoundary>
  )
}