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