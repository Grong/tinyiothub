'use client'

import React from 'react'
import cn from '@/utils/classnames'

export interface CheckboxWithLabelProps {
  label: string
  isChecked: boolean
  onChange: () => void
  className?: string
  disabled?: boolean
}

const CheckboxWithLabel: React.FC<CheckboxWithLabelProps> = ({
  label,
  isChecked,
  onChange,
  className,
  disabled = false,
}) => {
  return (
    <label className={cn(
      'flex items-center cursor-pointer',
      disabled && 'cursor-not-allowed opacity-50',
      className
    )}>
      <input
        type="checkbox"
        checked={isChecked}
        onChange={onChange}
        disabled={disabled}
        className="sr-only"
      />
      <div className={cn(
        'relative flex items-center justify-center w-4 h-4 border rounded',
        isChecked
          ? 'bg-components-button-primary-bg border-components-button-primary-border'
          : 'bg-components-panel-bg border-divider-regular',
        !disabled && 'hover:border-components-button-primary-border',
        disabled && 'opacity-50'
      )}>
        {isChecked && (
          <svg
            className="w-3 h-3 text-components-button-primary-text"
            fill="currentColor"
            viewBox="0 0 20 20"
          >
            <path
              fillRule="evenodd"
              d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z"
              clipRule="evenodd"
            />
          </svg>
        )}
      </div>
      <span className={cn(
        'ml-2 text-sm text-text-secondary',
        disabled && 'opacity-50'
      )}>
        {label}
      </span>
    </label>
  )
}

export default CheckboxWithLabel