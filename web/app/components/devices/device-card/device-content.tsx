/**
 * 设备卡片内容组件
 * 显示设备描述和属性
 */

import React from 'react'
import { useTranslation } from 'react-i18next'
import type { Device } from '@/types'
import { DISPLAY_LIMITS } from '@/lib/constants'

interface DeviceContentProps {
  device: Device
}

const DeviceContent: React.FC<DeviceContentProps> = ({ device }) => {
  const { t } = useTranslation('device')

  return (
    <div className='title-wrapper h-[90px] px-[14px] text-xs leading-normal text-text-tertiary'>
      <div className='line-clamp-2' title={device.description}>
        {device.description || t('messages.noDescription')}
      </div>
      {device.properties && device.properties.length > 0 && (
        <div className='mt-2'>
          <div className='text-xs text-text-quaternary mb-1'>{t('properties')}:</div>
          <div className='flex flex-wrap gap-1'>
            {device.properties.slice(0, DISPLAY_LIMITS.MAX_VISIBLE_PROPERTIES).map((prop: any, index: number) => (
              <span 
                key={index} 
                className='inline-flex items-center px-1.5 py-0.5 rounded text-xs bg-components-badge-bg-blue-soft text-text-accent'
              >
                {prop.name}: {prop.value}
              </span>
            ))}
            {device.properties.length > DISPLAY_LIMITS.MAX_VISIBLE_PROPERTIES && (
              <span className='inline-flex items-center px-1.5 py-0.5 rounded text-xs bg-components-badge-bg-gray-soft text-text-tertiary'>
                +{device.properties.length - DISPLAY_LIMITS.MAX_VISIBLE_PROPERTIES}
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

export default React.memo(DeviceContent)
