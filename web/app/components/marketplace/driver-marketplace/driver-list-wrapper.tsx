'use client'

import React from 'react'
import Loading from '@/app/components/base/loading'
import { useDriverMarketplaceContext } from './context'
import SortDropdown from './sort-dropdown'
import DriverList from './driver-list'

const DriverListWrapper: React.FC = () => {
  const { 
    drivers, 
    totalCount, 
    isLoading, 
    error,
    hasMore,
    loadMore 
  } = useDriverMarketplaceContext()

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center py-12">
        <div className="text-text-destructive">加载驱动失败</div>
        <div className="mt-2 text-sm text-text-tertiary">{error.message}</div>
      </div>
    )
  }

  return (
    <div
      style={{ scrollbarGutter: 'stable' }}
      className="relative flex grow flex-col bg-background-default-subtle px-12 py-2"
    >
      <div className="mb-4 flex items-center pt-3">
        <div className="title-xl-semi-bold text-text-primary">
          找到 {totalCount} 个驱动
        </div>
        <div className="mx-3 h-3.5 w-[1px] bg-divider-regular"></div>
        <SortDropdown />
      </div>

      {isLoading && drivers.length === 0 && (
        <div className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2">
          <Loading />
        </div>
      )}

      {!isLoading || drivers.length > 0 ? (
        <DriverList 
          drivers={drivers}
          hasMore={hasMore}
          onLoadMore={loadMore}
          isLoading={isLoading}
        />
      ) : null}
    </div>
  )
}

export default DriverListWrapper
