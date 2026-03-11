'use client'

import React from 'react'
import DeviceMonitoringDashboard from '@/app/components/devices/monitoring/device-monitoring-dashboard'

interface DeviceMonitoringProps {
  deviceId?: string
}

const DeviceMonitoring: React.FC<DeviceMonitoringProps> = ({ deviceId = '' }) => {
  return (
    <div className="h-full overflow-y-auto bg-chatbot-bg px-4 py-6 sm:px-12">
      <DeviceMonitoringDashboard deviceId={deviceId} />
    </div>
  )
}

export default DeviceMonitoring
