'use client'
import React, { createContext, useContext } from 'react'

interface ProviderContextType {
  enableBilling: boolean
  plan: {
    type: string
  }
  isEducationAccount: boolean
}

const ProviderContext = createContext<ProviderContextType | undefined>(undefined)

export const ProviderContextProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const value: ProviderContextType = {
    enableBilling: false,
    plan: {
      type: 'free',
    },
    isEducationAccount: false,
  }

  return (
    <ProviderContext.Provider value={value}>
      {children}
    </ProviderContext.Provider>
  )
}

export const useProviderContext = () => {
  const context = useContext(ProviderContext)
  if (context === undefined) {
    throw new Error('useProviderContext must be used within a ProviderContextProvider')
  }
  return context
}