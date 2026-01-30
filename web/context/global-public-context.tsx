'use client'

import { create } from 'zustand'
import { useQuery } from '@tanstack/react-query'
import type { FC, PropsWithChildren } from 'react'
import { useEffect } from 'react'
import type { SystemFeatures } from '@/types/feature'
import { defaultSystemFeatures } from '@/types/feature'
import { useSystemFeatures } from '@/service/system'
import Loading from '@/app/components/base/loading'

type GlobalPublicStore = {
  isGlobalPending: boolean
  setIsGlobalPending: (isPending: boolean) => void
  systemFeatures: SystemFeatures
  setSystemFeatures: (systemFeatures: SystemFeatures) => void
}

export const useGlobalPublicStore = create<GlobalPublicStore>(set => ({
  isGlobalPending: true,
  setIsGlobalPending: (isPending: boolean) => set(() => ({ isGlobalPending: isPending })),
  systemFeatures: defaultSystemFeatures,
  setSystemFeatures: (systemFeatures: SystemFeatures) => set(() => ({ systemFeatures })),
}))

const GlobalPublicStoreProvider: FC<PropsWithChildren> = ({ children }) => {
  const { 
    isPending, 
    data: systemFeaturesResponse,
    error 
  } = useSystemFeatures()
  
  const { setSystemFeatures, setIsGlobalPending: setIsPending } = useGlobalPublicStore()
  
  useEffect(() => {
    if (systemFeaturesResponse?.result) {
      setSystemFeatures({ ...defaultSystemFeatures, ...systemFeaturesResponse.result })
    }
  }, [systemFeaturesResponse, setSystemFeatures])

  useEffect(() => {
    setIsPending(isPending)
  }, [isPending, setIsPending])

  // 错误处理：如果加载失败，使用默认特性并继续
  useEffect(() => {
    if (error) {
      console.warn('Failed to load system features, using defaults:', error)
      setSystemFeatures(defaultSystemFeatures)
      setIsPending(false)
    }
  }, [error, setSystemFeatures, setIsPending])

  if (isPending) {
    return (
      <div className='flex h-screen w-screen items-center justify-center'>
        <Loading />
      </div>
    )
  }
  
  return <>{children}</>
}

export default GlobalPublicStoreProvider