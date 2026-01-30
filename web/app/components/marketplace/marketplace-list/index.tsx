'use client'

import { RiArrowRightSLine, RiArrowRightUpLine } from '@remixicon/react'
import { useState } from 'react'
import Button from '@/app/components/base/button'
import type { TemplateMetadata, DriverMetadata } from '@/service/marketplace'
import { useInstallTemplate, useInstallDriver } from '@/service/marketplace'
import { cn } from '@/utils/classnames'

type MarketplaceListProps = {
  templates?: TemplateMetadata[]
  drivers?: DriverMetadata[]
  showTemplates?: boolean
  showDrivers?: boolean
}

const MarketplaceList = ({
  templates,
  drivers,
  showTemplates = true,
  showDrivers = true,
}: MarketplaceListProps) => {
  const installTemplate = useInstallTemplate()
  const installDriver = useInstallDriver()
  const [installingId, setInstallingId] = useState<string | null>(null)

  const handleInstallTemplate = async (id: string) => {
    setInstallingId(id)
    try {
      await installTemplate.mutateAsync({ id })
    } finally {
      setInstallingId(null)
    }
  }

  const handleInstallDriver = async (id: string) => {
    setInstallingId(id)
    try {
      await installDriver.mutateAsync({ id })
    } finally {
      setInstallingId(null)
    }
  }

  return (
    <>
      {/* 模板分类 */}
      {showTemplates && templates && templates.length > 0 && (
        <div className="py-3">
          <div className="flex items-end justify-between">
            <div>
            </div>
            <div className="system-xs-medium flex cursor-pointer items-center text-text-accent">
              查看更多
              <RiArrowRightSLine className="h-4 w-4" />
            </div>
          </div>
          <div className={cn('mt-2 grid grid-cols-4 gap-3')}>
            {templates.slice(0, 8).map((template) => (
              <div
                key={template.id}
                className="group relative cursor-pointer overflow-hidden rounded-xl border-[0.5px] border-components-panel-border bg-components-panel-on-panel-item-bg shadow-xs hover:bg-components-panel-on-panel-item-bg-hover"
              >
                <div className="p-4 pb-3">
                  <div className="flex items-start">
                    <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-background-section-burn text-2xl">
                      📦
                    </div>
                    <div className="ml-3 flex-1">
                      <div className="system-sm-semibold text-text-primary">{template.name}</div>
                      <div className="system-xs-regular mt-0.5 text-text-tertiary">
                        {template.manufacturer}
                      </div>
                    </div>
                  </div>
                  <div className="system-xs-regular mt-3 line-clamp-2 text-text-secondary">
                    {template.description}
                  </div>
                  <div className="mt-3 flex items-center gap-2">
                    <div className="system-2xs-medium-uppercase rounded bg-util-colors-blue-blue-100 px-1.5 py-0.5 text-util-colors-blue-blue-600">
                      {template.category}
                    </div>
                    <div className="system-xs-regular text-text-tertiary">
                      {template.downloads} 次下载
                    </div>
                  </div>
                </div>
                {/* 悬停时显示的按钮 */}
                <div className="absolute bottom-0 hidden w-full items-center space-x-2 rounded-b-xl bg-gradient-to-tr from-components-panel-on-panel-item-bg to-background-gradient-mask-transparent px-4 pb-4 pt-8 group-hover:flex">
                  <Button
                    variant="primary"
                    className="w-[calc(50%-4px)]"
                    onClick={() => handleInstallTemplate(template.id)}
                    disabled={installingId === template.id}
                  >
                    {installingId === template.id ? '安装中...' : '安装'}
                  </Button>
                  <Button className="w-[calc(50%-4px)] gap-0.5">
                    详情
                    <RiArrowRightUpLine className="ml-1 h-4 w-4" />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* 驱动分类 */}
      {showDrivers && drivers && drivers.length > 0 && (
        <div className="py-3">
          <div className="flex items-end justify-between">
            <div>
            </div>
            <div className="system-xs-medium flex cursor-pointer items-center text-text-accent">
              查看更多
              <RiArrowRightSLine className="h-4 w-4" />
            </div>
          </div>
          <div className={cn('mt-2 grid grid-cols-4 gap-3')}>
            {drivers.slice(0, 8).map((driver) => (
              <div
                key={driver.id}
                className="group relative cursor-pointer overflow-hidden rounded-xl border-[0.5px] border-components-panel-border bg-components-panel-on-panel-item-bg shadow-xs hover:bg-components-panel-on-panel-item-bg-hover"
              >
                <div className="p-4 pb-3">
                  <div className="flex items-start">
                    <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-background-section-burn text-2xl">
                      🔌
                    </div>
                    <div className="ml-3 flex-1">
                      <div className="system-sm-semibold text-text-primary">{driver.name}</div>
                      <div className="system-xs-regular mt-0.5 text-text-tertiary">
                        v{driver.version}
                      </div>
                    </div>
                  </div>
                  <div className="system-xs-regular mt-3 line-clamp-2 text-text-secondary">
                    {driver.description}
                  </div>
                  <div className="mt-3 flex items-center gap-2">
                    <div className="system-2xs-medium-uppercase rounded bg-util-colors-blue-blue-100 px-1.5 py-0.5 text-util-colors-blue-blue-600">
                      {driver.protocol}
                    </div>
                    <div className="system-xs-regular text-text-tertiary">
                      {driver.downloads} 次下载
                    </div>
                  </div>
                </div>
                {/* 悬停时显示的按钮 */}
                <div className="absolute bottom-0 hidden w-full items-center space-x-2 rounded-b-xl bg-gradient-to-tr from-components-panel-on-panel-item-bg to-background-gradient-mask-transparent px-4 pb-4 pt-8 group-hover:flex">
                  <Button
                    variant="primary"
                    className="w-[calc(50%-4px)]"
                    onClick={() => handleInstallDriver(driver.id)}
                    disabled={installingId === driver.id}
                  >
                    {installingId === driver.id ? '安装中...' : '安装'}
                  </Button>
                  <Button className="w-[calc(50%-4px)] gap-0.5">
                    详情
                    <RiArrowRightUpLine className="ml-1 h-4 w-4" />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* 空状态 */}
      {(!templates || templates.length === 0) &&
        (!drivers || drivers.length === 0) && (
          <div className="flex h-64 flex-col items-center justify-center">
            <div className="text-6xl">🏪</div>
            <div className="mt-4 text-lg font-medium text-text-secondary">
              市场内容即将上线
            </div>
            <div className="mt-2 text-sm text-text-tertiary">
              敬请期待更多精彩内容
            </div>
          </div>
        )}
    </>
  )
}

export default MarketplaceList
