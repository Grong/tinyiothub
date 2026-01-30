'use client'

import type { FC, PropsWithChildren } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'

const STALE_TIME = 1000 * 60 * 30 // 30 minutes

const client = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: STALE_TIME,
      retry: (failureCount, error: any) => {
        // Don't retry on 4xx errors
        if (error?.status >= 400 && error?.status < 500) {
          return false
        }
        return failureCount < 3
      },
    },
  },
})

export const TanstackQueryInitializer: FC<PropsWithChildren> = ({ children }) => {
  return (
    <QueryClientProvider client={client}>
      {children}
      <ReactQueryDevtools initialIsOpen={false} />
    </QueryClientProvider>
  )
}