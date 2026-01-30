'use client'

import React, { forwardRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { RiAddLine } from '@remixicon/react'
import cn from '@/utils/classnames'
import CreateDeviceWizardModal from './create-device-wizard'

export type NewDeviceCardProps = {
  onSuccess?: () => void
  className?: string
}

const NewDeviceCard = forwardRef<HTMLDivElement, NewDeviceCardProps>(({ onSuccess, className }, ref) => {
  const { t } = useTranslation('device')
  const [showWizardModal, setShowWizardModal] = useState(false)

  const handleSuccess = () => {
    if (onSuccess) {
      onSuccess()
    }
  }

  return (
    <>
      <div
        ref={ref}
        className={cn(
          'group relative col-span-1 flex h-[160px] cursor-pointer flex-col items-center justify-center rounded-xl border-[1px] border-dashed border-components-card-border bg-components-card-bg transition-all duration-200 ease-in-out hover:border-components-button-primary-border hover:bg-components-panel-bg-alt',
          className
        )}
        onClick={() => setShowWizardModal(true)}
      >
        <div className='flex flex-col items-center gap-3'>
          <div className='flex h-10 w-10 items-center justify-center rounded-lg bg-components-button-primary-bg text-components-button-primary-text group-hover:bg-components-button-primary-bg-hover'>
            <RiAddLine className='h-5 w-5' />
          </div>
          <div className='text-center'>
            <div className='text-sm font-medium text-text-secondary group-hover:text-text-primary'>
              {t('createDevice')}
            </div>
            <div className='text-xs text-text-tertiary'>
              {t('createDeviceDescription')}
            </div>
          </div>
        </div>
      </div>
      
      {showWizardModal && (
        <CreateDeviceWizardModal
          isShow={showWizardModal}
          onClose={() => setShowWizardModal(false)}
          onSuccess={handleSuccess}
        />
      )}
    </>
  )
})

NewDeviceCard.displayName = 'NewDeviceCard'

export default NewDeviceCard