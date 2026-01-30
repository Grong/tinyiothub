'use client'

import React, { useMemo } from 'react'
import { RiSearchLine } from '@remixicon/react'
import Button from '@/app/components/base/button'
import Input from '@/app/components/base/input'
import TemplateCard from '@/app/components/templates/marketplace/template-card'
import type { ProcessedDeviceTemplate } from '@/service/templates'

interface TemplateSelectionPageProps {
  searchQuery: string
  onSearchChange: (query: string) => void
  templates: ProcessedDeviceTemplate[]
  isLoading: boolean
  onTemplateSelect: (template: ProcessedDeviceTemplate) => void
  onClose: () => void
}

const TemplateSelectionPage: React.FC<TemplateSelectionPageProps> = ({
  searchQuery,
  onSearchChange,
  templates,
  isLoading,
  onTemplateSelect,
  onClose,
}) => {
  // 按分类分组模板
  const groupedTemplates = useMemo(() => {
    const groups: Record<string, ProcessedDeviceTemplate[]> = {}
    templates.forEach(template => {
      const category = template.category || 'others'
      if (!groups[category]) {
        groups[category] = []
      }
      groups[category].push(template)
    })
    return groups
  }, [templates])

  const categoryLabels: Record<string, string> = {
    sensors: '传感器',
    controllers: '控制器',
    cameras: '摄像头',
    gateways: '网关',
    others: '其他'
  }

  return (
    <div className="flex h-full flex-col">
      {/* 固定头部 */}
      <div className="flex-shrink-0 px-12 py-6">
        <div className="mx-auto max-w-4xl text-center">
          <p className="system-md-regular text-text-secondary">
            选择一个设备模板来快速创建和配置您的IoT设备
          </p>
        </div>
        
        {/* 搜索框 */}
        <div className="mt-6">
          <div className="mx-auto w-[640px] shrink-0">
            <div className="relative">
              <Input
                className="w-full pl-10"
                placeholder="搜索设备模板..."
                value={searchQuery}
                onChange={(e) => onSearchChange(e.target.value)}
              />
              <RiSearchLine className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-text-tertiary" />
            </div>
          </div>
        </div>
      </div>

      {/* 模板列表 - 全宽显示 */}
      <div className="flex-1 overflow-y-auto px-12" style={{ scrollbarGutter: 'stable' }}>
        {isLoading ? (
          <div className="flex items-center justify-center py-12">
            <div className="text-text-tertiary">加载中...</div>
          </div>
        ) : templates.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12">
            <div className="text-6xl">📦</div>
            <div className="mt-4 text-lg font-medium text-text-secondary">
              没有找到匹配的模板
            </div>
            <div className="mt-2 text-sm text-text-tertiary">
              尝试调整搜索条件或浏览其他分类
            </div>
          </div>
        ) : (
          <div className="space-y-8 pb-8 pt-6">
            {Object.entries(groupedTemplates).map(([category, categoryTemplates]) => (
              <div key={category}>
                <div className="mb-4 flex items-center">
                  <div className="title-xl-semi-bold text-text-primary">
                    {categoryLabels[category] || category}
                  </div>
                  <div className="mx-3 h-3.5 w-[1px] bg-divider-regular"></div>
                  <div className="system-xs-regular text-text-tertiary">
                    {categoryTemplates.length} 个模板
                  </div>
                </div>
                <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                  {categoryTemplates.map((template) => (
                    <TemplateCard
                      key={template.id}
                      template={template}
                      onUse={onTemplateSelect}
                    />
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* 固定底部 */}
      <div className="flex-shrink-0 px-12 py-4">
        <div className="flex items-center justify-between">
          <div className="system-xs-regular text-text-tertiary">
            选择一个模板开始创建设备
          </div>
          <Button onClick={onClose}>取消</Button>
        </div>
      </div>
    </div>
  )
}

export default TemplateSelectionPage