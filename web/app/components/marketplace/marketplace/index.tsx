'use client'

import { RiArrowUpDoubleLine } from '@remixicon/react'
import Loading from '@/app/components/base/loading'
import MarketplaceList from '../marketplace-list'
import type { TemplateMetadata, DriverMetadata } from '@/service/marketplace'

type MarketplaceProps = {
  isMarketplaceArrowVisible?: boolean
  showMarketplacePanel?: () => void
  templates?: TemplateMetadata[]
  drivers?: DriverMetadata[]
  isLoading?: boolean
  showTemplates?: boolean
  showDrivers?: boolean
}

const Marketplace = ({
  isMarketplaceArrowVisible = false,
  showMarketplacePanel,
  templates,
  drivers,
  isLoading = false,
  showTemplates = true,
  showDrivers = true,
}: MarketplaceProps) => {
  return (
    <>
      <div className="sticky bottom-0 flex shrink-0 flex-col bg-background-default-subtle px-12 pb-[14px] pt-2">
        {isMarketplaceArrowVisible && (
          <RiArrowUpDoubleLine
            className="absolute left-1/2 top-2 z-10 h-4 w-4 -translate-x-1/2 cursor-pointer text-text-quaternary"
            onClick={showMarketplacePanel}
          />
        )}
        <div className="pb-3 pt-4">
          <div className="title-2xl-semi-bold bg-gradient-to-r from-[rgba(11,165,236,0.95)] to-[rgba(21,90,239,0.95)] bg-clip-text text-transparent">
            探索更多
          </div>
          <div className="body-md-regular flex items-center text-center text-text-tertiary">
            在市场中发现更多
            <span className="body-md-medium relative ml-1 text-text-secondary after:absolute after:bottom-[1.5px] after:left-0 after:h-2 after:w-full after:bg-text-text-selected after:content-['']">
              设备模板
            </span>
            和
            <span className="body-md-medium relative ml-1 text-text-secondary after:absolute after:bottom-[1.5px] after:left-0 after:h-2 after:w-full after:bg-text-text-selected after:content-['']">
              驱动程序
            </span>
          </div>
        </div>
      </div>
      <div className="mt-[-14px] shrink-0 grow bg-background-default-subtle px-12 pb-2">
        {isLoading && (
          <div className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2">
            <Loading />
          </div>
        )}
        {!isLoading && (
          <MarketplaceList
            templates={templates}
            drivers={drivers}
            showTemplates={showTemplates}
            showDrivers={showDrivers}
          />
        )}
      </div>
    </>
  )
}

export default Marketplace
