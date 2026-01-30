'use client'

import React from 'react'
import { useDrivers } from '@/service/drivers'
import Loading from '@/app/components/base/loading'
import DriverCard from './driver-card'

const InstalledDrivers: React.FC = () => {
  const { data: drivers, isLoading } = useDrivers()

  if (isLoading) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Loading />
      </div>
    )
  }

  if (!drivers || drivers.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12">
        <div className="text-6xl">🔌</div>
        <div className="mt-4 text-lg font-medium text-text-secondary">
          暂无已安装驱动
        </div>
        <div className="mt-2 text-sm text-text-tertiary">
          前往驱动市场安装驱动程序
        </div>
      </div>
    )
  }

  return (
    <div className="px-12 py-6">
      <div className="mb-4">
        <div className="title-xl-semi-bold text-text-primary">
          已安装 {drivers.length} 个驱动
        </div>
      </div>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
        {drivers.map((driver) => (
          <DriverCard
            key={driver.name}
            driver={driver}
          />
        ))}
      </div>
    </div>
  )
}

export default InstalledDrivers
