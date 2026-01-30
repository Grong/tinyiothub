'use client'

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ThemeProvider } from 'next-themes'
import { useState } from 'react'
import { StoreProvider } from '@/store/provider'
import { AppProvider } from '@/context/app-context'
import { ModalProvider } from '@/context/modal-context'
import I18NProvider from '@/app/components/providers/i18n-provider'

export function Providers({ children }: { children: React.ReactNode }) {
  const [queryClient] = useState(() => new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 60 * 1000, // 1分钟
        refetchOnWindowFocus: false, // 禁用窗口聚焦时自动刷新
        refetchOnReconnect: false, // 禁用网络重连时自动刷新
        refetchOnMount: false, // 禁用组件挂载时自动刷新
        retry: (failureCount, error: any) => {
          // 不重试 4xx 错误
          if (error?.status >= 400 && error?.status < 500) {
            return false
          }
          return failureCount < 3
        },
      },
    },
  }))

  return (
    <QueryClientProvider client={queryClient}>
      <ThemeProvider 
        attribute="data-theme" 
        defaultTheme="system" 
        enableSystem
        disableTransitionOnChange
      >
        <I18NProvider>
          <StoreProvider>
            <AppProvider>
              <ModalProvider>
                {children}
              </ModalProvider>
            </AppProvider>
          </StoreProvider>
        </I18NProvider>
      </ThemeProvider>
    </QueryClientProvider>
  )
}