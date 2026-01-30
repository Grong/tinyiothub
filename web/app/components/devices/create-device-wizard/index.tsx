'use client'

import React from 'react'
import FullScreenModal from '@/app/components/base/fullscreen-modal'
import StepIndicator from './step-indicator'
import { useWizardController } from './wizard-controller'
import TemplateSelectionPage from './template-selection-page'
import DeviceInfoPage from './device-info-page'

interface CreateDeviceWizardModalProps {
  isShow: boolean
  onClose: () => void
  onSuccess: () => void
}

const CreateDeviceWizardModal: React.FC<CreateDeviceWizardModalProps> = ({
  isShow,
  onClose,
  onSuccess,
}) => {
  const [state, actions] = useWizardController({ onSuccess })

  const handleClose = () => {
    actions.resetState()
    onClose()
  }

  if (!isShow) return null

  return (
    <FullScreenModal
      overflowVisible
      closable
      open={isShow}
      onClose={handleClose}
    >
      <div className="flex h-full flex-col">
        {/* 统一的固定头部：标题和步骤指示器 */}
          <div className="flex-shrink-0 px-12 py-6">
            <div className="mx-auto ">
              <h1 className="title-2xl-semi-bold mb-2 text-text-primary">
                创建设备
              </h1>
              <StepIndicator currentStep={state.currentStep} />
            </div>
          </div>

        {/* 页面内容 */}
        <div className="flex-1 overflow-hidden">
          {state.currentStep === 'template' ? (
            <TemplateSelectionPage
              searchQuery={state.searchQuery}
              onSearchChange={actions.setSearchQuery}
              templates={state.filteredTemplates}
              isLoading={state.isLoading}
              onTemplateSelect={actions.handleTemplateSelect}
              onClose={handleClose}
            />
          ) : (
            state.selectedTemplate && (
              <DeviceInfoPage
                selectedTemplate={state.selectedTemplate}
                deviceName={state.deviceName}
                onDeviceNameChange={actions.setDeviceName}
                deviceDescription={state.deviceDescription}
                onDeviceDescriptionChange={actions.setDeviceDescription}
                deviceAddress={state.deviceAddress}
                onDeviceAddressChange={actions.setDeviceAddress}
                devicePosition={state.devicePosition}
                onDevicePositionChange={actions.setDevicePosition}
                selectedDriverName={state.selectedDriverName}
                onDriverNameChange={actions.setSelectedDriverName}
                driverConfig={state.driverConfig}
                onDriverConfigChange={actions.setDriverConfig}
                validationErrors={state.validationErrors}
                isFormValid={state.isFormValid}
                isCreating={state.isCreating}
                onPreviousStep={actions.handlePreviousStep}
                onClose={handleClose}
                onCreate={actions.handleCreateDevice}
              />
            )
          )}
        </div>
      </div>
    </FullScreenModal>
  )
}

export default CreateDeviceWizardModal