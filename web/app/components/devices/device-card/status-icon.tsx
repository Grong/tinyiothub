/**
 * 设备状态图标组件
 */

import React from 'react'
import { RiWifiLine, RiWifiOffLine, RiAlarmWarningLine, RiSettings3Line } from '@remixicon/react'
import type { DeviceStatus } from '@/lib/device-utils'

interface StatusIconProps {
  status: DeviceStatus
}

const StatusIcon: React.FC<StatusIconProps> = ({ status }) => {
  switch (status) {
    case 'online':
      return <RiWifiLine className='h-4 w-4 text-text-success' />
    case 'error':
      return <RiAlarmWarningLine className='h-4 w-4 text-text-destructive' />
    case 'maintenance':
      return <RiSettings3Line className='h-4 w-4 text-text-warning' />
    default:
      return <RiWifiOffLine className='h-4 w-4 text-text-tertiary' />
  }
}

export default React.memo(StatusIcon)
