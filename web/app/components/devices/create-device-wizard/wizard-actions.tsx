'use client'

import React from 'react'
import { RiArrowLeftLine, RiCommandLine, RiCornerDownLeftLine } from '@remixicon/react'
import Button from '@/app/components/base/button'
import type { WizardStep } from './wizard-controller'

interface WizardActionsProps {
  currentStep: WizardStep
  canCreate: boolean
  isCreating: boolean
  templateDisplayName?: string
  onPreviousStep: () => void
  onClose: () => void
  onCreate: () => void
}

const WizardActions: React.FC<WizardActionsProps> = ({
  currentStep,
  canCreate,
  isCreating,
  templateDisplayName,
  onPreviousStep,
  onClose,
  onCreate,
}) => {
  return (
    <div className="flex items-center justify-between">
      <div className="system-xs-regular text-text-tertiary">
        {currentStep === 'template' && (
          <span>选择一个模板开始创建设备</span>
        )}
        {currentStep === 'device' && templateDisplayName && (
          <span>基于 {templateDisplayName} 模板创建</span>
        )}
      </div>
      <div className="flex gap-2">
        {currentStep === 'device' && (
          <Button onClick={onPreviousStep} className="gap-1">
            <RiArrowLeftLine size={16} />
            <span>上一步</span>
          </Button>
        )}
        <Button onClick={onClose}>取消</Button>
        {currentStep === 'device' && (
          <Button 
            disabled={!canCreate || isCreating} 
            className="gap-1" 
            variant="primary" 
            onClick={onCreate}
          >
            <span>{isCreating ? '创建中...' : '创建设备'}</span>
            <div className="flex gap-0.5">
              <RiCommandLine size={14} className="system-kbd rounded-sm bg-components-kbd-bg-white p-0.5" />
              <RiCornerDownLeftLine size={14} className="system-kbd rounded-sm bg-components-kbd-bg-white p-0.5" />
            </div>
          </Button>
        )}
      </div>
    </div>
  )
}

export default WizardActions