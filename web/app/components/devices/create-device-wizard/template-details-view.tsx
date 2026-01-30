'use client'

import React, { useState } from 'react'
import { type ProcessedDeviceTemplate } from '@/service/templates'
import { useLocalizedText } from '@/utils/i18n-template'
import cn from '@/utils/classnames'

interface TemplateDetailsViewProps {
  template: ProcessedDeviceTemplate
}

// 获取分类图标
const getCategoryIcon = (category: string) => {
  const icons: Record<string, string> = {
    sensors: '🌡️',
    controllers: '🎛️',
    cameras: '📷',
    gateways: '🌐',
    default: '📦',
  }
  return icons[category] || icons.default
}

const TemplateDetailsView: React.FC<TemplateDetailsViewProps> = ({ template }) => {
  const [activeTab, setActiveTab] = useState<'properties' | 'commands'>('properties')
  const getLocalizedText = useLocalizedText()
  const templateDisplayName = getLocalizedText(template.displayName || {}, template.name)
  const templateDescription = getLocalizedText(template.description || {}, '')

  return (
    <div className="flex h-full flex-col">

      {/* 固定的标签页导航 */}
      <div className="flex-shrink-0 border-b border-divider-subtle bg-components-panel-bg">
        <div className="flex">
          <button
            className={cn(
              "px-4 py-3 text-sm font-medium transition-colors",
              activeTab === 'properties'
                ? "border-b-2 border-components-button-primary-border text-text-primary"
                : "text-text-secondary hover:text-text-primary"
            )}
            onClick={() => setActiveTab('properties')}
          >
            属性 ({template.properties?.length || 0})
          </button>
          <button
            className={cn(
              "px-4 py-3 text-sm font-medium transition-colors",
              activeTab === 'commands'
                ? "border-b-2 border-components-button-primary-border text-text-primary"
                : "text-text-secondary hover:text-text-primary"
            )}
            onClick={() => setActiveTab('commands')}
          >
            命令 ({template.commands?.length || 0})
          </button>
        </div>
      </div>

      {/* 可滚动的标签页内容 */}
      <div className="flex-1 overflow-y-auto">
        {activeTab === 'properties' && (
          <TemplatePropertiesList template={template} />
        )}
        {activeTab === 'commands' && (
          <TemplateCommandsList template={template} />
        )}
      </div>
    </div>
  )
}

// 模板属性列表组件
const TemplatePropertiesList: React.FC<{ template: ProcessedDeviceTemplate }> = ({ template }) => {
  const getLocalizedText = useLocalizedText()

  if (!template.properties || template.properties.length === 0) {
    return (
      <div className="flex h-32 items-center justify-center text-text-tertiary">
        <div className="text-center">
          <div className="text-2xl opacity-50">📊</div>
          <div className="mt-2">此模板没有定义属性</div>
        </div>
      </div>
    )
  }

  return (
    <div className="p-4">
      <div className="space-y-2">
        {template.properties.map((property, index) => {
          const displayName = getLocalizedText(property.displayName || {}, property.name)
          const description = getLocalizedText(property.description || {}, '')
          
          return (
            <div
              key={`${property.name}-${index}`}
              className="flex items-center justify-between rounded-lg border border-components-panel-border bg-components-panel-on-panel-item-bg px-4 py-3 hover:bg-components-panel-on-panel-item-bg-hover"
            >
              {/* 左侧：名称和描述 */}
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="font-medium text-text-primary">{displayName}</span>
                  {description && (
                    <span className="truncate text-xs text-text-tertiary" title={description}>
                      - {description}
                    </span>
                  )}
                </div>
              </div>
              
              {/* 右侧：标签和属性信息 */}
              <div className="flex items-center gap-2 text-xs">
                <div className="rounded bg-components-badge-bg-blue px-2 py-0.5 text-components-badge-text-blue">
                  {property.dataType}
                </div>
                {property.unit && (
                  <div className="rounded bg-components-badge-bg-gray px-2 py-0.5 text-components-badge-text-gray">
                    {property.unit}
                  </div>
                )}
                {property.isReadOnly && (
                  <div className="rounded bg-components-badge-bg-orange px-2 py-0.5 text-components-badge-text-orange">
                    只读
                  </div>
                )}
                {property.isRequired && (
                  <div className="rounded bg-components-badge-bg-red px-2 py-0.5 text-components-badge-text-red">
                    必需
                  </div>
                )}
                {property.defaultValue !== undefined && (
                  <span className="text-text-tertiary">默认: {String(property.defaultValue)}</span>
                )}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

// 模板命令列表组件
const TemplateCommandsList: React.FC<{ template: ProcessedDeviceTemplate }> = ({ template }) => {
  const getLocalizedText = useLocalizedText()

  if (!template.commands || template.commands.length === 0) {
    return (
      <div className="flex h-32 items-center justify-center text-text-tertiary">
        <div className="text-center">
          <div className="text-2xl opacity-50">⚡</div>
          <div className="mt-2">此模板没有定义命令</div>
        </div>
      </div>
    )
  }

  return (
    <div className="p-4">
      <div className="space-y-2">
        {template.commands.map((command, index) => {
          const displayName = getLocalizedText(command.displayName || {}, command.name)
          const description = getLocalizedText(command.description || {}, '')
          
          return (
            <div
              key={`${command.name}-${index}`}
              className="flex items-center justify-between rounded-lg border border-components-panel-border bg-components-panel-on-panel-item-bg px-4 py-3 hover:bg-components-panel-on-panel-item-bg-hover"
            >
              {/* 左侧：名称和描述 */}
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="font-medium text-text-primary">{displayName}</span>
                  {description && (
                    <span className="truncate text-xs text-text-tertiary" title={description}>
                      - {description}
                    </span>
                  )}
                </div>
              </div>
              
              {/* 右侧：标签和参数信息 */}
              <div className="flex items-center gap-2 text-xs">
                <div className="rounded bg-components-badge-bg-green px-2 py-0.5 text-components-badge-text-green">
                  命令
                </div>
                {command.isRequired && (
                  <div className="rounded bg-components-badge-bg-red px-2 py-0.5 text-components-badge-text-red">
                    必需
                  </div>
                )}
                {command.parameters && (
                  <span className="truncate text-text-tertiary" title={command.parameters}>
                    参数: {command.parameters}
                  </span>
                )}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

export default TemplateDetailsView