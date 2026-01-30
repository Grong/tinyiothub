'use client'

import React, { useState, useMemo, useRef } from 'react'
import { RiBookOpenLine } from '@remixicon/react'
import Button from '@/app/components/base/button'
import TabSlider from '@/app/components/base/tab-slider'
import { useTemplates } from '@/service/templates'
import { useAllDrivers, type Driver } from '@/service/drivers'
import Loading from '@/app/components/base/loading'
import TemplateCard from './installed-templates/template-card'
import DriverCard from './installed-drivers/driver-card'

const MARKETPLACE_TABS = {
  templates: 'templates',
  drivers: 'drivers',
} as const

type MarketplaceTab = typeof MARKETPLACE_TABS[keyof typeof MARKETPLACE_TABS]

export type MarketplacePageProps = {
  templateMarketplace: React.ReactNode
  driverMarketplace: React.ReactNode
}

const MarketplacePage = ({
  templateMarketplace,
  driverMarketplace,
}: MarketplacePageProps) => {
  const containerRef = useRef<HTMLDivElement>(null)
  const marketplaceTailRef = useRef<HTMLDivElement>(null)
  
  const [activeTab, setActiveTab] = useState<MarketplaceTab>(MARKETPLACE_TABS.templates)
  
  const { data: templates, isLoading: isTemplatesLoading } = useTemplates()
  const { data: driversData, isLoading: isDriversLoading } = useAllDrivers()

  // 合并静态驱动和动态驱动
  const allDrivers = useMemo(() => {
    if (!driversData) return []
    return [...(driversData.staticDrivers || []), ...(driversData.dynamic || [])]
  }, [driversData])

  const tabOptions = useMemo(() => [
    {
      value: MARKETPLACE_TABS.templates,
      text: '模板',
    },
    {
      value: MARKETPLACE_TABS.drivers,
      text: '驱动',
    },
  ], [])

  const handleTabChange = (value: string) => {
    setActiveTab(value as MarketplaceTab)
  }

  const isTemplatesTab = activeTab === MARKETPLACE_TABS.templates
  const isDriversTab = activeTab === MARKETPLACE_TABS.drivers

  const isLoading = isTemplatesTab ? isTemplatesLoading : isDriversLoading
  const currentList = isTemplatesTab ? templates : allDrivers
  const isEmpty = !currentList || currentList.length === 0

  // 调试信息
  console.log('MarketplacePage Debug:', {
    activeTab,
    isTemplatesTab,
    isDriversTab,
    isLoading,
    templates,
    allDrivers,
    isEmpty,
  })

  return (
    <>
      <div className="relative flex h-0 shrink-0 grow overflow-hidden">
        <div
          ref={containerRef}
          className="relative flex grow flex-col overflow-y-auto bg-background-body"
        >
          {/* 固定头部 */}
          <div className="sticky top-0 z-10 flex flex-wrap items-center justify-between gap-y-2 bg-background-body px-12 pb-2 pt-4 leading-[56px]">
            <TabSlider
              value={activeTab}
              onChange={handleTabChange}
              options={tabOptions}
            />
            <div className="flex shrink-0 items-center gap-1">
              <Button
                variant="ghost"
                className="text-text-tertiary"
              >
                开发指南
              </Button>
              <Button
                className="px-3"
                variant="secondary-accent"
              >
                <RiBookOpenLine className="mr-1 h-4 w-4" />
                发布到市场
              </Button>
            </div>
          </div>

          {/* 已安装内容区域 */}
          {isLoading && (
            <div className="flex h-64 items-center justify-center">
              <Loading />
            </div>
          )}

          {!isLoading && !isEmpty && (
            <div className="relative grid shrink-0 grid-cols-1 content-start gap-4 px-12 pb-4 pt-2 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4">
              {isTemplatesTab && templates?.map((template) => (
                <TemplateCard
                  key={template.id}
                  template={template}
                />
              ))}
              {isDriversTab && allDrivers.map((driver: Driver) => (
                <DriverCard
                  key={driver.name}
                  driver={driver}
                />
              ))}
            </div>
          )}

          {!isLoading && isEmpty && (
            <div className="h-[224px] shrink-0 px-12">
              <div className="flex h-full flex-col items-center justify-center">
                <div className="text-6xl">{isTemplatesTab ? '📦' : '🔌'}</div>
                <div className="mt-4 text-lg font-medium text-text-secondary">
                  暂无已安装{isTemplatesTab ? '模板' : '驱动'}
                </div>
              </div>
            </div>
          )}

          {/* 市场分隔标记 */}
          <div ref={marketplaceTailRef} />

          {/* 市场内容 - 根据当前 Tab 显示对应市场 */}
          {isTemplatesTab && templateMarketplace}
          {isDriversTab && driverMarketplace}
        </div>
      </div>
    </>
  )
}

export default MarketplacePage
