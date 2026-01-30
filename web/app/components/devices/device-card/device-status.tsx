/**
 * 设备状态组件
 * 显示设备状态徽章
 */

import React from 'react'
import cn from '@/utils/classnames'
import { useTranslation } from 'react-i18next'
import { getDeviceStatusColor } from '@/lib/device-utils'
import type { DeviceStatus } from '@/lib/device-utils'

interface DeviceStatusProps {
  status: DeviceStatus
}

const DeviceStatus: React.FC<DeviceStatusProps> = ({ status }) => {
  const { t } = useTranslation('device')

  return (
    <div className='flex h-5 w-5 shrink-0 items-center justify-center'>
      <span className={cn(
        'inline-flex items-center px-2 py-1 rounded-full text-xs font-medium',
        getDeviceStatusColor(status)
      )}>
        {t(`status.${status}`)}
      </span>
    </div>
  )
}

export default React.memo(DeviceStatus)
