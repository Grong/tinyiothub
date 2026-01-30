'use client'

import React from 'react'
import { useParams } from 'next/navigation'
import DeviceMonitoringDashboard from '@/app/components/devices/monitoring/device-monitoring-dashboard'

const DeviceMonitoring = () => {
  const params = useParams()
  const deviceId = params.deviceId as string

  return (
    <div className="h-full overflow-y-auto bg-chatbot-bg px-4 py-6 sm:px-12">
      <DeviceMonitoringDashboard deviceId={deviceId} />
    </div>
  )
}

export default DeviceMonitoring
