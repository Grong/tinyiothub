'use client'

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { RiBookOpenLine } from '@remixicon/react'
import Button from '@/app/components/base/button'
import TabSlider from '@/app/components/base/tab-slider'
import Input from '@/app/components/base/input'
import { cn } from '@/utils/classnames'
import { useTemplates } from '@/service/templates'
import { useAllDrivers, type Driver } from '@/service/drivers'
import { useMarketplaceTemplates, useMarketplaceDrivers } from '@/service/marketplace'
import Loading from '@/app/components/base/loading'
import TemplateCard from './installed-templates/template-card'
import DriverCard from './installed-drivers/driver-card'
import Marketplace from './marketplace'

const MARKETPLACE_TABS = {
  templates: 'templates',
  drivers: 'drivers',
} as const

type MarketplaceTab = typeof MARKETPLACE_TABS[keyof typeof MARKETPLACE_TABS]

const ProviderList = () => {
  const containerRef = useRef<HTMLDivElement>(null)
  const marketplaceTailRef = useRef<HTMLDivElement>(null)
  
  const [activeTab, setActiveTab] = useState<MarketplaceTab>(MARKETPLACE_TABS.templates)
  const [keywords, setKeywords] = useState<string>('')
  
  // 已安装的数据
  const { data: templates, isLoading: isTemplatesLoading } = useTemplates()
  const { data: driversData, isLoading: isDriversLoading } = useAllDrivers()
  
  // 市场数据
  const { data: marketplaceTemplates, isLoading: isMarketTemplatesLoading } = useMarketplaceTemplates()
  const { data: marketplaceDrivers, isLoading: isMarketDriversLoading } = useMarketplaceDrivers()

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

  const handleKeywordsChange = (value: string) => {
    setKeywords(value)
  }

  const isTemplatesTab = activeTab === MARKETPLACE_TABS.templates
  const isDriversTab = activeTab === MARKETPLACE_TABS.drivers

  const isLoading = isTemplatesTab ? isTemplatesLoading : isDriversLoading
  const currentList = isTemplatesTab ? templates : allDrivers
  const isEmpty = !currentList || currentList.length === 0

  // 过滤市场数据
  const filteredMarketplaceTemplates = useMemo(() => {
    if (!marketplaceTemplates) return []
    if (!keywords) return marketplaceTemplates
    return marketplaceTemplates.filter(t => 
      t.name.toLowerCase().includes(keywords.toLowerCase()) ||
      t.description.toLowerCase().includes(keywords.toLowerCase())
    )
  }, [marketplaceTemplates, keywords])

  const filteredMarketplaceDrivers = useMemo(() => {
    if (!marketplaceDrivers) return []
    if (!keywords) return marketplaceDrivers
    return marketplaceDrivers.filter(d => 
      d.name.toLowerCase().includes(keywords.toLowerCase()) ||
      d.description.toLowerCase().includes(keywords.toLowerCase())
    )
  }, [marketplaceDrivers, keywords])

  const showMarketplacePanel = useCallback(() => {
    containerRef.current?.scrollTo({
      top: marketplaceTailRef.current
        ? marketplaceTailRef.current?.offsetTop - 80
        : 0,
      behavior: 'smooth',
    })
  }, [marketplaceTailRef])

  const [isMarketplaceArrowVisible, setIsMarketplaceArrowVisible] = useState(true)
  const onContainerScroll = useCallback(() => {
    if (containerRef.current && marketplaceTailRef.current) {
      setIsMarketplaceArrowVisible(
        containerRef.current.scrollTop < (marketplaceTailRef.current?.offsetTop - 80)
      )
    }
  }, [])

  useEffect(() => {
    const container = containerRef.current
    if (container)
      container.addEventListener('scroll', onContainerScroll)

    return () => {
      if (container)
        container.removeEventListener('scroll', onContainerScroll)
    }
  }, [onContainerScroll])

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
            <div className="flex items-center gap-2">
              <Input
                showLeftIcon
                showClearIcon
                wrapperClassName="w-[200px]"
                value={keywords}
                onChange={e => handleKeywordsChange(e.target.value)}
                onClear={() => handleKeywordsChange('')}
                placeholder="搜索..."
              />
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
          <Marketplace
            isMarketplaceArrowVisible={isMarketplaceArrowVisible}
            showMarketplacePanel={showMarketplacePanel}
            templates={isTemplatesTab ? filteredMarketplaceTemplates : undefined}
            drivers={isDriversTab ? filteredMarketplaceDrivers : undefined}
            isLoading={isTemplatesTab ? isMarketTemplatesLoading : isMarketDriversLoading}
            showTemplates={isTemplatesTab}
            showDrivers={isDriversTab}
          />
        </div>
      </div>
    </>
  )
}

ProviderList.displayName = 'MarketplaceProviderList'
export default ProviderList
