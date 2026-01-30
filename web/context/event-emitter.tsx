'use client'
import React, { createContext, useContext } from 'react'

interface EventEmitterContextType {
  eventEmitter?: {
    useSubscription: (callback: (event: any) => void) => void
  }
}

const EventEmitterContext = createContext<EventEmitterContextType | undefined>(undefined)

export const EventEmitterContextProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const value: EventEmitterContextType = {
    eventEmitter: undefined,
  }

  return (
    <EventEmitterContext.Provider value={value}>
      {children}
    </EventEmitterContext.Provider>
  )
}

export const useEventEmitterContextContext = () => {
  const context = useContext(EventEmitterContext)
  if (context === undefined) {
    throw new Error('useEventEmitterContextContext must be used within a EventEmitterContextProvider')
  }
  return context
}