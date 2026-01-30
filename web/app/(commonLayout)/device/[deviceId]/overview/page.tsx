'use client'

import React from 'react'
import { useParams } from 'next/navigation'
import DeviceOverviewContent from './device-overview-content'

const DeviceOverview = () => {
  const params = useParams()
  const deviceId = params.deviceId as string

  return (
    <div className="h-full overflow-y-auto bg-chatbot-bg px-4 py-6 sm:px-12">
      <DeviceOverviewContent deviceId={deviceId} />
    </div>
  )
}

export default DeviceOverview
