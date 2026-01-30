'use client'

import { createContext, useContext, useRef, useEffect } from 'react'
import { useStore } from 'zustand'
import { createAuthStore, type AuthStore } from './auth'

type StoreApi = ReturnType<typeof createAuthStore>

const StoreContext = createContext<StoreApi | undefined>(undefined)

export interface StoreProviderProps {
  children: React.ReactNode
}

export const StoreProvider = ({ children }: StoreProviderProps) => {
  const storeRef = useRef<StoreApi | undefined>(undefined)
  if (!storeRef.current) {
    storeRef.current = createAuthStore()
  }

  // 初始化认证状态
  useEffect(() => {
    if (storeRef.current) {
      storeRef.current.getState().initialize()
    }
  }, [])

  return (
    <StoreContext.Provider value={storeRef.current}>
      {children}
    </StoreContext.Provider>
  )
}

export const useStoreContext = () => {
  const context = useContext(StoreContext)
  if (!context) {
    throw new Error('useStoreContext must be used within StoreProvider')
  }
  return context
}

export const useAuthStore = () => {
  const store = useStoreContext()
  return useStore(store)
}