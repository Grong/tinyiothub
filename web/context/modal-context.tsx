'use client'
import { createContext, useContext, useState, ReactNode } from 'react'

interface ModalContextType {
  setShowAccountSettingModal: (options?: { payload?: string }) => void
}

const ModalContext = createContext<ModalContextType | undefined>(undefined)

export function ModalProvider({ children }: { children: ReactNode }) {
  const setShowAccountSettingModal = (options?: { payload?: string }) => {
    // TODO: Implement account setting modal
    console.log('Show account setting modal:', options)
  }

  return (
    <ModalContext.Provider value={{
      setShowAccountSettingModal,
    }}>
      {children}
    </ModalContext.Provider>
  )
}

export function useModalContext() {
  const context = useContext(ModalContext)
  if (context === undefined) {
    throw new Error('useModalContext must be used within a ModalProvider')
  }
  return context
}