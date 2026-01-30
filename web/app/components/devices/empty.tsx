'use client'

import { useTranslation } from 'react-i18next'
import { RiDeviceLine } from '@remixicon/react'

const Empty = () => {
  const { t } = useTranslation('device')

  return (
    <div className='col-span-full flex h-[240px] items-center justify-center'>
      <div className='flex flex-col items-center gap-4'>
        <div className='flex h-16 w-16 items-center justify-center rounded-full bg-components-panel-bg-alt'>
          <RiDeviceLine className='h-8 w-8 text-text-quaternary' />
        </div>
        <div className='text-center'>
          <div className='text-sm font-medium text-text-secondary'>
            {t('device.noDevices')}
          </div>
          <div className='text-xs text-text-tertiary'>
            {t('device.noDevicesDescription')}
          </div>
        </div>
      </div>
    </div>
  )
}

export default Empty