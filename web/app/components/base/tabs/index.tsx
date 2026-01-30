import React, { createContext, useContext, useState } from 'react'
import cn from '@/utils/classnames'

interface TabsContextValue {
  value: string
  onValueChange: (value: string) => void
}

const TabsContext = createContext<TabsContextValue | undefined>(undefined)

interface TabsProps {
  defaultValue?: string
  value?: string
  onValueChange?: (value: string) => void
  children: React.ReactNode
  className?: string
}

interface TabsListProps {
  children: React.ReactNode
  className?: string
}

interface TabsTriggerProps {
  value: string
  children: React.ReactNode
  className?: string
  disabled?: boolean
}

interface TabsContentProps {
  value: string
  children: React.ReactNode
  className?: string
}

const Tabs = ({ defaultValue, value, onValueChange, children, className }: TabsProps) => {
  const [internalValue, setInternalValue] = useState(defaultValue || '')
  
  const currentValue = value !== undefined ? value : internalValue
  const handleValueChange = (newValue: string) => {
    if (value === undefined) {
      setInternalValue(newValue)
    }
    onValueChange?.(newValue)
  }

  return (
    <TabsContext.Provider value={{ value: currentValue, onValueChange: handleValueChange }}>
      <div className={className}>
        {children}
      </div>
    </TabsContext.Provider>
  )
}

const TabsList = ({ children, className }: TabsListProps) => {
  return (
    <div
      className={cn(
        'inline-flex h-10 items-center justify-center rounded-md bg-components-panel-on-panel-item-bg p-1 text-text-tertiary',
        className
      )}
    >
      {children}
    </div>
  )
}

const TabsTrigger = ({ value, children, className, disabled }: TabsTriggerProps) => {
  const context = useContext(TabsContext)
  if (!context) {
    throw new Error('TabsTrigger must be used within a Tabs component')
  }

  const isActive = context.value === value

  return (
    <button
      className={cn(
        'inline-flex items-center justify-center whitespace-nowrap rounded-sm px-3 py-1.5 text-sm font-medium ring-offset-background-default transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-components-input-border-active focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50',
        isActive
          ? 'bg-components-card-bg text-text-primary shadow-sm'
          : 'text-text-secondary hover:text-text-primary',
        className
      )}
      onClick={() => !disabled && context.onValueChange(value)}
      disabled={disabled}
    >
      {children}
    </button>
  )
}

const TabsContent = ({ value, children, className }: TabsContentProps) => {
  const context = useContext(TabsContext)
  if (!context) {
    throw new Error('TabsContent must be used within a Tabs component')
  }

  if (context.value !== value) {
    return null
  }

  return (
    <div
      className={cn(
        'mt-2 ring-offset-background-default focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-components-input-border-active focus-visible:ring-offset-2',
        className
      )}
    >
      {children}
    </div>
  )
}

export { Tabs, TabsList, TabsTrigger, TabsContent }