'use client'

import React from 'react'
import DeviceInfoStep from './device-info-step'
import TemplateDetailsView from './template-details-view'
import WizardActions from './wizard-actions'
import { useLocalizedText } from '@/utils/i18n-template'
import type { ProcessedDeviceTemplate } from '@/service/templates'

interface DeviceInfoPageProps {
  selectedTemplate: ProcessedDeviceTemplate
  deviceName: string
  onDeviceNameChange: (name: string) => void
  deviceDescription: string
  onDeviceDescriptionChange: (description: string) => void
  deviceAddress: string
  onDeviceAddressChange: (address: string) => void
  devicePosition: string
  onDevicePositionChange: (position: string) => void
  selectedDriverName: string
  onDriverNameChange: (driverName: string) => void
  driverConfig: Record<string, string>
  onDriverConfigChange: (config: Record<string, string>) => void
  validationErrors: Record<string, string>
  isFormValid: boolean
  isCreating: boolean
  onPreviousStep: () => void
  onClose: () => void
  onCreate: () => void
}

const DeviceInfoPage: React.FC<DeviceInfoPageProps> = ({
  selectedTemplate,
  deviceName,
  onDeviceNameChange,
  deviceDescription,
  onDeviceDescriptionChange,
  deviceAddress,
  onDeviceAddressChange,
  devicePosition,
  onDevicePositionChange,
  selectedDriverName,
  onDriverNameChange,
  driverConfig,
  onDriverConfigChange,
  validationErrors,
  isFormValid,
  isCreating,
  onPreviousStep,
  onClose,
  onCreate,
}) => {
  const getLocalizedText = useLocalizedText()
  
  const templateDisplayName = getLocalizedText(selectedTemplate.displayName || {}, selectedTemplate.name)
  const canCreate = isFormValid && !isCreating

  return (
    <div className="flex h-full">
      {/* 左侧内容区域 - 自动宽度 */}
      <div className="flex shrink-0">
        <div className="flex h-full w-[680px] flex-col px-10">
          {/* 可滚动的内容区域 */}
          <div className="flex-1 overflow-y-auto pt-6">
            <div className="w-full max-w-[660px]">
              <DeviceInfoStep
                selectedTemplate={selectedTemplate}
                deviceName={deviceName}
                onDeviceNameChange={onDeviceNameChange}
                deviceDescription={deviceDescription}
                onDeviceDescriptionChange={onDeviceDescriptionChange}
                deviceAddress={deviceAddress}
                onDeviceAddressChange={onDeviceAddressChange}
                devicePosition={devicePosition}
                onDevicePositionChange={onDevicePositionChange}
                selectedDriverName={selectedDriverName}
                onDriverNameChange={onDriverNameChange}
                driverConfig={driverConfig}
                onDriverConfigChange={onDriverConfigChange}
                validationErrors={validationErrors}
              />
            </div>
          </div>

          {/* 固定底部：操作按钮 */}
          <div className="flex-shrink-0 pb-10 pt-5">
            <WizardActions
              currentStep="device"
              canCreate={canCreate}
              isCreating={isCreating}
              templateDisplayName={templateDisplayName}
              onPreviousStep={onPreviousStep}
              onClose={onClose}
              onCreate={onCreate}
            />
          </div>
        </div>
      </div>

      {/* 右侧预览区域 - 填充剩余空间 */}
      <div className="relative flex h-full flex-1 overflow-hidden border-l border-divider-subtle">
        <div className="flex h-full w-full flex-col">
          {/* 预览内容 */}
          <div className="px-8 py-4">
            <h4 className="system-sm-semibold-uppercase text-text-secondary">
              {templateDisplayName}
            </h4>
            <div className="system-xs-regular mt-1 min-h-8 max-w-96 text-text-tertiary">
              <span>{getLocalizedText(selectedTemplate.description || {}, '') || '基于此模板创建设备'}</span>
            </div>
          </div>
          {/* 预览区域 */}
          <div className="h-full w-full overflow-y-auto bg-components-panel-bg">
            <div className="p-6">
              <TemplateDetailsView template={selectedTemplate} />
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export default DeviceInfoPage