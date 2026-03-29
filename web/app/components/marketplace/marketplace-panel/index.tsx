'use client'

import React from 'react'
import TemplateMarketplace from '../template-marketplace'
import DriverMarketplace from '../driver-marketplace'

const MarketplacePanel: React.FC = () => {
  return (
    <>
      {/* 设备模板市场 */}
      <div className="px-12 pb-8">
        <div className="glass rounded-2xl p-6 mb-4">
          <h2 className="text-lg font-semibold text-gray-900">
            设备模板
          </h2>
          <p className="text-sm text-gray-500 mt-1">从市场安装设备模板，快速接入设备</p>
        </div>
        <TemplateMarketplace />
      </div>

      {/* 驱动程序市场 */}
      <div className="px-12 pb-6">
        <div className="glass rounded-2xl p-6 mb-4">
          <h2 className="text-lg font-semibold text-gray-900">
            驱动程序
          </h2>
          <p className="text-sm text-gray-500 mt-1">从市场安装驱动程序，支持多种协议</p>
        </div>
        <DriverMarketplace />
      </div>
    </>
  )
}

export default MarketplacePanel
