'use client'

import React from 'react'
import { cn } from '@/utils/classnames'
import type { WizardStep } from './wizard-controller'

interface StepIndicatorProps {
  currentStep: WizardStep
}

const StepIndicator: React.FC<StepIndicatorProps> = ({ currentStep }) => {
  return (
    <div className="flex items-center gap-2">
      <div className={cn(
        "flex h-6 w-6 items-center justify-center rounded-full text-xs font-medium",
        currentStep === 'template' 
          ? "bg-components-button-primary-bg text-components-button-primary-text"
          : "bg-components-button-secondary-bg text-components-button-secondary-text"
      )}>
        1
      </div>
      <span className={cn(
        "text-sm",
        currentStep === 'template' ? "text-text-primary" : "text-text-secondary"
      )}>
        选择模板
      </span>
      <div className="h-px w-8 bg-divider-subtle" />
      <div className={cn(
        "flex h-6 w-6 items-center justify-center rounded-full text-xs font-medium",
        currentStep === 'device' 
          ? "bg-components-button-primary-bg text-components-button-primary-text"
          : "bg-components-button-secondary-bg text-components-button-secondary-text"
      )}>
        2
      </div>
      <span className={cn(
        "text-sm",
        currentStep === 'device' ? "text-text-primary" : "text-text-secondary"
      )}>
        设备信息
      </span>
    </div>
  )
}

export default StepIndicator