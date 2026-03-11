'use client'

import React from 'react'

interface DeviceConfigurationProps {
  deviceId?: string
}

const DeviceConfiguration: React.FC<DeviceConfigurationProps> = ({ deviceId = '' }) => {
  return (
    <div className="h-full overflow-y-auto bg-chatbot-bg px-4 py-6 sm:px-12">
      <div className="bg-components-panel-bg rounded-xl p-6 shadow-sm border border-divider-subtle">
        <h1 className="text-xl font-semibold text-text-primary mb-4">配置</h1>
        <div className="flex items-center justify-center h-64 text-text-tertiary">
          <div className="text-center">
            <div className="text-sm">配置功能即将推出</div>
            <div className="text-xs mt-1">在这里您将能够修改设备的配置参数和设置</div>
          </div>
        </div>
      </div>
    </div>
  )
}

export default DeviceConfiguration
