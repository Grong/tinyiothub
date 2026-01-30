'use client'

import React from 'react'
import TemplateMarketplace from '../template-marketplace'
import DriverMarketplace from '../driver-marketplace'

const MarketplacePanel: React.FC = () => {
  return (
    <>
      {/* 设备模板市场 */}
      <div className="px-12 pb-8">
        <div className="mb-4">
          <h2 className="text-lg font-semibold text-text-primary">
            设备模板
          </h2>
        </div>
        <TemplateMarketplace />
      </div>

      {/* 驱动程序市场 */}
      <div className="px-12 pb-6">
        <div className="mb-4">
          <h2 className="text-lg font-semibold text-text-primary">
            驱动程序
          </h2>
        </div>
        <DriverMarketplace />
      </div>
    </>
  )
}

export default MarketplacePanel
