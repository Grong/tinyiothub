/**
 * 设备卡片头部组件
 * 显示设备图标、名称、产品信息和时间
 */

import React from 'react'
import type { Device } from '@/types'
import { formatTime } from '@/utils/time'
import { useTranslation } from 'react-i18next'
import { getDeviceDisplayName, getDeviceProductName } from '@/lib/device-utils'

interface DeviceHeaderProps {
  device: Device
  statusIcon: React.ReactNode
}

const DeviceHeader: React.FC<DeviceHeaderProps> = ({ device, statusIcon }) => {
  const { t } = useTranslation('device')
  
  const displayName = getDeviceDisplayName(device)
  const productName = getDeviceProductName(device, t('unknownProduct'))

  const editTimeText = React.useMemo(() => {
    const timeString = device.updatedAt || device.createdAt
    if (!timeString) return t('noTimeInfo')
    
    const timeMs = new Date(timeString).getTime()
    return `${t('editedAt')} ${formatTime(timeMs, t('dateTimeFormat'))}`
  }, [device.updatedAt, device.createdAt, t])

  return (
    <div className='flex h-[66px] shrink-0 grow-0 items-center gap-3 px-[14px] pb-3 pt-[14px]'>
      <div className='relative shrink-0'>
        <div className='flex h-10 w-10 items-center justify-center rounded-lg bg-components-panel-bg-alt border border-divider-subtle'>
          {statusIcon}
        </div>
      </div>
      <div className='w-0 grow py-[1px]'>
        <div className='flex items-center text-sm font-semibold leading-5 text-text-secondary'>
          <div className='truncate' title={displayName}>{displayName}</div>
        </div>
        <div className='flex items-center gap-1 text-[10px] font-medium leading-[18px] text-text-tertiary'>
          <div className='truncate' title={productName}>{productName}</div>
          <div>·</div>
          <div className='truncate' title={editTimeText}>{editTimeText}</div>
        </div>
      </div>
    </div>
  )
}

export default React.memo(DeviceHeader)
