'use client'
import React, { createContext, useContext, ReactNode } from 'react'

interface AppContextType {
  isCurrentWorkspaceEditor: boolean
  isCurrentWorkspaceDatasetOperator: boolean
}

const AppContext = createContext<AppContextType | undefined>(undefined)

export function AppProvider({ children }: { children: ReactNode }) {
  // For now, assume user has all permissions
  const isCurrentWorkspaceEditor = true
  const isCurrentWorkspaceDatasetOperator = true

  // 临时设置认证token用于测试
  React.useEffect(() => {
    if (typeof window !== 'undefined') {
      const token = localStorage.getItem('auth-token')
      if (!token) {
        // 设置一个临时的测试token
        const testToken = 'test-token-for-development'
        localStorage.setItem('auth-token', testToken)
        console.log('设置临时认证token用于测试')
      }
    }
  }, [])

  return (
    <AppContext.Provider value={{
      isCurrentWorkspaceEditor,
      isCurrentWorkspaceDatasetOperator,
    }}>
      {children}
    </AppContext.Provider>
  )
}

export function useAppContext() {
  const context = useContext(AppContext)
  if (context === undefined) {
    throw new Error('useAppContext must be used within an AppProvider')
  }
  return context
}