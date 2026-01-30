'use client'

import React, { useState, useMemo } from 'react'
import { RiBookOpenLine, RiAddLine } from '@remixicon/react'
import Button from '@/app/components/base/button'
import TabSlider from '@/app/components/base/tab-slider'
import { cn } from '@/utils/classnames'
import TemplateMarketplace from './marketplace'
import MyTemplates from './my-templates'

const TEMPLATE_PAGE_TABS = {
  marketplace: 'marketplace',
  myTemplates: 'myTemplates',
} as const

type TemplatePageTab = typeof TEMPLATE_PAGE_TABS[keyof typeof TEMPLATE_PAGE_TABS]

const TemplatesPage: React.FC = () => {
  const [activeTab, setActiveTab] = useState<TemplatePageTab>(TEMPLATE_PAGE_TABS.marketplace)

  const tabOptions = useMemo(() => [
    {
      value: TEMPLATE_PAGE_TABS.marketplace,
      text: '模板市场',
    },
    {
      value: TEMPLATE_PAGE_TABS.myTemplates,
      text: '我的模板',
    },
  ], [])

  const handleTabChange = (value: string) => {
    setActiveTab(value as TemplatePageTab)
  }

  const isMarketplaceTab = activeTab === TEMPLATE_PAGE_TABS.marketplace
  const isMyTemplatesTab = activeTab === TEMPLATE_PAGE_TABS.myTemplates

  return (
    <div
      className={cn(
        'relative flex grow flex-col overflow-y-auto border-t border-divider-subtle',
        isMyTemplatesTab ? 'rounded-t-xl bg-components-panel-bg' : 'bg-background-body'
      )}
      style={{ scrollbarGutter: 'stable' }}
    >
      {/* 固定头部 */}
      <div
        className={cn(
          'sticky top-0 z-10 flex min-h-[60px] items-center gap-1 self-stretch bg-components-panel-bg px-12 pb-2 pt-4',
          isMarketplaceTab && 'bg-background-body',
        )}
      >
        <div className="flex w-full items-center justify-between">
          <div className="flex-1">
            <TabSlider
              value={activeTab}
              onChange={handleTabChange}
              options={tabOptions}
            />
          </div>
          <div className="flex shrink-0 items-center gap-1">
            {isMarketplaceTab && (
              <>
                <Button
                  variant="ghost"
                  className="text-text-tertiary"
                >
                  模板开发指南
                </Button>
                <Button
                  className="px-3"
                  variant="secondary-accent"
                >
                  <RiBookOpenLine className="mr-1 h-4 w-4" />
                  发布模板
                </Button>
                <div className="mx-1 h-3.5 w-[1px] shrink-0 bg-divider-regular"></div>
              </>
            )}
            {isMyTemplatesTab && (
              <Button
                className="px-3"
                variant="primary"
              >
                <RiAddLine className="mr-1 h-4 w-4" />
                创建模板
              </Button>
            )}
          </div>
        </div>
      </div>

      {/* 内容区域 */}
      {isMarketplaceTab && <TemplateMarketplace />}
      {isMyTemplatesTab && <MyTemplates />}
    </div>
  )
}

export default TemplatesPage