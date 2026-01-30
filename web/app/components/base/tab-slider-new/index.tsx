'use client'

import React from 'react'
import cn from '@/utils/classnames'

export interface TabOption {
  value: string
  text: string
  icon?: React.ReactNode
}

interface TabSliderNewProps {
  value: string
  onChange: (value: string) => void
  options: TabOption[]
  className?: string
}

const TabSliderNew: React.FC<TabSliderNewProps> = ({
  value,
  onChange,
  options,
  className,
}) => {
  return (
    <div className={cn('flex items-center gap-1 rounded-lg bg-components-input-bg-normal p-1', className)}>
      {options.map((option) => (
        <button
          key={option.value}
          onClick={() => onChange(option.value)}
          className={cn(
            'flex items-center gap-1 rounded-md px-3 py-1.5 text-sm font-medium transition-colors',
            value === option.value
              ? 'bg-components-button-primary-bg text-components-button-primary-text shadow-xs'
              : 'text-text-secondary hover:bg-state-base-hover hover:text-text-primary'
          )}
        >
          {option.icon}
          {option.text}
        </button>
      ))}
    </div>
  )
}

export default TabSliderNew