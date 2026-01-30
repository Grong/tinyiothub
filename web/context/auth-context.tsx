'use client'

import { createContext, useContext, useRef } from 'react'
import { useStore } from 'zustand'
import { createAuthStore, type AuthStore } from '@/store/auth'

type StoreApi = ReturnType<typeof createAuthStore>

const AuthContext = createContext<StoreApi | undefined>(undefined)

export interface AuthProviderProps {
  children: React.ReactNode
}

export const AuthProvider = ({ children }: AuthProviderProps) => {
  const storeRef = useRef<StoreApi | undefined>(undefined)
  if (!storeRef.current) {
    storeRef.current = createAuthStore()
  }

  return (
    <AuthContext.Provider value={storeRef.current}>
      {children}
    </AuthContext.Provider>
  )
}

export const useAuthContext = () => {
  const context = useContext(AuthContext)
  if (!context) {
    throw new Error('useAuthContext must be used within AuthProvider')
  }
  return context
}

export const useAuthStore = () => {
  const store = useAuthContext()
  return useStore(store)
}