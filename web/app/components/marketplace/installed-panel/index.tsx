'use client'

import React from 'react'
import { useTemplates } from '@/service/templates'
import { useDrivers, type Driver } from '@/service/drivers'
import Loading from '@/app/components/base/loading'
import TemplateCard from '../installed-templates/template-card'
import DriverCard from '../installed-drivers/driver-card'

const InstalledPanel: React.FC = () => {
  const { data: templates, isLoading: isTemplatesLoading } = useTemplates()
  const { data: driversData, isLoading: isDriversLoading } = useDrivers()

  // 合并静态驱动和动态驱动
  const allDrivers = React.useMemo(() => {
    if (!driversData) return []
    return [...(driversData.staticDrivers || []), ...(driversData.dynamic || [])]
  }, [driversData])

  const isLoading = isTemplatesLoading || isDriversLoading

  return (
    <div className="flex grow flex-col px-12 pb-6">
      {isLoading && (
        <div className="flex h-64 items-center justify-center">
          <Loading />
        </div>
      )}

      {!isLoading && (
        <>
          {/* 模板区域 */}
          <div className="mb-8">
            <div className="mb-4 flex items-center justify-between">
              <h2 className="text-lg font-semibold text-text-primary">
                模板 ({templates?.length || 0})
              </h2>
            </div>
            {(!templates || templates.length === 0) ? (
              <div className="flex flex-col items-center justify-center rounded-xl border border-divider-subtle bg-background-default py-12">
                <div className="text-6xl">📦</div>
                <div className="mt-4 text-lg font-medium text-text-secondary">
                  暂无已安装模板
                </div>
                <div className="mt-2 text-sm text-text-tertiary">
                  前往市场安装设备模板
                </div>
              </div>
            ) : (
              <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                {templates.map((template) => (
                  <TemplateCard
                    key={template.id}
                    template={template}
                  />
                ))}
              </div>
            )}
          </div>

          {/* 驱动区域 */}
          <div>
            <div className="mb-4 flex items-center justify-between">
              <h2 className="text-lg font-semibold text-text-primary">
                驱动 ({allDrivers.length})
              </h2>
            </div>
            {allDrivers.length === 0 ? (
              <div className="flex flex-col items-center justify-center rounded-xl border border-divider-subtle bg-background-default py-12">
                <div className="text-6xl">🔌</div>
                <div className="mt-4 text-lg font-medium text-text-secondary">
                  暂无已安装驱动
                </div>
                <div className="mt-2 text-sm text-text-tertiary">
                  前往市场安装驱动程序
                </div>
              </div>
            ) : (
              <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                {allDrivers.map((driver: Driver) => (
                  <DriverCard
                    key={driver.name}
                    driver={driver}
                  />
                ))}
              </div>
            )}
          </div>
        </>
      )}
    </div>
  )
}

export default InstalledPanel
