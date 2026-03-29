'use client'

import React from 'react'
import DriverCard from '@/app/components/marketplace/driver-marketplace/driver-card'
import type { DriverMetadata } from '@/service/marketplace'

interface DriverGridProps {
  drivers: DriverMetadata[]
  isLoading?: boolean
}

export default function DriverGrid({ drivers, isLoading }: DriverGridProps) {
  if (isLoading) {
    return (
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 2xl:grid-cols-5 gap-4">
        {[...Array(8)].map((_, i) => (
          <div key={i} className="glass-card h-48 animate-pulse" />
        ))}
      </div>
    )
  }

  if (drivers.length === 0) {
    return (
      <div className="glass-card p-12 text-center">
        <p className="text-gray-500">暂无驱动</p>
      </div>
    )
  }

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 2xl:grid-cols-5 gap-4">
      {drivers.map((driver) => (
        <DriverCard key={driver.id} driver={driver} />
      ))}
    </div>
  )
}
